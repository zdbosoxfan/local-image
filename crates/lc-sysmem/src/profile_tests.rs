use super::*;
use std::collections::HashMap;

/// A mocked sysfs/procfs: path → contents.
fn fs(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |p: &str| map.get(p).cloned()
}

/// sysfs for `cores` physical cores with `smt` threads each, numbered like Linux does on x86
/// (siblings of core c are c, c + cores, …).
fn smt_sysfs(cores: usize, smt: usize) -> Vec<(String, String)> {
    let n = cores * smt;
    let mut v = vec![("/sys/devices/system/cpu/online".to_string(), format!("0-{}", n - 1))];
    for cpu in 0..n {
        let core = cpu % cores;
        let sibs: Vec<String> = (0..smt).map(|t| (core + t * cores).to_string()).collect();
        v.push((format!("/sys/devices/system/cpu/cpu{cpu}/topology/core_cpus_list"), sibs.join(",")));
    }
    v
}

fn as_refs(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
}

#[test]
fn pool_size_for_the_test_matrix_machine_classes() {
    // (class, logical, physical, expected C) from the review's §6 test matrix.
    let matrix = [
        ("small laptop 2c/4t", 4, 2, 2),
        ("4-core no SMT", 4, 4, 3),
        ("mid laptop 4c/8t", 8, 4, 3),
        ("8C/16T (Ryzen 9800X3D)", 16, 8, 7),
        ("M1 4P+4E, no SMT", 8, 8, 7),
        ("M4 Max 12P+4E", 16, 16, 15),
        ("hybrid 6P+8E (6P with SMT)", 20, 14, 13),
        ("i9-14900K 8P+16E", 32, 24, 23),
        ("32C/64T workstation", 64, 32, 31),
        ("SMT off 8c/8t", 8, 8, 7),
        ("2-CPU container on a 64-core host", 2, 64, 2),
        ("single CPU", 1, 1, 2),
    ];
    for (class, logical, physical, want) in matrix {
        assert_eq!(auto_threads(logical, physical), want, "{class}");
    }
}

#[test]
fn detection_failure_uses_the_documented_fallbacks() {
    let hw = hardware_from(None, Topology::default(), None);
    assert_eq!((hw.logical, hw.physical, hw.ram_total), (4, 4, 8 << 30));
    assert_eq!(hw.fallbacks, vec!["logical", "physical", "ram"]);
    assert_eq!(auto_threads(hw.logical, hw.physical), 3);
    // Unknown topology never halves the logical count (Apple silicon, ARM, SMT off).
    let hw = hardware_from(Some(16), Topology::default(), Some(64 << 30));
    assert_eq!((hw.physical, hw.fallbacks.as_slice()), (16, &["physical"][..]));
}

#[test]
fn linux_smt_on_and_off() {
    let on = smt_sysfs(8, 2);
    assert_eq!(linux_topology(&fs(&as_refs(&on))), Topology { physical: Some(8), perf: None, eff: None });
    let off = smt_sysfs(8, 1);
    assert_eq!(linux_topology(&fs(&as_refs(&off))).physical, Some(8));
}

#[test]
fn linux_older_kernels_use_thread_siblings_and_core_ids() {
    let mut v = smt_sysfs(4, 2);
    for (k, _) in v.iter_mut() {
        *k = k.replace("core_cpus_list", "thread_siblings_list");
    }
    assert_eq!(linux_topology(&fs(&as_refs(&v))).physical, Some(4));
    // No sibling lists: (package, cluster, core_id); ARM clusters reuse core ids.
    let mut ids = vec![("/sys/devices/system/cpu/online".to_string(), "0-7".to_string())];
    for cpu in 0..8 {
        let p = format!("/sys/devices/system/cpu/cpu{cpu}/topology/");
        ids.push((format!("{p}physical_package_id"), "0".into()));
        ids.push((format!("{p}cluster_id"), (cpu / 4).to_string()));
        ids.push((format!("{p}core_id"), (cpu % 4).to_string()));
    }
    assert_eq!(linux_topology(&fs(&as_refs(&ids))).physical, Some(8));
}

#[test]
fn linux_intel_hybrid_counts_p_and_e_cores() {
    // 6 P-cores with SMT (CPUs 0-11, siblings 2k/2k+1) and 8 E-cores (12-19).
    let mut v = vec![
        ("/sys/devices/system/cpu/online".to_string(), "0-19".to_string()),
        ("/sys/devices/cpu_core/cpus".to_string(), "0-11".to_string()),
        ("/sys/devices/cpu_atom/cpus".to_string(), "12-19".to_string()),
    ];
    for cpu in 0..20 {
        let sib = if cpu < 12 { format!("{}-{}", cpu / 2 * 2, cpu / 2 * 2 + 1) } else { cpu.to_string() };
        v.push((format!("/sys/devices/system/cpu/cpu{cpu}/topology/core_cpus_list"), sib));
    }
    let t = linux_topology(&fs(&as_refs(&v)));
    assert_eq!(t, Topology { physical: Some(14), perf: Some(6), eff: Some(8) });
    let hw = hardware_from(Some(20), t, Some(32 << 30));
    assert_eq!(auto_threads(hw.logical, hw.physical), 13);
}

#[test]
fn linux_arm_capacity_marks_big_cores() {
    let mut v = smt_sysfs(8, 1);
    for cpu in 0..8 {
        v.push((format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity"), if cpu < 4 { "446" } else { "1024" }.into()));
    }
    assert_eq!(linux_topology(&fs(&as_refs(&v))), Topology { physical: Some(8), perf: Some(4), eff: Some(4) });
}

#[test]
fn linux_missing_or_odd_sysfs_is_unknown_not_guessed() {
    assert_eq!(linux_topology(&fs(&[])), Topology::default());
    assert_eq!(linux_topology(&fs(&[("/sys/devices/system/cpu/online", "garbage")])), Topology::default());
    // An online CPU without any topology files: unknown (then physical = logical).
    assert_eq!(linux_topology(&fs(&[("/sys/devices/system/cpu/online", "0-3")])), Topology::default());
    assert_eq!(parse_cpu_list("0-3,8,10-11"), Some(vec![0, 1, 2, 3, 8, 10, 11]));
    assert_eq!(parse_cpu_list("5-2"), None);
    assert_eq!(parse_cpu_list(""), None);
}

#[test]
fn meminfo_total_and_available() {
    let s = "MemTotal:       65536000 kB\nMemFree:         1000 kB\nMemAvailable:   50331648 kB\n";
    assert_eq!(parse_meminfo(s), (Some(65_536_000 * 1024), Some(50_331_648 * 1024)));
    // Old kernels without MemAvailable: total only (available falls back to total / 2).
    assert_eq!(parse_meminfo("MemTotal: 1024 kB\n"), (Some(1024 * 1024), None));
    assert_eq!(parse_meminfo(""), (None, None));
}

#[test]
fn macos_sysctl_topology_and_memory() {
    let m4: HashMap<&str, u64> = [
        ("hw.physicalcpu", 16),
        ("hw.nperflevels", 2),
        ("hw.perflevel0.physicalcpu", 12),
        ("hw.perflevel1.physicalcpu", 4),
        ("hw.pagesize", 16384),
        ("vm.page_free_count", 1000),
        ("vm.page_speculative_count", 100),
        ("vm.page_pageable_external_count", 900),
    ]
    .into_iter()
    .collect();
    let get = |n: &str| m4.get(n).copied();
    assert_eq!(macos_topology(&get), Topology { physical: Some(16), perf: Some(12), eff: Some(4) });
    assert_eq!(macos_available(&get), Some(2000 * 16384));
    // Intel Mac: one performance level, no hybrid; missing page counts → None (fallback).
    let intel = |n: &str| (n == "hw.physicalcpu").then_some(8);
    assert_eq!(macos_topology(&intel), Topology { physical: Some(8), perf: None, eff: None });
    assert_eq!(macos_available(&intel), None);
}

/// A `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` processor-core record (48 bytes, one group).
fn win_core(efficiency: u8) -> Vec<u8> {
    let mut r = vec![0u8; 48];
    r[0..4].copy_from_slice(&0u32.to_ne_bytes());
    r[4..8].copy_from_slice(&48u32.to_ne_bytes());
    r[9] = efficiency;
    r
}

#[test]
fn windows_core_records() {
    let hybrid: Vec<u8> = (0..24).flat_map(|i| win_core(if i < 8 { 1 } else { 0 })).collect();
    assert_eq!(windows_cores(&hybrid), Topology { physical: Some(24), perf: Some(8), eff: Some(16) });
    let plain: Vec<u8> = (0..8).flat_map(|_| win_core(0)).collect();
    assert_eq!(windows_cores(&plain), Topology { physical: Some(8), perf: None, eff: None });
    // Truncated or zero-sized records stop the walk instead of looping.
    let mut bad = win_core(0);
    bad[4..8].copy_from_slice(&0u32.to_ne_bytes());
    assert_eq!(windows_cores(&bad), Topology::default());
    assert_eq!(windows_cores(&[1, 2, 3]), Topology::default());
}

#[test]
fn thread_overrides_env_then_setting_then_auto() {
    let hw = hardware_from(Some(16), Topology { physical: Some(8), perf: None, eff: None }, Some(64 << 30));
    let none = |_: &str| None;
    assert_eq!(resolve_threads(&hw, None, &none), (7, ThreadSource::Auto));
    assert_eq!(resolve_threads(&hw, Some(4), &none), (4, ThreadSource::Setting));
    // The setting is a cap within the machine; 0 means Auto.
    assert_eq!(resolve_threads(&hw, Some(64), &none), (16, ThreadSource::Setting));
    assert_eq!(resolve_threads(&hw, Some(0), &none), (7, ThreadSource::Auto));
    let rayon_env = |k: &str| (k == "RAYON_NUM_THREADS").then(|| "3".to_string());
    assert_eq!(resolve_threads(&hw, Some(4), &rayon_env), (3, ThreadSource::RayonNumThreads));
    let both = |k: &str| match k {
        "LOCAL_IMAGE_THREADS" => Some("1".to_string()),
        "RAYON_NUM_THREADS" => Some("3".to_string()),
        _ => None,
    };
    assert_eq!(resolve_threads(&hw, None, &both), (1, ThreadSource::LocalImageThreads));
    let junk = |k: &str| (k == "LOCAL_IMAGE_THREADS").then(|| "lots".to_string());
    assert_eq!(resolve_threads(&hw, None, &junk), (7, ThreadSource::Auto));
}

#[test]
fn the_profile_logs_what_it_found_and_decided() {
    let hw = hardware_from(Some(20), Topology { physical: Some(14), perf: Some(6), eff: Some(8) }, Some(32 << 30));
    let p = build_profile(hw, Overrides { threads: None, background_priority: BackgroundPriority::Low, memory_percent: Some(95) }, |n| (true, n));
    assert!(p.is_hybrid());
    assert_eq!((p.threads, p.memory_percent()), (13, 90));
    assert_eq!(p.memory_budget(), (32u64 << 30) / 100 * 90);
    let line = p.to_string();
    for part in ["20 logical / 14 physical", "(6P + 8E)", "RAM 32.0 GB", "compute threads 13 (auto)", "background priority low", "memory budget 90 %"]
    {
        assert!(line.contains(part), "{line:?} lacks {part:?}");
    }
}

#[test]
fn this_machine_detects_something_sane() {
    let hw = detect();
    assert!(hw.logical >= 1 && hw.physical >= 1 && hw.ram_total > 0);
    if cfg!(target_os = "linux") {
        // Linux CI and dev boxes have sysfs and /proc/meminfo.
        assert!(!hw.fallbacks.contains(&"ram"), "{hw:?}");
    }
    assert!(ram_available() > 0);
    let p = profile();
    assert!(p.threads >= 1 && p.threads_auto >= 2);
}
