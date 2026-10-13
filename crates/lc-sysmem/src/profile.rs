//! The compute profile: what this machine has (logical and physical cores, performance and
//! efficiency cores, RAM), detected once at start-up and then frozen, and the one compute budget
//! `C` everything parallel derives from (`docs/wip/REVIEW-threading-policy-fable.md` §2).
//!
//! Only floors, ceilings and fractions: nothing here is tuned to a particular machine. When a
//! detection fails the documented fallback is used and named in the profile's `fallbacks` (and the
//! start-up log line): logical 4, physical = logical (never "half"), RAM 8 GB, available = total / 2.

use std::fmt;
use std::sync::{Mutex, PoisonError};

/// The logical CPU count used when the OS can't tell us.
pub const FALLBACK_LOGICAL: usize = 4;
/// RAM assumed when the OS can't tell us.
pub const FALLBACK_RAM: u64 = 8 << 30;
/// Share of RAM heavy work may plan for when the user hasn't set one (Photoshop's default is 70 %).
pub const AUTO_MEMORY_PERCENT: u8 = 70;

/// What the hardware looks like (no overrides applied).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hardware {
    /// CPUs this process may run on (`available_parallelism`: honours cgroup quotas and affinity).
    pub logical: usize,
    /// Physical cores (SMT siblings counted once; P and E cores both count).
    pub physical: usize,
    /// Performance cores, on hybrid chips only.
    pub perf_cores: Option<usize>,
    /// Efficiency cores, on hybrid chips only.
    pub eff_cores: Option<usize>,
    /// Installed RAM in bytes.
    pub ram_total: u64,
    /// Which values are fallbacks because detection failed (`"logical"`, `"physical"`, `"ram"`).
    pub fallbacks: Vec<&'static str>,
}

/// Where the thread count came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadSource {
    /// Derived from the hardware.
    Auto,
    /// The user's Preferences › Performance setting.
    Setting,
    /// `LOCAL_IMAGE_THREADS` (testing).
    LocalImageThreads,
    /// `RAYON_NUM_THREADS` (testing).
    RayonNumThreads,
}

/// Background work priority (Preferences › Performance).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundPriority {
    /// Batch work runs at a lower OS priority, so the UI wins contested time slices (default).
    #[default]
    Low,
    /// No priority change: batch work competes with the UI as an equal.
    Normal,
}

/// Optional user overrides over the automatic values (Preferences › Performance). `None` = Auto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    /// Processing threads (clamped to 1..=logical).
    pub threads: Option<usize>,
    pub background_priority: BackgroundPriority,
    /// Memory heavy work may plan for, in percent of RAM (clamped to 10..=90).
    pub memory_percent: Option<u8>,
}

/// The frozen compute profile: hardware plus the derived budget and the overrides in effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeProfile {
    pub hardware: Hardware,
    /// The compute budget `C` from the hardware alone.
    pub threads_auto: usize,
    /// The compute threads in effect (the global rayon pool's size when it was built here).
    pub threads: usize,
    pub threads_from: ThreadSource,
    pub background_priority: BackgroundPriority,
    /// The user's memory share, if set.
    pub memory_percent_override: Option<u8>,
    /// Whether [`init`] built the rayon global pool (false when something used rayon first).
    pub pool_built: bool,
}

impl ComputeProfile {
    /// Memory heavy work may plan for, in percent of RAM (the user's setting or [`AUTO_MEMORY_PERCENT`]).
    pub fn memory_percent(&self) -> u8 {
        self.memory_percent_override.unwrap_or(AUTO_MEMORY_PERCENT)
    }

    /// That share of the installed RAM, in bytes.
    pub fn memory_budget(&self) -> u64 {
        self.hardware.ram_total / 100 * u64::from(self.memory_percent())
    }

    /// Whether performance and efficiency cores were detected.
    pub fn is_hybrid(&self) -> bool {
        self.hardware.perf_cores.is_some() && self.hardware.eff_cores.is_some()
    }
}

impl fmt::Display for ComputeProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = &self.hardware;
        write!(f, "compute profile: {} logical / {} physical cores", h.logical, h.physical)?;
        if let (Some(p), Some(e)) = (h.perf_cores, h.eff_cores) {
            write!(f, " ({p}P + {e}E)")?;
        }
        write!(f, ", RAM {:.1} GB", h.ram_total as f64 / (1u64 << 30) as f64)?;
        let source = match self.threads_from {
            ThreadSource::Auto => "auto".to_string(),
            ThreadSource::Setting => format!("Preferences, auto {}", self.threads_auto),
            ThreadSource::LocalImageThreads => format!("LOCAL_IMAGE_THREADS, auto {}", self.threads_auto),
            ThreadSource::RayonNumThreads => format!("RAYON_NUM_THREADS, auto {}", self.threads_auto),
        };
        write!(f, "; compute threads {} ({source})", self.threads)?;
        if !self.pool_built {
            write!(f, " [rayon global pool not built by init]")?;
        }
        let prio = match self.background_priority {
            BackgroundPriority::Low => "low",
            BackgroundPriority::Normal => "normal",
        };
        write!(f, "; background priority {prio}; memory budget {} % ", self.memory_percent())?;
        write!(f, "({})", if self.memory_percent_override.is_some() { "Preferences" } else { "auto" })?;
        if !h.fallbacks.is_empty() {
            write!(f, "; detection fell back for: {}", h.fallbacks.join(", "))?;
        }
        Ok(())
    }
}

/// The compute budget `C` for a machine: physical cores (capped by the logical count, which
/// honours cgroup quotas) minus one core of headroom when there are at least four, never fewer than
/// two workers. Hybrid chips count P and E cores together (the OS places work by priority).
///
/// `C = clamp(min(physical, logical) - reserve, 2, max(logical, 2))`, `reserve = 1` when
/// `min(physical, logical) >= 4`, else `0`.
pub fn auto_threads(logical: usize, physical: usize) -> usize {
    let logical = logical.max(1);
    let cores = physical.max(1).min(logical);
    let reserve = usize::from(cores >= 4);
    (cores - reserve).clamp(2, logical.max(2))
}

/// The threads in effect and where they came from: `LOCAL_IMAGE_THREADS`, then `RAYON_NUM_THREADS`
/// (both for testing, taken as given within 1..=1024), then the user's setting (clamped to
/// 1..=logical), then [`auto_threads`].
pub fn resolve_threads(hw: &Hardware, setting: Option<usize>, env: &dyn Fn(&str) -> Option<String>) -> (usize, ThreadSource) {
    let parse = |name: &str| env(name).and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n > 0).map(|n| n.min(1024));
    if let Some(n) = parse("LOCAL_IMAGE_THREADS") {
        return (n, ThreadSource::LocalImageThreads);
    }
    if let Some(n) = parse("RAYON_NUM_THREADS") {
        return (n, ThreadSource::RayonNumThreads);
    }
    if let Some(n) = setting.filter(|n| *n > 0) {
        return (n.min(hw.logical.max(1)), ThreadSource::Setting);
    }
    (auto_threads(hw.logical, hw.physical), ThreadSource::Auto)
}

// ------------------------------------------------------------------ detection (pure parts)

/// Physical, performance and efficiency core counts read from the platform (`None` = unknown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Topology {
    pub physical: Option<usize>,
    pub perf: Option<usize>,
    pub eff: Option<usize>,
}

/// A Linux CPU list (`0-3,8,10-11`) as CPU numbers; `None` when malformed or empty.
pub fn parse_cpu_list(s: &str) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for part in s.trim().split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?);
                if b < a || b - a > 65_536 {
                    return None;
                }
                out.extend(a..=b);
            }
            None => out.push(part.parse().ok()?),
        }
    }
    out.sort_unstable();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

/// Linux topology from sysfs, through `read(path) -> contents` (so tests can mock it):
/// - physical cores = distinct SMT sibling sets (`topology/core_cpus_list`, or the older
///   `thread_siblings_list`) over the online CPUs, falling back to distinct
///   `(physical_package_id, cluster_id, core_id)`;
/// - P/E cores: Intel hybrid lists `/sys/devices/cpu_core/cpus` and `/sys/devices/cpu_atom/cpus`;
///   otherwise ARM-style `cpu_capacity` (the largest capacity = performance cores).
pub fn linux_topology(read: &dyn Fn(&str) -> Option<String>) -> Topology {
    let cpu = |n: usize, leaf: &str| read(&format!("/sys/devices/system/cpu/cpu{n}/{leaf}"));
    let Some(online) = read("/sys/devices/system/cpu/online").and_then(|s| parse_cpu_list(&s)) else { return Topology::default() };
    // Each online CPU's core identity.
    let mut keys: Vec<(usize, String)> = Vec::with_capacity(online.len());
    for &n in &online {
        let siblings = cpu(n, "topology/core_cpus_list").or_else(|| cpu(n, "topology/thread_siblings_list")).and_then(|s| parse_cpu_list(&s));
        let key = match siblings {
            Some(list) => format!("s{list:?}"),
            None => match (cpu(n, "topology/physical_package_id"), cpu(n, "topology/core_id")) {
                (Some(pkg), Some(core)) => {
                    let cluster = cpu(n, "topology/cluster_id").unwrap_or_default();
                    format!("c{}/{}/{}", pkg.trim(), cluster.trim(), core.trim())
                }
                _ => return Topology::default(),
            },
        };
        keys.push((n, key));
    }
    let distinct = |cpus: &mut dyn Iterator<Item = usize>| -> usize {
        let mut ks: Vec<&str> = cpus.filter_map(|c| keys.iter().find(|(n, _)| *n == c).map(|(_, k)| k.as_str())).collect();
        ks.sort_unstable();
        ks.dedup();
        ks.len()
    };
    let physical = distinct(&mut online.iter().copied());
    let mut topo = Topology { physical: (physical > 0).then_some(physical), perf: None, eff: None };
    // Intel hybrid (Alder Lake and later): the kernel exposes one PMU per core type.
    let core = read("/sys/devices/cpu_core/cpus").and_then(|s| parse_cpu_list(&s));
    let atom = read("/sys/devices/cpu_atom/cpus").and_then(|s| parse_cpu_list(&s));
    if let (Some(core), Some(atom)) = (core, atom) {
        let (p, e) = (distinct(&mut core.into_iter()), distinct(&mut atom.into_iter()));
        if p > 0 && e > 0 {
            (topo.perf, topo.eff) = (Some(p), Some(e));
        }
        return topo;
    }
    // ARM big.LITTLE / DynamIQ: relative capacities per CPU.
    let caps: Option<Vec<(usize, u64)>> =
        online.iter().map(|&n| cpu(n, "cpu_capacity").and_then(|s| s.trim().parse().ok()).map(|c| (n, c))).collect();
    if let Some(caps) = caps
        && let (Some(max), Some(min)) = (caps.iter().map(|c| c.1).max(), caps.iter().map(|c| c.1).min())
        && max > min
    {
        let p = distinct(&mut caps.iter().filter(|c| c.1 == max).map(|c| c.0));
        let e = distinct(&mut caps.iter().filter(|c| c.1 != max).map(|c| c.0));
        (topo.perf, topo.eff) = (Some(p), Some(e));
    }
    topo
}

/// `/proc/meminfo` → (`MemTotal`, `MemAvailable`) in bytes.
pub fn parse_meminfo(s: &str) -> (Option<u64>, Option<u64>) {
    let field = |name: &str| {
        s.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?.strip_prefix(':')?;
            let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            Some(kb.saturating_mul(1024))
        })
    };
    (field("MemTotal"), field("MemAvailable"))
}

/// macOS topology from sysctl values (`get(name)`): `hw.physicalcpu`, and with two performance
/// levels `hw.perflevel0.physicalcpu` (P) / `hw.perflevel1.physicalcpu` (E).
pub fn macos_topology(get: &dyn Fn(&str) -> Option<u64>) -> Topology {
    let physical = get("hw.physicalcpu").filter(|n| *n > 0).map(|n| n as usize);
    let levels = get("hw.nperflevels").unwrap_or(1);
    let (perf, eff) = if levels >= 2 {
        match (get("hw.perflevel0.physicalcpu"), get("hw.perflevel1.physicalcpu")) {
            (Some(p), Some(e)) if p > 0 && e > 0 => (Some(p as usize), Some(e as usize)),
            _ => (None, None),
        }
    } else {
        (None, None)
    };
    Topology { physical, perf, eff }
}

/// macOS memory available to new work from page counts: free + speculative + purgeable +
/// file-backed (reclaimable cache) pages, times the page size. `None` without the free count.
pub fn macos_available(get: &dyn Fn(&str) -> Option<u64>) -> Option<u64> {
    let page = get("hw.pagesize").filter(|p| *p > 0)?;
    let free = get("vm.page_free_count")?;
    let rest: u64 = ["vm.page_speculative_count", "vm.page_purgeable_count", "vm.page_pageable_external_count"].iter().filter_map(|n| get(n)).sum();
    Some(free.saturating_add(rest).saturating_mul(page))
}

/// Windows `GetLogicalProcessorInformationEx(RelationProcessorCore)` records → topology: one record
/// per physical core; `EfficiencyClass` (byte 9) is higher on performance cores, and a chip is
/// hybrid when the classes differ.
pub fn windows_cores(records: &[u8]) -> Topology {
    let mut classes = Vec::new();
    let mut off = 0usize;
    while off + 10 <= records.len() {
        let word = |at: usize| u32::from_ne_bytes([records[at], records[at + 1], records[at + 2], records[at + 3]]);
        let (relationship, size) = (word(off), word(off + 4) as usize);
        if size < 10 || off + size > records.len() {
            break;
        }
        if relationship == 0 {
            classes.push(records[off + 9]);
        }
        off += size;
    }
    if classes.is_empty() {
        return Topology::default();
    }
    let (max, min) = (classes.iter().copied().max().unwrap_or(0), classes.iter().copied().min().unwrap_or(0));
    let (perf, eff) = if max > min {
        let p = classes.iter().filter(|c| **c == max).count();
        (Some(p), Some(classes.len() - p))
    } else {
        (None, None)
    };
    Topology { physical: Some(classes.len()), perf, eff }
}

/// Combine what was detected with the fallbacks (physical = logical when unknown, never half).
pub fn hardware_from(logical: Option<usize>, topo: Topology, ram_total: Option<u64>) -> Hardware {
    let mut fallbacks = Vec::new();
    let logical = logical.filter(|n| *n > 0).unwrap_or_else(|| {
        fallbacks.push("logical");
        FALLBACK_LOGICAL
    });
    let physical = topo.physical.filter(|n| *n > 0).unwrap_or_else(|| {
        fallbacks.push("physical");
        logical
    });
    let ram_total = ram_total.filter(|n| *n > 0).unwrap_or_else(|| {
        fallbacks.push("ram");
        FALLBACK_RAM
    });
    Hardware { logical, physical, perf_cores: topo.perf, eff_cores: topo.eff, ram_total, fallbacks }
}

// ------------------------------------------------------------------ detection (this machine)

fn logical_now() -> Option<usize> {
    std::thread::available_parallelism().ok().map(usize::from)
}

#[cfg(target_os = "linux")]
fn read_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(target_os = "macos")]
fn sysctl(name: &str) -> Option<u64> {
    let c = std::ffi::CString::new(name).ok()?;
    crate::sys::macos::sysctl_u64(&c)
}

fn topology_now() -> Topology {
    #[cfg(target_os = "linux")]
    {
        linux_topology(&read_file)
    }
    #[cfg(target_os = "macos")]
    {
        macos_topology(&sysctl)
    }
    #[cfg(target_os = "windows")]
    {
        crate::sys::windows::core_records().map(|r| windows_cores(&r)).unwrap_or_default()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Topology::default()
    }
}

/// (total, available) RAM in bytes, as far as the platform tells us.
fn memory_now() -> (Option<u64>, Option<u64>) {
    #[cfg(target_os = "linux")]
    {
        read_file("/proc/meminfo").map(|s| parse_meminfo(&s)).unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        (sysctl("hw.memsize"), macos_available(&sysctl))
    }
    #[cfg(target_os = "windows")]
    {
        crate::sys::windows::memory().map_or((None, None), |(t, a)| (Some(t), Some(a)))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        (None, None)
    }
}

/// Detect this machine's hardware (not cached; [`profile`] holds the frozen copy).
pub fn detect() -> Hardware {
    hardware_from(logical_now(), topology_now(), memory_now().0)
}

/// RAM available to new work right now, in bytes (re-read on every call): Linux `MemAvailable`,
/// macOS free + reclaimable pages, Windows `ullAvailPhys`. Falls back to half the installed RAM.
pub fn ram_available() -> u64 {
    match memory_now() {
        (_, Some(a)) if a > 0 => a,
        _ => profile().hardware.ram_total / 2,
    }
}

// ------------------------------------------------------------------ the frozen profile

/// The frozen profile. [`init`] replaces a provisional one that [`profile`] made before it ran
/// (each is leaked once: at most two small allocations per process).
static PROFILE: Mutex<Option<(&'static ComputeProfile, bool)>> = Mutex::new(None);

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `pool(wanted)` builds the pool and returns `(built, threads in effect)`.
fn build_profile(hw: Hardware, overrides: Overrides, pool: impl FnOnce(usize) -> (bool, usize)) -> ComputeProfile {
    let threads_auto = auto_threads(hw.logical, hw.physical);
    let (wanted, threads_from) = resolve_threads(&hw, overrides.threads, &env_var);
    let (pool_built, threads) = pool(wanted);
    ComputeProfile {
        hardware: hw,
        threads_auto,
        threads,
        threads_from,
        background_priority: overrides.background_priority,
        memory_percent_override: overrides.memory_percent.map(|p| p.clamp(10, 90)),
        pool_built,
    }
}

/// Detect the machine once, build rayon's global pool with the compute budget (named
/// `li-compute-N`) and freeze the profile. Call first thing in `main`, before anything uses rayon
/// (rayon's default pool is one thread per logical CPU and can't be resized later). Logs the profile
/// once. Later calls return the first profile unchanged.
pub fn init(overrides: Overrides) -> &'static ComputeProfile {
    let mut slot = PROFILE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((p, true)) = *slot {
        return p;
    }
    let hw = slot.map_or_else(detect, |(p, _)| p.hardware.clone());
    let p: &'static ComputeProfile = Box::leak(Box::new(build_profile(hw, overrides, |n| {
        match rayon::ThreadPoolBuilder::new().num_threads(n).thread_name(|i| format!("li-compute-{i}")).build_global() {
            Ok(()) => (true, n),
            Err(e) => {
                let running = rayon::current_num_threads();
                log::warn!("compute pool: rayon's global pool was already running ({e}); keeping its {running} threads");
                (false, running)
            }
        }
    })));
    *slot = Some((p, true));
    drop(slot);
    crate::priority::set_background_priority(p.background_priority);
    log::info!("{p}");
    p
}

/// The frozen profile ([`init`]'s, or until `init` runs the detected hardware with no overrides,
/// e.g. in tests: then `threads` is the budget `init` would use and rayon is not touched, so a later
/// `init` still builds the pool).
pub fn profile() -> &'static ComputeProfile {
    let mut slot = PROFILE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((p, _)) = *slot {
        return p;
    }
    let p: &'static ComputeProfile = Box::leak(Box::new(build_profile(detect(), Overrides::default(), |n| (false, n))));
    *slot = Some((p, false));
    p
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
