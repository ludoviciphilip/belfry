//! Files the user shares in a room's chat are served straight from this app
//! (with the user's own permissions) to other members, who download them
//! into Downloads/Belfry. Nothing goes through the service or any server.

use belfry_proto::{in_cidr, FileOffer, FILE_PORT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone)]
pub struct Share {
    pub id: String,
    pub token: String,
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub room_id: String,
    pub key: String,
    pub cidr: String,
}

pub struct Shares {
    file: PathBuf,
    map: Mutex<HashMap<String, Share>>,
}

impl Shares {
    pub fn load(file: PathBuf) -> Arc<Shares> {
        let map: HashMap<String, Share> = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Arc::new(Shares { file, map: Mutex::new(map) })
    }

    fn save(&self, map: &HashMap<String, Share>) {
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(b) = serde_json::to_vec_pretty(map) {
            let _ = std::fs::write(&self.file, b);
        }
    }

    pub fn add(&self, share: Share) {
        let mut map = self.map.lock().unwrap();
        map.insert(share.id.clone(), share);
        self.save(&map);
    }

    pub fn remove(&self, id: &str) {
        let mut map = self.map.lock().unwrap();
        map.remove(id);
        self.save(&map);
    }

    pub fn remove_room(&self, room_id: &str) {
        let mut map = self.map.lock().unwrap();
        map.retain(|_, s| s.room_id != room_id);
        self.save(&map);
    }

    pub fn ids(&self) -> Vec<String> {
        self.map.lock().unwrap().keys().cloned().collect()
    }

    fn get(&self, id: &str) -> Option<Share> {
        self.map.lock().unwrap().get(id).cloned()
    }
}

pub fn new_share(path: &Path, room_id: &str, key: &str, cidr: &str) -> Result<Share, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err("only files can be shared (zip a folder first)".into());
    }
    // Check now that we can read it, rather than when a friend asks.
    std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    Ok(Share {
        id: belfry_proto::code::hex(&belfry_proto::code::random_bytes::<9>()),
        token: belfry_proto::code::hex(&belfry_proto::code::random_bytes::<16>()),
        path: path.to_path_buf(),
        name,
        size: meta.len(),
        room_id: room_id.into(),
        key: key.into(),
        cidr: cidr.into(),
    })
}

impl Share {
    pub fn offer(&self) -> FileOffer {
        FileOffer { id: self.id.clone(), name: self.name.clone(), size: self.size, token: self.token.clone() }
    }
}

fn file_port() -> u16 {
    std::env::var("BELFRY_FILE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(FILE_PORT)
}

/// HTTP file server for room members: GET /f/<id>?t=<token>.
pub fn serve(shares: Arc<Shares>) {
    let port = file_port();
    let server = match tiny_http::Server::http(("0.0.0.0", port)) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("file sharing unavailable (port {port}): {e}");
            return;
        }
    };
    for _ in 0..4 {
        let (server, shares) = (server.clone(), shares.clone());
        std::thread::spawn(move || {
            while let Ok(req) = server.recv() {
                let resp = answer(&shares, &req);
                let _ = match resp {
                    Ok((file, share)) => {
                        let disp = format!("attachment; filename=\"{}\"", share.name.replace('"', "'"));
                        let r = tiny_http::Response::from_file(file)
                            .with_header(tiny_http::Header::from_bytes("Content-Type", "application/octet-stream").unwrap())
                            .with_header(tiny_http::Header::from_bytes("Content-Disposition", disp.as_bytes()).unwrap());
                        req.respond(r)
                    }
                    Err((code, msg)) => req.respond(tiny_http::Response::from_string(msg).with_status_code(code)),
                };
            }
        });
    }
}

fn answer(shares: &Shares, req: &tiny_http::Request) -> Result<(std::fs::File, Share), (u16, String)> {
    let url = req.url();
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let id = path.strip_prefix("/f/").ok_or((404, "not found".to_string()))?;
    let token = query.split('&').find_map(|kv| kv.strip_prefix("t=")).unwrap_or("");
    let key = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Belfry-Key"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let share = shares.get(id).ok_or((404, "this file is no longer shared".to_string()))?;
    let from = req.remote_addr().map(|a| a.ip()).ok_or((400, "no address".to_string()))?;
    if share.token != token || share.key != key || !in_cidr(&from, &share.cidr) {
        return Err((403, "forbidden".into()));
    }
    let file = std::fs::File::open(&share.path).map_err(|_| (410, "the file was moved or deleted".to_string()))?;
    Ok((file, share))
}

#[derive(Serialize, Clone)]
pub struct Progress {
    pub msg_id: String,
    pub done: u64,
    pub total: u64,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// Picks Downloads/Belfry/<name>, adding " (2)" etc. if taken.
fn target_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

/// Streams a file from a member's app into `dir`, reporting progress.
pub fn download(
    emit: impl Fn(Progress) + Send + 'static,
    dir: PathBuf,
    msg_id: String,
    ip: String,
    offer: FileOffer,
    key: String,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let fail = |e: String| Progress { msg_id: msg_id.clone(), done: 0, total: offer.size, path: None, error: Some(e) };
        let port = file_port();
        let url = format!("http://{ip}:{port}/f/{}?t={}", offer.id, offer.token);
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(30)).build();
        let resp = match agent.get(&url).set("X-Belfry-Key", &key).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => return emit(fail(r.into_string().unwrap_or_default())),
            Err(e) => return emit(fail(format!("can't reach the sender: {e}"))),
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return emit(fail(format!("can't create {}", dir.display())));
        }
        let name = belfry_proto::sanitize_file_name(&offer.name);
        let final_path = target_path(&dir, &name);
        let part = final_path.with_file_name(format!(".{}.part", name));
        let mut out = match std::fs::File::create(&part) {
            Ok(f) => f,
            Err(e) => return emit(fail(e.to_string())),
        };
        let total = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(offer.size);
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 256 * 1024];
        let (mut done, mut last) = (0u64, Instant::now());
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = out.write_all(&buf[..n]) {
                        let _ = std::fs::remove_file(&part);
                        return emit(fail(e.to_string()));
                    }
                    done += n as u64;
                    if last.elapsed() > Duration::from_millis(150) {
                        last = Instant::now();
                        emit(Progress { msg_id: msg_id.clone(), done, total, path: None, error: None });
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    return emit(fail(format!("transfer interrupted: {e}")));
                }
            }
        }
        drop(out);
        if done != total {
            let _ = std::fs::remove_file(&part);
            return emit(fail("transfer interrupted".into()));
        }
        if let Err(e) = std::fs::rename(&part, &final_path) {
            return emit(fail(e.to_string()));
        }
        emit(Progress {
            msg_id: msg_id.clone(),
            done,
            total,
            path: Some(final_path.to_string_lossy().into_owned()),
            error: None,
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn serve_and_download() {
        let tmp = std::env::temp_dir().join(format!("belfry-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        let data: Vec<u8> = (0..5_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let src = tmp.join("src").join("Save do Bloodborne.zip");
        std::fs::write(&src, &data).unwrap();

        std::env::set_var("BELFRY_FILE_PORT", "47999");
        let shares = Shares::load(tmp.join("shares.json"));
        let share = new_share(&src, "room1", "k3y", "127.0.0.0/24").unwrap();
        shares.add(share.clone());
        {
            let s = shares.clone();
            std::thread::spawn(move || serve(s));
        }
        std::thread::sleep(Duration::from_millis(300));

        let get = |offer: FileOffer, key: &str| {
            let (tx, rx) = mpsc::channel();
            download(move |p| { let _ = tx.send(p); }, tmp.join("dl"), "m1".into(), "127.0.0.1".into(), offer, key.into())
                .join()
                .unwrap();
            rx.try_iter().last().unwrap()
        };
        // Right token and key: identical bytes.
        let p = get(share.offer(), "k3y");
        assert!(p.error.is_none(), "{:?}", p.error);
        assert_eq!(std::fs::read(p.path.as_ref().unwrap()).unwrap(), data);
        // Same name again: "(2)", nothing overwritten.
        let p2 = get(share.offer(), "k3y");
        assert!(p2.path.unwrap().ends_with("Save do Bloodborne (2).zip"));
        // Wrong key or token: refused, no file written.
        assert!(get(share.offer(), "nope").error.is_some());
        let mut bad = share.offer();
        bad.token = "0".repeat(32);
        assert!(get(bad, "k3y").error.is_some());
        // Unshared: gone.
        shares.remove(&share.id);
        let e = get(share.offer(), "k3y").error.unwrap();
        assert!(e.contains("no longer shared"), "{e}");
        assert_eq!(std::fs::read_dir(tmp.join("dl")).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
