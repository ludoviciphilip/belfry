// The Belfry window. All networking lives in the belfryd service; this side
// only forwards the UI's requests to it over loopback.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;

use belfry_proto::code::RoomCode;
use belfry_proto::*;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_updater::UpdaterExt;

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



// ---------- chat & files ----------

#[tauri::command]
async fn chat_list(id: String, since: u64) -> Result<Vec<ChatMsg>, String> {
    blocking(move || call("GET", &format!("/v1/rooms/{id}/chat?since={since}"), None)).await
}

/// Long-poll: returns the service's chat version once it changes (or ~25 s).
#[tauri::command]
async fn chat_wait(since: u64) -> Result<u64, String> {
    blocking(move || {
        let token = std::fs::read_to_string(token_path()).map_err(|e| format!("NO_TOKEN: {e}"))?;
        let r: serde_json::Value = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(800))
            .timeout(Duration::from_secs(35))
            .build()
            .get(&format!("{}/v1/chat/wait?since={since}", base_url()))
            .set("Authorization", &format!("Bearer {}", token.trim()))
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        r["version"].as_u64().ok_or_else(|| "bad answer".into())
    })
    .await
}

#[tauri::command]
async fn chat_send(id: String, text: String) -> Result<ChatMsg, String> {
    let body = serde_json::to_value(ChatPost { text, file: None }).unwrap();
    blocking(move || call("POST", &format!("/v1/rooms/{id}/chat"), Some(body))).await
}

/// Starts serving a file and posts it in the room's chat.
#[tauri::command]
async fn share_file(
    shares: tauri::State<'_, std::sync::Arc<files::Shares>>,
    id: String,
    path: String,
) -> Result<ChatMsg, String> {
    let shares = shares.inner().clone();
    blocking(move || {
        let status: Status = call("GET", "/v1/status", None)?;
        let room = status.rooms.into_iter().find(|r| r.id == id).ok_or("API: room not found")?;
        let share = files::new_share(std::path::Path::new(&path), &id, &room.peer_key, &room.cidr)?;
        let body = serde_json::to_value(ChatPost { text: String::new(), file: Some(share.offer()) }).unwrap();
        shares.add(share.clone());
        call::<ChatMsg>("POST", &format!("/v1/rooms/{id}/chat"), Some(body)).inspect_err(|_| shares.remove(&share.id))
    })
    .await
}

#[tauri::command]
fn unshare_file(shares: tauri::State<'_, std::sync::Arc<files::Shares>>, file_id: String) {
    shares.remove(&file_id);
}

#[tauri::command]
fn my_shares(shares: tauri::State<'_, std::sync::Arc<files::Shares>>) -> Vec<String> {
    shares.ids()
}

#[tauri::command]
fn forget_room_shares(shares: tauri::State<'_, std::sync::Arc<files::Shares>>, id: String) {
    shares.remove_room(&id);
}

fn download_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(d) = std::env::var("BELFRY_DOWNLOAD_DIR") {
        return d.into();
    }
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Belfry")
}

/// Progress arrives as `download-progress` events.
#[tauri::command]
fn download_file(app: tauri::AppHandle, msg_id: String, ip: String, file: FileOffer, key: String) {
    let dir = download_dir(&app);
    let emitter = app.clone();
    files::download(move |p| { let _ = emitter.emit("download-progress", p); }, dir, msg_id, ip, file, key);
}

// ---------- tray ----------

/// Tray state shared with the window-close handler.
struct Tray {
    available: AtomicBool,
    /// What the X button does: "ask" (UI shows a dialog), "tray" or "quit".
    close_action: Mutex<String>,
    open: Mutex<Option<MenuItem<tauri::Wry>>>,
    quit: Mutex<Option<MenuItem<tauri::Wry>>>,
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Linux trays need a StatusNotifier host (KDE, GNOME + extension, Waybar,
/// Caelestia...). Without one the icon would be invisible, so closing must
/// really close.
#[cfg(target_os = "linux")]
fn tray_host_present() -> bool {
    std::process::Command::new("busctl")
        .args(["--user", "--no-pager", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("StatusNotifierWatcher"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn tray_host_present() -> bool {
    true
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Belfry", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Belfry", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Belfry")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    let state = app.state::<Tray>();
    *state.open.lock().unwrap() = Some(open);
    *state.quit.lock().unwrap() = Some(quit);
    state.available.store(tray_host_present(), Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn tray_available(tray: tauri::State<Tray>) -> bool {
    tray.available.load(Ordering::Relaxed)
}

/// The UI sends its translated labels and a status line for the tooltip.
#[tauri::command]
fn update_tray(app: tauri::AppHandle, tray: tauri::State<Tray>, open: String, quit: String, tooltip: String) {
    if let Some(i) = tray.open.lock().unwrap().as_ref() {
        let _ = i.set_text(open);
    }
    if let Some(i) = tray.quit.lock().unwrap().as_ref() {
        let _ = i.set_text(quit);
    }
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_tooltip(Some(tooltip));
    }
}

#[tauri::command]
fn set_close_action(tray: tauri::State<Tray>, action: String) {
    if ["ask", "tray", "quit"].contains(&action.as_str()) {
        *tray.close_action.lock().unwrap() = action;
    }
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

// ---------- updates ----------

#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
    /// false = this kind of install can't replace itself (deb/rpm): send the
    /// user to the releases page instead.
    can_install: bool,
}

fn self_updatable() -> bool {
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some()
    } else {
        true
    }
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone(), can_install: self_updatable() }))
}

/// Downloads, verifies the signature, installs and restarts. Progress goes
/// to the UI as `update-progress` events (downloaded, total bytes).
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no update available")?;
    let mut done: u64 = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("update-progress", (done, total));
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // Windows never gets here: the installer closes the app and reopens it.
    app.restart();
}

fn main() {
    tauri::Builder::default()
        .manage(Tray {
            available: AtomicBool::new(false),
            close_action: Mutex::new("ask".into()),
            open: Mutex::new(None),
            quit: Mutex::new(None),
        })
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
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
            install_service,
            tray_available,
            update_tray,
            set_close_action,
            hide_window,
            quit_app,
            check_update,
            install_update,
            chat_list,
            chat_wait,
            chat_send,
            share_file,
            unshare_file,
            my_shares,
            forget_room_shares,
            download_file
        ])
        .setup(|app| {
            let shares_file = std::env::var("BELFRY_SHARES_FILE").map(PathBuf::from).unwrap_or_else(|_| {
                app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from(".")).join("shares.json")
            });
            let shares = files::Shares::load(shares_file);
            {
                let shares = shares.clone();
                std::thread::spawn(move || files::serve(shares));
            }
            app.manage(shares);
            if let Err(e) = build_tray(app) {
                eprintln!("tray unavailable: {e}");
            }
            // Release check: BELFRY_SELFTEST_UPDATE=1 updates without any clicks
            // and logs each step to stderr.
            if std::env::var_os("BELFRY_SELFTEST_UPDATE").is_some() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    eprintln!("selftest: running {VERSION}, checking for updates");
                    match check_update(handle.clone()).await {
                        Ok(Some(u)) => {
                            eprintln!("selftest: found {} (can_install={})", u.version, u.can_install);
                            if let Err(e) = install_update(handle).await {
                                eprintln!("selftest: install failed: {e}");
                            }
                        }
                        Ok(None) => eprintln!("selftest: no update"),
                        Err(e) => eprintln!("selftest: check failed: {e}"),
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // With a tray, the X button hides the window; the UI decides
            // (first time it asks). Without a tray, X closes as usual.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let tray = window.app_handle().state::<Tray>();
                if !tray.available.load(Ordering::Relaxed) {
                    return;
                }
                let action = tray.close_action.lock().unwrap().clone();
                match action.as_str() {
                    "quit" => {}
                    "tray" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {
                        api.prevent_close();
                        let _ = window.emit("close-requested", ());
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Belfry");
}
