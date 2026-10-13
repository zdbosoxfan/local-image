//! The platform calls behind [`crate::profile`] and [`crate::priority`]: one small `unsafe` block per
//! OS call, each with its SAFETY note. Everything here returns plain values or `None`/`Err` on
//! failure; the parsing of what the OS hands back lives in safe, tested code in `profile.rs`.

/// What a priority call failed with (an OS error number, or a short reason).
pub type PriorityResult = Result<(), String>;

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub mod linux {
    use super::PriorityResult;

    /// Linux nice values and scheduling policy for a thread class: `(nice, SCHED_BATCH?)`.
    pub fn set_current_thread(nice: i32, batch: bool) -> PriorityResult {
        // SAFETY: `gettid` takes no arguments, cannot fail and touches no memory.
        let tid = unsafe { libc::gettid() };
        let param = libc::sched_param { sched_priority: 0 };
        let policy = if batch { libc::SCHED_BATCH } else { libc::SCHED_OTHER };
        // SAFETY: `param` is a valid, initialised `sched_param` that outlives the call; `tid` is
        // this thread's id, so only this thread's policy changes. SCHED_OTHER/SCHED_BATCH with
        // priority 0 need no privilege.
        let mut failure = None;
        if unsafe { libc::sched_setscheduler(tid, policy, &param) } != 0 {
            failure = Some(format!("sched_setscheduler: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: plain integer arguments; `getpriority` reads this thread's nice value (it may
        // legitimately return -1, which only makes us set the value below).
        let current = unsafe { libc::getpriority(libc::PRIO_PROCESS, tid as libc::id_t) };
        // Never raise priority above what the thread already has unless asked for nice 0, and
        // don't fight a user who started the whole app with a higher nice value.
        if current != nice && (nice == 0 || current < nice) {
            // SAFETY: plain integer arguments; affects only this thread (Linux nice is per
            // thread). Raising nice needs no privilege; lowering it may fail with EACCES, which
            // we report.
            if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } != 0 {
                failure = Some(format!("setpriority({nice}): {}", std::io::Error::last_os_error()));
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod macos {
    use super::PriorityResult;
    use std::ffi::{CStr, c_char, c_int, c_void};

    unsafe extern "C" {
        /// libSystem sysctl by name (sys/sysctl.h).
        fn sysctlbyname(name: *const c_char, oldp: *mut c_void, oldlenp: *mut usize, newp: *mut c_void, newlen: usize) -> c_int;
        /// libpthread (pthread/qos.h): set this thread's QoS class; `qos_class_t` is an unsigned
        /// int enum.
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: c_int) -> c_int;
    }

    /// QoS classes (pthread/qos.h `qos_class_t`).
    pub const QOS_CLASS_DEFAULT: u32 = 0x15;
    pub const QOS_CLASS_UTILITY: u32 = 0x11;
    pub const QOS_CLASS_BACKGROUND: u32 = 0x09;

    /// An integer sysctl (4- or 8-byte value), `None` when the name doesn't exist.
    pub fn sysctl_u64(name: &CStr) -> Option<u64> {
        let mut buf = [0u8; 8];
        let mut len = buf.len();
        // SAFETY: `name` is NUL-terminated; `buf` is writable for `len` bytes and `len` tells the
        // kernel so; no new value is set (null pointer, length 0).
        let r = unsafe { sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) };
        if r != 0 {
            return None;
        }
        match len {
            4 => Some(u64::from(u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]))),
            8 => Some(u64::from_ne_bytes(buf)),
            _ => None,
        }
    }

    pub fn set_qos(class: u32) -> PriorityResult {
        // SAFETY: plain integer arguments; only this thread's QoS changes.
        let r = unsafe { pthread_set_qos_class_self_np(class, 0) };
        if r == 0 { Ok(()) } else { Err(format!("pthread_set_qos_class_self_np: error {r}")) }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code, non_snake_case)]
pub mod windows {
    use super::PriorityResult;
    use std::ffi::c_void;

    /// `MEMORYSTATUSEX` (sysinfoapi.h).
    #[repr(C)]
    struct MemoryStatusEx {
        dwLength: u32,
        dwMemoryLoad: u32,
        ullTotalPhys: u64,
        ullAvailPhys: u64,
        ullTotalPageFile: u64,
        ullAvailPageFile: u64,
        ullTotalVirtual: u64,
        ullAvailVirtual: u64,
        ullAvailExtendedVirtual: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
        fn GetLastError() -> u32;
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        fn GetLogicalProcessorInformationEx(relationship: i32, buffer: *mut u8, returned_length: *mut u32) -> i32;
    }

    pub const THREAD_PRIORITY_NORMAL: i32 = 0;
    pub const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
    pub const THREAD_MODE_BACKGROUND_BEGIN: i32 = 0x0001_0000;
    pub const THREAD_MODE_BACKGROUND_END: i32 = 0x0002_0000;
    const ERROR_THREAD_MODE_ALREADY_BACKGROUND: u32 = 400;
    const ERROR_THREAD_MODE_NOT_BACKGROUND: u32 = 401;
    const RELATION_PROCESSOR_CORE: i32 = 0;

    /// (total, available) physical memory in bytes.
    pub fn memory() -> Option<(u64, u64)> {
        let mut m = MemoryStatusEx {
            dwLength: std::mem::size_of::<MemoryStatusEx>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };
        // SAFETY: `m` is a properly sized, writable MEMORYSTATUSEX with `dwLength` set, as the
        // API requires.
        let ok = unsafe { GlobalMemoryStatusEx(&mut m) } != 0;
        ok.then_some((m.ullTotalPhys, m.ullAvailPhys))
    }

    /// The raw `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` records for processor cores (parsed by
    /// `profile::windows_cores`).
    pub fn core_records() -> Option<Vec<u8>> {
        let mut len = 0u32;
        // SAFETY: a null buffer with length 0 only asks for the needed length (the call fails with
        // ERROR_INSUFFICIENT_BUFFER and writes `len`).
        unsafe { GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, std::ptr::null_mut(), &mut len) };
        if len == 0 || len > 16 << 20 {
            return None;
        }
        // u64 storage keeps the records 8-byte aligned, as the structures (KAFFINITY) need.
        let mut words = vec![0u64; (len as usize).div_ceil(8)];
        let mut got = (words.len() * 8) as u32;
        // SAFETY: `words` is writable for `got` bytes and `got` says so; the API writes at most
        // `got` bytes and updates it with the length written.
        let ok = unsafe { GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, words.as_mut_ptr().cast(), &mut got) } != 0;
        if !ok {
            return None;
        }
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).take(got as usize).collect();
        Some(bytes)
    }

    fn set(priority: i32) -> Result<(), u32> {
        // SAFETY: GetCurrentThread returns a pseudo handle for this thread (never fails, needs no
        // closing); SetThreadPriority only changes this thread's scheduling.
        let ok = unsafe { SetThreadPriority(GetCurrentThread(), priority) } != 0;
        // SAFETY: reads this thread's last-error value; no arguments.
        if ok { Ok(()) } else { Err(unsafe { GetLastError() }) }
    }

    pub fn set_current_thread(priority: i32, background: bool) -> PriorityResult {
        if background {
            return match set(THREAD_MODE_BACKGROUND_BEGIN) {
                Ok(()) | Err(ERROR_THREAD_MODE_ALREADY_BACKGROUND) => Ok(()),
                Err(e) => Err(format!("SetThreadPriority(background begin): error {e}")),
            };
        }
        match set(THREAD_MODE_BACKGROUND_END) {
            Ok(()) | Err(ERROR_THREAD_MODE_NOT_BACKGROUND) => {}
            Err(e) => return Err(format!("SetThreadPriority(background end): error {e}")),
        }
        set(priority).map_err(|e| format!("SetThreadPriority({priority}): error {e}"))
    }
}
