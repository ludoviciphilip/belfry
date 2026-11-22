//! Loopback HTTP/JSON API used by the app. Every request needs the token
//! from the data directory, so web pages and other users can't drive it.

use crate::daemon::Daemon;
use belfry_proto::*;
use std::io::Read;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

pub fn load_or_create_token(daemon: &Daemon, owner_uid: Option<u32>) -> std::io::Result<String> {
    let path = daemon.data_dir.join("token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        if t.trim().len() >= 32 {
            crate::platform::restrict_to_owner(&path, owner_uid);
            return Ok(t.trim().to_string());
        }
    }
    let token = belfry_proto::code::hex(&belfry_proto::code::random_bytes::<32>());
    std::fs::write(&path, &token)?;
    crate::platform::restrict_to_owner(&path, owner_uid);
    Ok(token)
}

pub fn serve(daemon: Arc<Daemon>, listen: &str, token: String) -> std::io::Result<()> {
    let server = Arc::new(Server::http(listen).map_err(std::io::Error::other)?);
    let token = Arc::new(token);
    let mut workers = Vec::new();
    for _ in 0..3 {
        let (server, daemon, token) = (server.clone(), daemon.clone(), token.clone());
        workers.push(std::thread::spawn(move || {
            while let Ok(req) = server.recv() {
                handle(&daemon, &token, req);
            }
        }));
    }
    for w in workers {
        let _ = w.join();
    }
    Ok(())
}

fn json<T: serde::Serialize>(code: u16, body: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(serde_json::to_vec(body).unwrap_or_default())
        .with_status_code(code)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn err(code: u16, msg: impl Into<String>) -> Response<std::io::Cursor<Vec<u8>>> {
    json(code, &ApiError { error: msg.into() })
}

fn body<T: serde::de::DeserializeOwned>(req: &mut Request) -> Result<T, String> {
    let mut s = String::new();
    req.as_reader()
        .take(64 * 1024)
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| format!("bad request: {e}"))
}

fn handle(daemon: &Daemon, token: &str, mut req: Request) {
    let authorized = req.headers().iter().any(|h| {
        h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {token}")
    });
    if !authorized {
        let _ = req.respond(err(401, "missing or wrong token"));
        return;
    }
    let url = req.url().to_string();
    let parts: Vec<&str> = url.trim_matches('/').split('/').collect();
    let method = req.method().clone();
    let resp = match (&method, parts.as_slice()) {
        (Method::Get, ["v1", "status"]) => json(200, &daemon.status()),
        (Method::Post, ["v1", "rooms"]) => match body::<CreateRequest>(&mut req) {
            Ok(r) => result(daemon.create(r)),
            Err(e) => err(400, e),
        },
        (Method::Post, ["v1", "join"]) => match body::<JoinRequest>(&mut req) {
            Ok(r) => result(daemon.join(r)),
            Err(e) => err(400, e),
        },
        (Method::Post, ["v1", "rooms", id, "start"]) => result(daemon.set_active(id, true)),
        (Method::Post, ["v1", "rooms", id, "stop"]) => result(daemon.set_active(id, false)),
        (Method::Post, ["v1", "rooms", id, "nickname"]) => match body::<RenameRequest>(&mut req) {
            Ok(r) => result(daemon.set_nickname(id, &r.nickname)),
            Err(e) => err(400, e),
        },
        (Method::Delete, ["v1", "rooms", id]) => match daemon.delete(id) {
            Ok(()) => json(200, &serde_json::json!({"ok": true})),
            Err(e) => err(404, e),
        },
        (Method::Get, ["v1", "rooms", id, "log"]) => {
            Response::from_data(daemon.log_tail(id).into_bytes()).with_status_code(200)
        }
        _ => err(404, "no such endpoint"),
    };
    let _ = req.respond(resp);
}

fn result<T: serde::Serialize>(r: Result<T, String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(v) => json(200, &v),
        Err(e) => err(400, e),
    }
}
