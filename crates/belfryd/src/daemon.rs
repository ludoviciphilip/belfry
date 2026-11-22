//! Rooms, their engine processes and the status the app shows.

use crate::engine::{self, Engine};
use belfry_proto::code::{hex, RoomCode};
use belfry_proto::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What is saved to rooms.json.
#[derive(Serialize, Deserialize, Clone)]
pub struct Room {
    /// Canonical code. For hosts it has no addresses (they change); for
    /// guests it is exactly what they pasted.
    pub code: String,
    pub role: Role,
    pub nickname: String,
    pub active: bool,
    pub created: u64,
}

impl Room {
    pub fn parsed(&self) -> RoomCode {
        RoomCode::decode(&self.code).expect("stored code is valid")
    }
    pub fn id(&self) -> String {
        hex(&self.parsed().id)
    }
}

struct Run {
    child: Option<Child>,
    rpc_port: u16,
    started: Instant,
    failures: u32,
    retry_at: Instant,
    last_error: Option<String>,
    ever_online: bool,
    marked_private: bool,
}

#[derive(Default, Clone)]
struct Public {
    v4: Option<Ipv4Addr>,
    v6: Option<Ipv6Addr>,
}

pub struct Daemon {
    pub data_dir: PathBuf,
    pub chat: crate::chat::ChatStore,
    pub engine: Engine,
    engine_version: String,
    rooms: Mutex<Vec<Room>>,
    runs: Mutex<HashMap<String, Run>>,
    public: Mutex<Public>,
    /// Last computed view of each room. Computing one asks the engine
    /// (100-400 ms), so status and chat answer from here.
    views: Mutex<HashMap<String, (Instant, RoomView)>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn dev_name(id: &str) -> String {
    format!("blf{}", &id[..6])
}

fn port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok() && UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

impl Daemon {
    pub fn new(data_dir: PathBuf, engine: Engine) -> Arc<Daemon> {
        std::fs::create_dir_all(data_dir.join("logs")).ok();
        let rooms: Vec<Room> = std::fs::read(data_dir.join("rooms.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let rooms = rooms.into_iter().filter(|r| RoomCode::decode(&r.code).is_ok()).collect();
        let engine_version = engine.version();
        Arc::new(Daemon {
            chat: crate::chat::ChatStore::new(data_dir.join("chat")),
            data_dir,
            engine,
            engine_version,
            rooms: Mutex::new(rooms),
            runs: Mutex::new(HashMap::new()),
            public: Mutex::new(Public::default()),
            views: Mutex::new(HashMap::new()),
        })
    }

    fn save(&self, rooms: &[Room]) {
        let tmp = self.data_dir.join("rooms.json.tmp");
        if let Ok(b) = serde_json::to_vec_pretty(rooms) {
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, self.data_dir.join("rooms.json"));
            }
        }
    }

    fn log_path(&self, id: &str) -> PathBuf {
        self.data_dir.join("logs").join(format!("{id}.log"))
    }

    pub fn create(&self, req: CreateRequest) -> Result<RoomView, String> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err("give the room a name".into());
        }
        let relay = match req.relay.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()) {
            Some(r) if !valid_peer_url(&r) => return Err(format!("invalid relay address: {r}")),
            r => r,
        };
        let mut rooms = self.rooms.lock().unwrap();
        let used: Vec<u16> = rooms
            .iter()
            .filter(|r| r.role == Role::Host)
            .map(|r| r.parsed().port)
            .collect();
        let port = (DEFAULT_HOST_PORT..DEFAULT_HOST_PORT + 50)
            .find(|p| !used.contains(p) && port_free(*p))
            .ok_or("no free port between 11010 and 11060")?;
        let mut code = RoomCode::generate(name, port);
        code.relay = relay;
        let room = Room {
            code: code.encode(),
            role: Role::Host,
            nickname: clean_nickname(&req.nickname),
            active: true,
            created: now(),
        };
        let id = hex(&code.id);
        rooms.push(room);
        self.save(&rooms);
        drop(rooms);
        self.refresh_public();
        self.view(&id).ok_or_else(|| "room vanished".into())
    }

    pub fn join(&self, req: JoinRequest) -> Result<RoomView, String> {
        let code = RoomCode::decode(&req.code)?;
        if code.peer_urls().is_empty() {
            return Err("this code has no address to reach the host; ask for a new one".into());
        }
        let id = hex(&code.id);
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(r) = rooms.iter_mut().find(|r| r.id() == id) {
            if r.role == Role::Host {
                return Err("you are the host of this room".into());
            }
            // A newer code from the same host (e.g. their IP changed).
            r.code = code.encode();
            r.nickname = clean_nickname(&req.nickname);
            r.active = true;
            self.runs.lock().unwrap().remove(&id).map(kill_run);
        } else {
            rooms.push(Room {
                code: code.encode(),
                role: Role::Guest,
                nickname: clean_nickname(&req.nickname),
                active: true,
                created: now(),
            });
        }
        self.save(&rooms);
        drop(rooms);
        self.view(&id).ok_or_else(|| "room vanished".into())
    }

    pub fn set_active(&self, id: &str, active: bool) -> Result<RoomView, String> {
        let mut rooms = self.rooms.lock().unwrap();
        let r = rooms.iter_mut().find(|r| r.id() == id).ok_or("room not found")?;
        r.active = active;
        self.save(&rooms);
        drop(rooms);
        if !active {
            self.runs.lock().unwrap().remove(id).map(kill_run);
        }
        self.view(id).ok_or_else(|| "room vanished".into())
    }

    pub fn set_nickname(&self, id: &str, nick: &str) -> Result<RoomView, String> {
        let mut rooms = self.rooms.lock().unwrap();
        let r = rooms.iter_mut().find(|r| r.id() == id).ok_or("room not found")?;
        r.nickname = clean_nickname(nick);
        self.save(&rooms);
        drop(rooms);
        // Restart so the new name reaches everyone.
        self.runs.lock().unwrap().remove(id).map(kill_run);
        self.view(id).ok_or_else(|| "room vanished".into())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut rooms = self.rooms.lock().unwrap();
        let before = rooms.len();
        rooms.retain(|r| r.id() != id);
        if rooms.len() == before {
            return Err("room not found".into());
        }
        self.save(&rooms);
        drop(rooms);
        self.runs.lock().unwrap().remove(id).map(kill_run);
        let _ = std::fs::remove_file(self.log_path(id));
        self.chat.delete(id);
        Ok(())
    }

    pub fn log_tail(&self, id: &str) -> String {
        let data = std::fs::read(self.log_path(id)).unwrap_or_default();
        let tail = &data[data.len().saturating_sub(16384)..];
        engine::strip_ansi(&String::from_utf8_lossy(tail))
    }

    pub fn refresh_public(&self) {
        let p = Public { v4: crate::stun::public_v4(), v6: crate::stun::public_v6() };
        *self.public.lock().unwrap() = p;
    }

    fn engine_args(&self, room: &Room, rpc_port: u16) -> Vec<String> {
        let c = room.parsed();
        let id = hex(&c.id);
        let mut a: Vec<String> = vec![
            "--network-name".into(),
            c.network_name(),
            "--network-secret".into(),
            c.network_secret(),
            "--hostname".into(),
            room.nickname.clone(),
            "--rpc-portal".into(),
            format!("127.0.0.1:{rpc_port}"),
            "--dev-name".into(),
            dev_name(&id),
            // Only members of this room may use this PC as a relay.
            "--private-mode".into(),
            "true".into(),
            "--latency-first".into(),
            "true".into(),
            "--console-log-level".into(),
            "warn".into(),
        ];
        match room.role {
            Role::Host => {
                a.extend(["-i".into(), format!("{}/24", c.host_ip())]);
                a.push("-l".into());
                a.push(format!("tcp://0.0.0.0:{}", c.port));
                a.push(format!("udp://0.0.0.0:{}", c.port));
                a.push(format!("tcp://[::]:{}", c.port));
                a.push(format!("udp://[::]:{}", c.port));
                if let Some(r) = &c.relay {
                    a.extend(["-p".into(), r.clone()]);
                }
            }
            Role::Guest => {
                a.extend(["-d".into(), "-i".into(), c.cidr()]);
                a.push("-p".into());
                a.extend(c.peer_urls());
                a.push("-l".into());
                a.push("tcp://0.0.0.0:0".into());
                a.push("udp://0.0.0.0:0".into());
            }
        }
        a
    }

    /// Called every second: starts, restarts and stops engine processes.
    pub fn supervise(&self) {
        let rooms = self.rooms.lock().unwrap().clone();
        let mut runs = self.runs.lock().unwrap();
        let wanted: Vec<String> = rooms.iter().filter(|r| r.active).map(Room::id).collect();
        let stale: Vec<String> = runs.keys().filter(|k| !wanted.contains(k)).cloned().collect();
        for k in stale {
            runs.remove(&k).map(kill_run);
        }
        let mut taken: Vec<u16> = runs.values().map(|r| r.rpc_port).collect();
        for room in rooms.iter().filter(|r| r.active) {
            let id = room.id();
            let run = runs.entry(id.clone()).or_insert_with(|| Run {
                child: None,
                rpc_port: 0,
                started: Instant::now(),
                failures: 0,
                retry_at: Instant::now(),
                last_error: None,
                ever_online: false,
                marked_private: false,
            });
            // Exited?
            if let Some(child) = run.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    run.child = None;
                    run.failures += 1;
                    let why = engine::last_error(&self.log_path(&id))
                        .unwrap_or_else(|| format!("engine exited ({status})"));
                    run.last_error = Some(why);
                    let wait = (2u64 << run.failures.min(4)).min(30);
                    run.retry_at = Instant::now() + Duration::from_secs(wait);
                }
            }
            if run.child.is_none() && Instant::now() >= run.retry_at {
                let rpc = (15900u16..16900)
                    .find(|p| !taken.contains(p) && TcpListener::bind(("127.0.0.1", *p)).is_ok())
                    .unwrap_or(15900);
                taken.push(rpc);
                if room.role == Role::Host && !port_free(room.parsed().port) {
                    run.last_error = Some(format!(
                        "port {} is in use by another program",
                        room.parsed().port
                    ));
                    run.retry_at = Instant::now() + Duration::from_secs(5);
                    continue;
                }
                let args = self.engine_args(room, rpc);
                match self.engine.spawn(&args, &self.log_path(&id)) {
                    Ok(child) => {
                        run.child = Some(child);
                        run.rpc_port = rpc;
                        run.started = Instant::now();
                        run.marked_private = false;
                    }
                    Err(e) => {
                        run.failures += 1;
                        run.last_error = Some(format!("could not start the engine: {e}"));
                        run.retry_at = Instant::now() + Duration::from_secs(10);
                    }
                }
            }
        }
    }


    // ---------- chat ----------

    fn room_by_id(&self, id: &str) -> Option<Room> {
        self.rooms.lock().unwrap().iter().find(|r| r.id() == id).cloned()
    }

    /// A message typed in this machine's app: store it, then deliver it.
    pub fn post_chat(&self, id: &str, post: ChatPost) -> Result<ChatMsg, String> {
        let text: String = post.text.trim().chars().take(MAX_CHAT_TEXT).collect();
        if text.is_empty() && post.file.is_none() {
            return Err("empty message".into());
        }
        let v = self.cached_view(id).ok_or("room not found")?;
        let my_ip = match (v.state, v.my_ip.clone()) {
            (RoomState::Online, Some(ip)) => ip,
            _ => return Err("the room is not connected".into()),
        };
        let msg = ChatMsg {
            id: hex(&belfry_proto::code::random_bytes::<12>()),
            seq: 0,
            ts: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            from: v.nickname.clone(),
            from_ip: my_ip,
            text,
            file: post.file,
            mine: true,
        };
        let stored = self.chat.insert(id, msg).ok_or("duplicate message")?;
        let targets: Vec<String> =
            v.members.iter().filter(|m| m.link != Link::Me).map(|m| m.ip.clone()).collect();
        let (room_id, key, out) = (id.to_string(), v.peer_key.clone(), stored.clone());
        std::thread::spawn(move || {
            let workers: Vec<_> = targets
                .into_iter()
                .map(|ip| {
                    let (room_id, key, out) = (room_id.clone(), key.clone(), out.clone());
                    std::thread::spawn(move || {
                        // Members who miss it catch up from the host's history.
                        let _ = crate::chat::send(&ip, &room_id, &key, &out);
                    })
                })
                .collect();
            for w in workers {
                let _ = w.join();
            }
        });
        Ok(stored)
    }

    /// Checks a request from another member: right key, from inside the room.
    fn peer_ok(&self, id: &str, key: &str, from: &std::net::IpAddr) -> Result<Room, String> {
        let room = self.room_by_id(id).ok_or("unknown room")?;
        let c = room.parsed();
        if !room.active || key != peer_key(&c.network_secret()) {
            return Err("wrong key".into());
        }
        if !in_cidr(from, &c.cidr()) {
            return Err("not from the room network".into());
        }
        Ok(room)
    }

    pub fn peer_message(&self, id: &str, key: &str, from: &std::net::IpAddr, mut msg: ChatMsg) -> Result<(), String> {
        self.peer_ok(id, key, from)?;
        msg.text = msg.text.chars().take(MAX_CHAT_TEXT).collect();
        msg.from = clean_nickname(&msg.from);
        msg.from_ip = from.to_string();
        msg.mine = false;
        if let Some(f) = msg.file.as_mut() {
            f.name = belfry_proto::sanitize_file_name(&f.name);
        }
        self.chat.insert(id, msg);
        Ok(())
    }

    pub fn peer_history(&self, id: &str, key: &str, from: &std::net::IpAddr) -> Result<Vec<ChatMsg>, String> {
        self.peer_ok(id, key, from)?;
        Ok(self.chat.recent(id, 200))
    }

    /// Guests pull the host's recent history to fill gaps.
    pub fn sync_chats(&self) {
        let rooms: Vec<Room> = self
            .rooms
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.active && r.role == Role::Guest)
            .cloned()
            .collect();
        for room in rooms {
            let id = room.id();
            let Some(v) = self.view(&id) else { continue };
            let Some(my_ip) = v.my_ip.clone() else { continue };
            if v.state != RoomState::Online {
                continue;
            }
            if let Ok(msgs) = crate::chat::fetch_history(&v.host_ip, &id, &v.peer_key) {
                for mut m in msgs {
                    m.mine = m.from_ip == my_ip;
                    m.from = clean_nickname(&m.from);
                    if let Some(f) = m.file.as_mut() {
                        f.name = belfry_proto::sanitize_file_name(&f.name);
                    }
                    self.chat.insert(&id, m);
                }
            }
        }
    }

    pub fn stop_all(&self) {
        let mut runs = self.runs.lock().unwrap();
        for (_, r) in runs.drain() {
            kill_run(r);
        }
    }

    pub fn status(&self) -> Status {
        let ids: Vec<String> = self.rooms.lock().unwrap().iter().map(Room::id).collect();
        Status {
            version: VERSION.into(),
            engine_version: self.engine_version.clone(),
            rooms: ids.iter().filter_map(|id| self.cached_view(id)).collect(),
        }
    }

    /// Fresh view (asks the engine), also stored in the cache.
    pub fn view(&self, id: &str) -> Option<RoomView> {
        let v = self.compute_view(id);
        let mut views = self.views.lock().unwrap();
        match &v {
            Some(v) => {
                views.insert(id.to_string(), (Instant::now(), v.clone()));
            }
            None => {
                views.remove(id);
            }
        }
        v
    }

    /// The cached view if it is recent, else a fresh one. Chat counters are
    /// always current.
    pub fn cached_view(&self, id: &str) -> Option<RoomView> {
        let hit = self
            .views
            .lock()
            .unwrap()
            .get(id)
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(5))
            .map(|(_, v)| v.clone());
        let mut v = match hit {
            Some(v) => v,
            None => return self.view(id),
        };
        v.chat_seq = self.chat.last_seq(id);
        Some(v)
    }

    /// Keeps the cache warm (called from a background thread).
    pub fn refresh_views(&self) {
        let ids: Vec<String> = self.rooms.lock().unwrap().iter().map(Room::id).collect();
        for id in ids {
            self.view(&id);
        }
    }

    fn compute_view(&self, id: &str) -> Option<RoomView> {
        let room = self.rooms.lock().unwrap().iter().find(|r| r.id() == id)?.clone();
        let c = room.parsed();
        let host_ip = c.host_ip().to_string();
        let mut v = RoomView {
            id: id.into(),
            name: c.name.clone(),
            role: room.role,
            nickname: room.nickname.clone(),
            active: room.active,
            state: RoomState::Off,
            error: None,
            my_ip: None,
            host_ip: host_ip.clone(),
            members: vec![],
            code: None,
            public_addrs: vec![],
            port: c.port,
            chat_seq: self.chat.last_seq(id),
            peer_key: peer_key(&c.network_secret()),
            cidr: c.cidr(),
        };
        if room.role == Role::Host {
            let p = self.public.lock().unwrap().clone();
            let mut full = c.clone();
            full.v4 = p.v4;
            full.v6 = p.v6;
            v.public_addrs = full
                .v4
                .map(|i| i.to_string())
                .into_iter()
                .chain(full.v6.map(|i| i.to_string()))
                .collect();
            v.code = Some(full.encode());
        }
        if !room.active {
            return Some(v);
        }
        let (rpc, alive, started, last_error, ever_online) = {
            let runs = self.runs.lock().unwrap();
            match runs.get(id) {
                Some(r) => (r.rpc_port, r.child.is_some(), r.started, r.last_error.clone(), r.ever_online),
                None => (0, false, Instant::now(), None, false),
            }
        };
        if !alive {
            v.state = if last_error.is_some() { RoomState::Error } else { RoomState::Starting };
            v.error = last_error;
            return Some(v);
        }
        match self.engine.peers(rpc) {
            Ok(peers) => {
                for p in peers {
                    let ip = p.ipv4.clone();
                    let link = if p.cost.eq_ignore_ascii_case("local") {
                        Link::Me
                    } else if p.cost.starts_with("p2p") {
                        Link::Direct
                    } else {
                        Link::Relay
                    };
                    // Nodes without an address are relays, not players; x.x.x.0
                    // means the engine is still picking an address for them.
                    if ip.is_empty() || ip == "-" || ip.ends_with(".0") {
                        continue;
                    }
                    if link == Link::Me {
                        v.my_ip = Some(ip.clone());
                    }
                    v.members.push(Member {
                        name: p.hostname,
                        is_host: ip == host_ip,
                        ip,
                        latency_ms: p.lat_ms.parse().ok(),
                        link,
                        protocol: p.tunnel_proto,
                    });
                }
                v.members.sort_by_key(|m| (!m.is_host, m.link != Link::Me, m.ip.clone()));
                let host_seen = room.role == Role::Host || v.members.iter().any(|m| m.is_host);
                if v.my_ip.is_some() && host_seen {
                    v.state = RoomState::Online;
                    let mut runs = self.runs.lock().unwrap();
                    if let Some(r) = runs.get_mut(id) {
                        r.ever_online = true;
                        r.last_error = None;
                        r.failures = 0;
                        if !r.marked_private {
                            r.marked_private = true;
                            let dev = dev_name(id);
                            std::thread::spawn(move || crate::platform::mark_adapter_private(&dev));
                        }
                    }
                } else {
                    v.state = RoomState::Starting;
                    if room.role == Role::Guest && started.elapsed() > Duration::from_secs(25) {
                        v.state = RoomState::Error;
                        v.error = Some(if ever_online {
                            "lost the host; reconnecting".into()
                        } else {
                            "can't reach the host yet: check that their Belfry is on, or ask for a fresh code".into()
                        });
                    }
                }
            }
            Err(e) => {
                v.state = if started.elapsed() < Duration::from_secs(15) {
                    RoomState::Starting
                } else {
                    v.error = Some(format!("engine not answering: {e}"));
                    RoomState::Error
                };
            }
        }
        Some(v)
    }
}

fn kill_run(mut r: Run) {
    if let Some(mut c) = r.child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}
