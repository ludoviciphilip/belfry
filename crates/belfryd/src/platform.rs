//! OS specifics: making engine processes die with the service, hiding
//! console windows on Windows, file permissions.

use std::process::{Child, Command};

#[cfg(unix)]
pub fn tie_child_to_us(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            // If belfryd dies, the kernel sends SIGTERM to the engine.
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

#[cfg(unix)]
pub fn after_spawn(_child: &Child) {}

#[cfg(unix)]
pub fn hide_window(_cmd: &mut Command) {}

#[cfg(unix)]
pub fn is_admin() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(unix)]
pub fn restrict_to_owner(path: &std::path::Path, owner_uid: Option<u32>) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    if let Some(uid) = owner_uid {
        if let Ok(c) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) {
            unsafe {
                libc::chown(c.as_ptr(), uid, u32::MAX);
            }
        }
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(windows)]
pub fn tie_child_to_us(cmd: &mut Command) {
    hide_window(cmd);
}

/// All engine processes join one job object that is killed when belfryd exits.
#[cfg(windows)]
pub fn after_spawn(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        job as usize
    });
    unsafe {
        AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
    }
}

#[cfg(windows)]
pub fn is_admin() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev: TOKEN_ELEVATION = std::mem::zeroed();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

/// ProgramData\Belfry inherits "Users: read"; the token stays readable by
/// local users so the app can talk to the service without elevation.
#[cfg(windows)]
pub fn restrict_to_owner(_path: &std::path::Path, _owner_uid: Option<u32>) {}

/// New virtual adapters land in the "Public" firewall profile, which makes
/// Windows block incoming game connections. Mark ours as Private.
#[cfg(windows)]
pub fn mark_adapter_private(dev: &str) {
    let script = format!(
        "Get-NetConnectionProfile -InterfaceAlias '{dev}' -ErrorAction SilentlyContinue | \
         Set-NetConnectionProfile -NetworkCategory Private"
    );
    let _ = crate::engine::run_with_timeout(
        Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]),
        std::time::Duration::from_secs(20),
    );
}

#[cfg(not(windows))]
pub fn mark_adapter_private(_dev: &str) {}
