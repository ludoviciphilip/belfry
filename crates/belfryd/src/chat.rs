//! Room chat: stored by every member's service, sent directly to every other
//! member, and kept by the host so members who were away catch up.

use belfry_proto::{ChatMsg, PEER_PORT};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

const KEEP: usize = 500;

#[derive(Default)]
struct RoomChat {
    msgs: Vec<ChatMsg>,
    next_seq: u64,
}

pub struct ChatStore {
    dir: PathBuf,
    rooms: Mutex<HashMap<String, RoomChat>>,
    /// Bumped on every new message; the app long-polls on it.
    version: Mutex<u64>,
    changed: Condvar,
}

impl ChatStore {
    pub fn new(dir: PathBuf) -> ChatStore {
        let _ = std::fs::create_dir_all(&dir);
        ChatStore { dir, rooms: Mutex::new(HashMap::new()), version: Mutex::new(1), changed: Condvar::new() }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn with<R>(&self, id: &str, f: impl FnOnce(&mut RoomChat) -> R) -> R {
        let mut rooms = self.rooms.lock().unwrap();
        let chat = rooms.entry(id.to_string()).or_insert_with(|| {
            let msgs: Vec<ChatMsg> = std::fs::read(self.path(id))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let next_seq = msgs.iter().map(|m| m.seq).max().unwrap_or(0) + 1;
            RoomChat { msgs, next_seq }
        });
        f(chat)
    }

    fn save(&self, id: &str, chat: &RoomChat) {
        let tmp = self.path(id).with_extension("tmp");
        if let Ok(b) = serde_json::to_vec(&chat.msgs) {
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, self.path(id));
            }
        }
    }

    /// Stores a message unless it is already known. Returns it with its seq.
    pub fn insert(&self, id: &str, mut msg: ChatMsg) -> Option<ChatMsg> {
        let stored = self.with(id, |c| {
            if c.msgs.iter().any(|m| m.id == msg.id) {
                return None;
            }
            msg.seq = c.next_seq;
            c.next_seq += 1;
            c.msgs.push(msg.clone());
            if c.msgs.len() > KEEP {
                let drop = c.msgs.len() - KEEP;
                c.msgs.drain(..drop);
            }
            Some(msg)
        });
        if stored.is_some() {
            {
                let rooms = self.rooms.lock().unwrap();
                if let Some(c) = rooms.get(id) {
                    self.save(id, c);
                }
            }
            self.bump();
        }
        stored
    }

    pub fn since(&self, id: &str, seq: u64) -> Vec<ChatMsg> {
        self.with(id, |c| c.msgs.iter().filter(|m| m.seq > seq).cloned().collect())
    }

    pub fn last_seq(&self, id: &str) -> u64 {
        self.with(id, |c| c.msgs.last().map(|m| m.seq).unwrap_or(0))
    }

    pub fn recent(&self, id: &str, n: usize) -> Vec<ChatMsg> {
        self.with(id, |c| c.msgs[c.msgs.len().saturating_sub(n)..].to_vec())
    }

    pub fn delete(&self, id: &str) {
        self.rooms.lock().unwrap().remove(id);
        let _ = std::fs::remove_file(self.path(id));
        self.bump();
    }

    fn bump(&self) {
        *self.version.lock().unwrap() += 1;
        self.changed.notify_all();
    }

    /// Returns as soon as the version differs from `since`, or after `limit`.
    pub fn wait(&self, since: u64, limit: Duration) -> u64 {
        let guard = self.version.lock().unwrap();
        let (guard, _) = self.changed.wait_timeout_while(guard, limit, |v| *v == since).unwrap();
        *guard
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
}

/// Sends one message to one member's service.
pub fn send(ip: &str, room_id: &str, key: &str, msg: &ChatMsg) -> Result<(), String> {
    agent()
        .post(&format!("http://{ip}:{PEER_PORT}/p2p/v1/msg"))
        .set("X-Belfry-Room", room_id)
        .set("X-Belfry-Key", key)
        .send_json(msg)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The host's recent history, for catching up.
pub fn fetch_history(ip: &str, room_id: &str, key: &str) -> Result<Vec<ChatMsg>, String> {
    agent()
        .get(&format!("http://{ip}:{PEER_PORT}/p2p/v1/history"))
        .set("X-Belfry-Room", room_id)
        .set("X-Belfry-Key", key)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_wakes_on_insert() {
        let dir = std::env::temp_dir().join(format!("belfry-chat-{}", std::process::id()));
        let store = std::sync::Arc::new(ChatStore::new(dir.clone()));
        let v0 = store.wait(0, Duration::from_millis(10)); // returns at once: baseline
        let s = store.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            s.insert("r", ChatMsg { id: "m".into(), seq: 0, ts: 1, from: "a".into(), from_ip: String::new(), text: "oi".into(), file: None, mine: false });
        });
        let start = std::time::Instant::now();
        let v1 = store.wait(v0, Duration::from_secs(5));
        let took = start.elapsed();
        t.join().unwrap();
        assert!(v1 > v0);
        assert!(took < Duration::from_secs(1), "woke after {took:?}");
        // Nothing new: times out with the same version.
        assert_eq!(store.wait(v1, Duration::from_millis(50)), v1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
