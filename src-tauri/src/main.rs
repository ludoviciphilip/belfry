// The Belfry window. All networking lives in the belfryd service; this side
// only forwards the UI's requests to it over loopback.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use belfry_proto::code::RoomCode;
use belfry_proto::*;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use tauri::Manager;

fn base_url() -> String {
    std::env::var("BELFRY_URL").unwrap_or_else(|_| format!("http://{DAEMON_ADDR}"))
}

/// Errors start with a tag the UI understands: NO_SERVICE, NO_TOKEN, API.
fn call<T: serde::de::DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, String> {
    let token = std::fs::read_to_string(token_path())
        .map_err(|e| format!("NO_TOKEN: {} ({e})", token_path().display()))?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(800))
        .timeout(Duration::from_secs(20))
        .build();
    let req = agent
        .request(method, &format!("{}{path}", base_url()))
        .set("Authorization", &format!("Bearer {}", token.trim()));
    let res = match body {
        Some(b) => req.send_json(b),
        None => req.call(),
    };
    match res {
        Ok(r) => r.into_json::<T>().map_err(|e| format!("API: {e}")),
        Err(ureq::Error::Status(401, _)) => Err("NO_TOKEN: the service rejected the token".into()),
        Err(ureq::Error::Status(_, r)) => {
            let msg = r
                .into_json::<ApiError>()
                .map(|e| e.error)
                .unwrap_or_else(|_| "unknown error".into());
            Err(format!("API: {msg}"))
        }
        Err(ureq::Error::Transport(t)) => Err(format!("NO_SERVICE: {t}")),
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_status() -> Result<Status, String> {
    blocking(|| call("GET", "/v1/status", None)).await
}

#[tauri::command]
async fn create_room(name: String, nickname: String, relay: Option<String>) -> Result<RoomView, String> {
    let body = serde_json::to_value(CreateRequest { name, nickname, relay }).unwrap();
    blocking(move || call("POST", "/v1/rooms", Some(body))).await
}

#[tauri::command]
async fn join_room(code: String, nickname: String) -> Result<RoomView, String> {
    let body = serde_json::to_value(JoinRequest { code, nickname }).unwrap();
    blocking(move || call("POST", "/v1/join", Some(body))).await
}

#[tauri::command]
async fn set_room_active(id: String, active: bool) -> Result<RoomView, String> {
    let action = if active { "start" } else { "stop" };
    blocking(move || call("POST", &format!("/v1/rooms/{id}/{action}"), Some(serde_json::json!({})))).await
}

#[tauri::command]
async fn set_nickname(id: String, nickname: String) -> Result<RoomView, String> {
    let body = serde_json::to_value(RenameRequest { nickname }).unwrap();
    blocking(move || call("POST", &format!("/v1/rooms/{id}/nickname"), Some(body))).await
}

#[tauri::command]
async fn delete_room(id: String) -> Result<serde_json::Value, String> {
    blocking(move || call("DELETE", &format!("/v1/rooms/{id}"), None)).await
}

#[tauri::command]
async fn room_log(id: String) -> Result<String, String> {
    blocking(move || {
        let token = std::fs::read_to_string(token_path()).map_err(|e| e.to_string())?;
        ureq::get(&format!("{}/v1/rooms/{id}/log", base_url()))
            .set("Authorization", &format!("Bearer {}", token.trim()))
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Serialize)]
struct CodePreview {
    name: String,
    addrs: Vec<String>,
}

/// Lets the join dialog show which room a pasted code points to.
#[tauri::command]
fn preview_code(code: String) -> Result<CodePreview, String> {
    let c = RoomCode::decode(&code)?;
    let addrs = c
        .v4
        .map(|i| i.to_string())
        .into_iter()
        .chain(c.v6.map(|i| i.to_string()))
        .chain(c.relay.clone())
        .collect();
    Ok(CodePreview { name: c.name, addrs })
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    os: String,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo { version: VERSION.into(), os: std::env::consts::OS.into() }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.into()
    }
}

/// belfryd sits next to the app binary (Tauri sidecar); the engine is a resource.
fn bundled_paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .unwrap()
        .to_path_buf();
    let daemon = exe_dir.join(exe_name("belfryd"));
    let mut engines = vec![];
    if let Ok(r) = app.path().resource_dir() {
        engines.push(r.join("engine"));
    }
    engines.push(exe_dir.join("engine"));
    let engine = engines
        .into_iter()
        .find(|p| p.join(exe_name("easytier-core")).is_file())
        .ok_or("the EasyTier engine is missing from this Belfry build")?;
    if !daemon.is_file() {
        return Err(format!("belfryd is missing ({})", daemon.display()));
    }
    Ok((daemon, engine))
}

/// Installs (or updates) the background service. Asks for the admin password.
#[tauri::command]
async fn install_service(app: tauri::AppHandle) -> Result<(), String> {
    let (daemon, engine) = bundled_paths(&app)?;
    blocking(move || install_impl(daemon, engine)).await
}

#[cfg(target_os = "linux")]
fn install_impl(daemon: PathBuf, engine: PathBuf) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // Root can't read inside an AppImage's FUSE mount, so stage the files first.
    let stage = std::env::temp_dir().join(format!("belfry-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(stage.join("engine")).map_err(|e| e.to_string())?;
    let mut files = vec![(daemon, stage.join("belfryd"))];
    for exe in ["easytier-core", "easytier-cli"] {
        files.push((engine.join(exe), stage.join("engine").join(exe)));
    }
    for (src, dst) in &files {
        std::fs::copy(src, dst).map_err(|e| format!("{}: {e}", src.display()))?;
        std::fs::set_permissions(dst, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let uid = std::fs::metadata(&stage).map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap_or(1000);
    let out = std::process::Command::new("pkexec")
        .arg(stage.join("belfryd"))
        .args(["install", "--from"])
        .arg(&stage)
        .args(["--owner-uid", &uid.to_string()])
        .output()
        .map_err(|e| format!("pkexec: {e}"))?;
    let _ = std::fs::remove_dir_all(&stage);
    match out.status.code() {
        Some(0) => wait_for_service(),
        Some(126) | Some(127) => Err("CANCELLED: authorization was cancelled".into()),
        _ => Err(format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout))),
    }
}

#[cfg(windows)]
fn install_impl(daemon: PathBuf, _engine: PathBuf) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList 'install' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode",
        daemon.display()
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        wait_for_service()
    } else {
        Err(format!("CANCELLED: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn install_impl(_daemon: PathBuf, _engine: PathBuf) -> Result<(), String> {
    Err("run `sudo belfryd run` yourself on this OS".into())
}

fn wait_for_service() -> Result<(), String> {
    for _ in 0..40 {
        if call::<Status>("GET", "/v1/status", None).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err("the service was installed but is not answering yet".into())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            create_room,
            join_room,
            set_room_active,
            set_nickname,
            delete_room,
            room_log,
            preview_code,
            app_info,
            install_service
        ])
        .run(tauri::generate_context!())
        .expect("error while running Belfry");
}
