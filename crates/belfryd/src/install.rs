//! Installing belfryd as a system service.
//! Linux: copies itself + engine to /opt/belfry and adds a systemd unit.
//! Windows: registers the service in place (the installer already put the
//! files in Program Files) and opens the firewall for the engine.

use crate::Opts;
use std::process::Command;

fn sh(cmd: &mut Command) -> Result<(), String> {
    crate::platform::hide_window(cmd);
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{cmd:?}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(target_os = "linux")]
const PREFIX: &str = "/opt/belfry";
#[cfg(target_os = "linux")]
const UNIT: &str = "/etc/systemd/system/belfryd.service";

#[cfg(target_os = "linux")]
fn copy_exec(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // Write next to the target and rename: works even while the old binary runs.
    let tmp = dst.with_extension("new");
    std::fs::copy(src, &tmp).map_err(|e| format!("copy {}: {e}", src.display()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
pub fn install(o: &Opts) -> Result<(), String> {
    use std::path::PathBuf;
    if !crate::platform::is_admin() {
        return Err("install needs root (the app runs it through pkexec)".into());
    }
    let from: PathBuf = match &o.from {
        Some(f) => f.clone(),
        None => std::env::current_exe().map_err(|e| e.to_string())?.parent().unwrap().to_path_buf(),
    };
    let engine = crate::engine::Engine::locate(Some(from.join("engine")))?;
    // pkexec passes the caller's uid; the token file will belong to them.
    let owner = o.owner_uid.or_else(|| std::env::var("PKEXEC_UID").ok()?.parse().ok());
    std::fs::create_dir_all(format!("{PREFIX}/engine")).map_err(|e| e.to_string())?;
    copy_exec(&from.join("belfryd"), std::path::Path::new(&format!("{PREFIX}/belfryd")))?;
    for exe in ["easytier-core", "easytier-cli"] {
        copy_exec(&engine.dir.join(exe), std::path::Path::new(&format!("{PREFIX}/engine/{exe}")))?;
    }
    let owner_arg = owner.map(|u| format!(" --owner-uid {u}")).unwrap_or_default();
    let unit = format!(
        "[Unit]\n\
         Description=Belfry rooms (EasyTier engine)\n\
         Documentation=https://github.com/ludoviciphilip/belfry\n\
         After=network-online.target\n\
         Wants=network-online.target\n\n\
         [Service]\n\
         ExecStart={PREFIX}/belfryd run{owner_arg}\n\
         Restart=on-failure\n\
         RestartSec=3\n\n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    );
    std::fs::write(UNIT, unit).map_err(|e| format!("{UNIT}: {e}"))?;
    sh(Command::new("systemctl").arg("daemon-reload"))?;
    sh(Command::new("systemctl").args(["enable", "belfryd.service"]))?;
    sh(Command::new("systemctl").args(["restart", "belfryd.service"]))?;
    println!("belfryd installed in {PREFIX} and started");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn uninstall(o: &Opts) -> Result<(), String> {
    if !crate::platform::is_admin() {
        return Err("uninstall needs root".into());
    }
    let _ = sh(Command::new("systemctl").args(["disable", "--now", "belfryd.service"]));
    let _ = std::fs::remove_file(UNIT);
    let _ = sh(Command::new("systemctl").arg("daemon-reload"));
    let _ = std::fs::remove_dir_all(PREFIX);
    if o.purge {
        let _ = std::fs::remove_dir_all(&o.data_dir);
    }
    println!("belfryd removed");
    Ok(())
}

#[cfg(windows)]
pub fn install(_o: &Opts) -> Result<(), String> {
    use std::ffi::OsString;
    use windows_service::service::*;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    if !crate::platform::is_admin() {
        return Err("install needs administrator rights".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let engine = crate::engine::Engine::locate(None)?;
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|e| e.to_string())?;
    let info = ServiceInfo {
        name: OsString::from(crate::winsvc::NAME),
        display_name: OsString::from("Belfry"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.clone(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG;
    let service = match manager.open_service(crate::winsvc::NAME, access) {
        Ok(s) => {
            let _ = s.stop();
            wait_stopped(&s);
            s.change_config(&info).map_err(|e| e.to_string())?;
            s
        }
        Err(_) => manager.create_service(&info, access).map_err(|e| e.to_string())?,
    };
    let _ = service.set_description("Keeps your Belfry rooms connected (EasyTier engine).");
    // Let friends reach the engine; the rule is tied to the binary, not a port.
    let core = engine.core();
    let _ = sh(Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", "name=Belfry engine"]));
    sh(Command::new("netsh").args([
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=Belfry engine",
        "dir=in",
        "action=allow",
        "enable=yes",
        "profile=any",
        &format!("program={}", core.display()),
    ]))?;
    // Chat (belfryd) and file sharing (the app) take connections from room
    // members; both check the room key and the source subnet themselves.
    let _ = sh(Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", "name=Belfry chat"]));
    let _ = sh(Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", "name=Belfry files"]));
    let _ = sh(Command::new("netsh").args([
        "advfirewall", "firewall", "add", "rule", "name=Belfry chat", "dir=in", "action=allow",
        "enable=yes", "profile=any", "protocol=TCP", "localport=47831",
        &format!("program={}", exe.display()),
    ]));
    if let Some(app) = exe.parent().map(|d| d.join("belfry.exe")).filter(|p| p.is_file()) {
        let _ = sh(Command::new("netsh").args([
            "advfirewall", "firewall", "add", "rule", "name=Belfry files", "dir=in", "action=allow",
            "enable=yes", "profile=any", "protocol=TCP", "localport=47832",
            &format!("program={}", app.display()),
        ]));
    }
    service.start::<&str>(&[]).map_err(|e| format!("start: {e}"))?;
    println!("belfryd service installed and started");
    Ok(())
}

#[cfg(windows)]
fn wait_stopped(s: &windows_service::service::Service) {
    for _ in 0..50 {
        match s.query_status() {
            Ok(st) if st.current_state == windows_service::service::ServiceState::Stopped => return,
            Err(_) => return,
            _ => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    }
}

#[cfg(windows)]
pub fn uninstall(o: &Opts) -> Result<(), String> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    if !crate::platform::is_admin() {
        return Err("uninstall needs administrator rights".into());
    }
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| e.to_string())?;
    if let Ok(s) = manager.open_service(
        crate::winsvc::NAME,
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    ) {
        let _ = s.stop();
        wait_stopped(&s);
        s.delete().map_err(|e| e.to_string())?;
    }
    for rule in ["name=Belfry engine", "name=Belfry chat", "name=Belfry files"] {
        let _ = sh(Command::new("netsh").args(["advfirewall", "firewall", "delete", "rule", rule]));
    }
    if o.purge {
        let _ = std::fs::remove_dir_all(&o.data_dir);
    }
    println!("belfryd service removed");
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn install(_o: &Opts) -> Result<(), String> {
    Err("service install is only implemented for Linux and Windows; use `belfryd run` as root".into())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn uninstall(_o: &Opts) -> Result<(), String> {
    Err("not implemented on this OS".into())
}
