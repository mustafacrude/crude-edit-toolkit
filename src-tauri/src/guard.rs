//! Keeps the tools from hurting the computer. On Windows every tool process
//! joins one job object that runs it at below-normal priority, caps the memory
//! all tools may use together (and their CPU, when Settings asks), stops a
//! crashing tool from opening a Windows error dialog that would hang its job,
//! and kills every tool if the app itself dies. Also: free disk space, and
//! keeping the computer awake while jobs run.

use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc;

/// What fraction of RAM the tools may use together, at least `MIN_MEMORY` (Windows only).
#[cfg(windows)]
const MEMORY_SHARE: f64 = 0.7;
#[cfg(windows)]
const MIN_MEMORY: u64 = 2 * 1024 * 1024 * 1024;

#[cfg(windows)]
mod imp {
    use super::{MEMORY_SHARE, MIN_MEMORY};
    use std::path::Path;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
        JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PRIORITY_CLASS, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS;

    /// The job object's handle, kept for the life of the app. Closing it (the
    /// app exiting or crashing) kills every tool still in it.
    pub struct Job(usize);

    impl Job {
        pub fn create() -> Option<Self> {
            // SAFETY: plain Win32 calls with valid, fully initialised structs.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                    | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
                    | JOB_OBJECT_LIMIT_PRIORITY_CLASS;
                info.BasicLimitInformation.PriorityClass = BELOW_NORMAL_PRIORITY_CLASS;
                if let Some(limit) = memory_limit() {
                    info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
                    info.JobMemoryLimit = limit as usize;
                }
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                Some(Self(handle as usize))
            }
        }

        pub fn adopt(&self, process: HANDLE) -> bool {
            // SAFETY: both handles are live; the caller still owns the child.
            unsafe { AssignProcessToJobObject(self.0 as HANDLE, process) != 0 }
        }

        /// `percent` of all CPU cores, hard-capped; 100 removes the cap.
        pub fn set_cpu_limit(&self, percent: u8) {
            let mut info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                ControlFlags: 0,
                Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 { CpuRate: 0 },
            };
            if percent < 100 {
                info.ControlFlags =
                    JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
                // In hundredths of a percent.
                info.Anonymous.CpuRate = u32::from(percent.max(10)) * 100;
            }
            // SAFETY: valid handle and struct, sized as the call expects.
            unsafe {
                SetInformationJobObject(
                    self.0 as HANDLE,
                    JobObjectCpuRateControlInformation,
                    (&raw const info).cast(),
                    size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                );
            }
        }
    }

    /// None when the RAM size is unknown: then there's no cap rather than a wrong one.
    pub fn memory_limit() -> Option<u64> {
        // SAFETY: dwLength is set as the API requires.
        let total = unsafe {
            let mut status: MEMORYSTATUSEX = std::mem::zeroed();
            status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut status) == 0 {
                return None;
            }
            status.ullTotalPhys
        };
        Some(((total as f64 * MEMORY_SHARE) as u64).max(MIN_MEMORY))
    }

    pub fn free_space(dir: &Path) -> Option<u64> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let mut free = 0u64;
        // SAFETY: NUL-terminated path; out pointer is valid.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }

    pub struct Awake;

    impl Awake {
        pub fn new() -> Self {
            Self
        }

        /// Called from one dedicated thread: the setting belongs to that thread.
        pub fn set(&mut self, busy: bool) {
            let flags = if busy {
                ES_CONTINUOUS | ES_SYSTEM_REQUIRED
            } else {
                ES_CONTINUOUS
            };
            // SAFETY: no pointers involved.
            unsafe { SetThreadExecutionState(flags) };
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::path::Path;

    /// No job objects here; tools are niced instead (see tools.rs).
    pub struct Job;

    impl Job {
        pub fn create() -> Option<Self> {
            None
        }
        pub fn set_cpu_limit(&self, _percent: u8) {}
    }

    pub fn free_space(dir: &Path) -> Option<u64> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY: NUL-terminated path; statvfs fills the struct.
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
                return None;
            }
            Some(stat.f_bavail as u64 * stat.f_frsize as u64)
        }
    }

    /// macOS: `caffeinate -i` runs while jobs do. Elsewhere, nothing.
    pub struct Awake(Option<std::process::Child>);

    impl Awake {
        pub fn new() -> Self {
            Self(None)
        }

        pub fn set(&mut self, busy: bool) {
            match (busy, &mut self.0) {
                (true, None) if cfg!(target_os = "macos") => {
                    self.0 = std::process::Command::new("caffeinate")
                        .arg("-i")
                        .spawn()
                        .ok();
                }
                (false, Some(child)) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.0 = None;
                }
                _ => {}
            }
        }
    }
}

static JOB: OnceLock<Option<imp::Job>> = OnceLock::new();
static AWAKE: OnceLock<std::sync::Mutex<mpsc::Sender<bool>>> = OnceLock::new();

fn job() -> Option<&'static imp::Job> {
    JOB.get_or_init(imp::Job::create).as_ref()
}

/// Puts a freshly started tool under the limits. Its own children (yt-dlp's
/// ffmpeg, exiftool's perl) join automatically.
pub fn adopt(child: &tokio::process::Child) {
    #[cfg(windows)]
    if let (Some(job), Some(handle)) = (job(), child.raw_handle()) {
        job.adopt(handle);
    }
    #[cfg(unix)]
    let _ = child;
}

pub fn adopt_std(child: &std::process::Child) {
    #[cfg(windows)]
    if let Some(job) = job() {
        use std::os::windows::io::AsRawHandle;
        job.adopt(child.as_raw_handle());
    }
    #[cfg(unix)]
    let _ = child;
}

/// Caps the tools at `percent` of the CPU (Settings → Encoding). Windows only.
pub fn set_cpu_limit(percent: u8) {
    if let Some(job) = job() {
        job.set_cpu_limit(percent);
    }
}

/// Bytes free on the drive holding `dir` (or its nearest existing parent).
pub fn free_space(dir: &Path) -> Option<u64> {
    let existing = dir.ancestors().find(|d| d.exists())?;
    imp::free_space(existing)
}

/// Keeps the computer from going to sleep on its own while jobs run. Closing
/// a laptop's lid still sleeps it.
pub fn keep_awake(busy: bool) {
    let sender = AWAKE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            let mut awake = imp::Awake::new();
            let mut current = false;
            while let Ok(busy) = rx.recv() {
                if busy != current {
                    awake.set(busy);
                    current = busy;
                }
            }
        });
        std::sync::Mutex::new(tx)
    });
    let _ = sender.lock().map(|tx| tx.send(busy));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_free_space() {
        let free = free_space(&std::env::temp_dir().join("not-made-yet")).unwrap();
        assert!(free > 0);
    }

    #[cfg(windows)]
    #[test]
    fn caps_memory_sensibly() {
        let limit = imp::memory_limit().unwrap();
        assert!(limit >= MIN_MEMORY);
    }

    #[test]
    fn keep_awake_toggles() {
        keep_awake(true);
        keep_awake(true);
        keep_awake(false);
    }
}
