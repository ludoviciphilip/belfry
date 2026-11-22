//! Shared pieces of Belfry: the room code format and the JSON types spoken
//! between the GUI and the background service (`belfryd`).

pub mod code;

use serde::{Deserialize, Serialize};

/// Address the service listens on (loopback only).
pub const DAEMON_ADDR: &str = "127.0.0.1:47830";
/// First port tried when hosting a room (TCP + UDP).
pub const DEFAULT_HOST_PORT: u16 = 11010;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the service keeps its state and the API token.
pub fn default_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("BELFRY_DATA_DIR") {
        return dir.into();
    }
    #[cfg(windows)]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
        std::path::PathBuf::from(base).join("Belfry")
    }
    #[cfg(not(windows))]
    {
        "/var/lib/belfry".into()
    }
}

pub fn token_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("BELFRY_TOKEN_FILE") {
        return p.into();
    }
    default_data_dir().join("token")
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Guest,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoomState {
    Off,
    Starting,
    Online,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    /// This machine.
    Me,
    /// Direct peer-to-peer connection.
    Direct,
    /// Traffic goes through another member or a relay.
    Relay,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    pub name: String,
    pub ip: String,
    pub latency_ms: Option<f64>,
    pub link: Link,
    pub protocol: String,
    pub is_host: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub nickname: String,
    pub active: bool,
    pub state: RoomState,
    pub error: Option<String>,
    /// This machine's address inside the room (e.g. 10.42.7.3).
    pub my_ip: Option<String>,
    /// The host always gets .1.
    pub host_ip: String,
    pub members: Vec<Member>,
    /// Only for hosts: the code friends paste to join.
    pub code: Option<String>,
    /// Only for hosts: what was put into the code.
    pub public_addrs: Vec<String>,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Status {
    pub version: String,
    pub engine_version: String,
    pub rooms: Vec<RoomView>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateRequest {
    pub name: String,
    pub nickname: String,
    /// Optional extra node everyone also connects to (your own VPS).
    #[serde(default)]
    pub relay: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JoinRequest {
    pub code: String,
    pub nickname: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RenameRequest {
    pub nickname: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
}

/// Peer URLs EasyTier accepts from a room code or the settings.
pub fn valid_peer_url(url: &str) -> bool {
    let ok_scheme = ["tcp://", "udp://", "ws://", "wss://", "quic://", "wg://"]
        .iter()
        .any(|s| url.starts_with(s));
    ok_scheme
        && url.len() <= 200
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ":/.[]-_".contains(c))
}

/// Nicknames become EasyTier hostnames; keep them short and printable.
pub fn clean_nickname(nick: &str) -> String {
    let s: String = nick
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(24)
        .collect();
    if s.is_empty() {
        "player".into()
    } else {
        s
    }
}
