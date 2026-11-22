//! Shared pieces of Belfry: the room code format and the JSON types spoken
//! between the GUI and the background service (`belfryd`).

pub mod code;

use serde::{Deserialize, Serialize};

/// Address the service listens on (loopback only).
pub const DAEMON_ADDR: &str = "127.0.0.1:47830";
/// Port each service listens on for other members (chat), on all interfaces;
/// requests must carry the room's peer key and come from the room subnet.
pub const PEER_PORT: u16 = 47831;
/// Port each app listens on to serve files its user shared.
pub const FILE_PORT: u16 = 47832;
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
    /// Sequence number of the newest chat message (for unread badges).
    #[serde(default)]
    pub chat_seq: u64,
    /// Proves membership to other members' chat and file servers.
    #[serde(default)]
    pub peer_key: String,
    #[serde(default)]
    pub cidr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileOffer {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMsg {
    /// Random, the same on every member (dedupe key).
    pub id: String,
    /// Local order on this machine.
    #[serde(default)]
    pub seq: u64,
    /// Sender's clock, ms since epoch.
    pub ts: u64,
    pub from: String,
    /// Sender's address in the room; files are fetched from here.
    #[serde(default)]
    pub from_ip: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub file: Option<FileOffer>,
    #[serde(default)]
    pub mine: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatPost {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub file: Option<FileOffer>,
}

pub const MAX_CHAT_TEXT: usize = 2000;

/// Shared by every member of a room: derived from the room secret.
pub fn peer_key(network_secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(format!("belfry-peer-v1:{network_secret}").as_bytes());
    code::hex(&d[..16])
}

/// Is `ip` inside 10.a.b.0/24 given as "10.a.b.0/24"?
pub fn in_cidr(ip: &std::net::IpAddr, cidr: &str) -> bool {
    let Some(net) = cidr.split_once('/').and_then(|(n, _)| n.parse::<std::net::Ipv4Addr>().ok()) else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(v4) => v4.octets()[..3] == net.octets()[..3],
        _ => false,
    }
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

/// File names come from other people's machines: no paths, no control chars.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c))
        .take(120)
        .collect();
    let clean = clean.trim().trim_matches('.').to_string();
    if clean.is_empty() {
        "file".into()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cidr_and_key() {
        let ip = "10.45.54.7".parse().unwrap();
        assert!(super::in_cidr(&ip, "10.45.54.0/24"));
        assert!(!super::in_cidr(&ip, "10.45.5.0/24"));
        assert!(!super::in_cidr(&"10.45.540.1".parse().unwrap_or(ip), "10.45.5.0/24"));
        assert_eq!(super::peer_key("abc").len(), 32);
        assert_ne!(super::peer_key("abc"), super::peer_key("abd"));
        assert_eq!(super::sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(super::sanitize_file_name("C:\\x\\a<b>.zip"), "ab.zip");
        assert_eq!(super::sanitize_file_name(".."), "file");
    }
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
