//! Finding the addresses friends can use to reach this PC: public IPv4 via a
//! STUN binding request, and the global IPv6 address the OS would use.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

const STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun.cloudflare.com:3478",
    "stun.nextcloud.com:3478",
];

pub fn public_v4() -> Option<Ipv4Addr> {
    if let Some(v) = override_var("BELFRY_ADVERTISE_V4") {
        return v.and_then(|s| s.parse().ok());
    }
    for server in STUN_SERVERS {
        let Ok(addrs) = server.to_socket_addrs() else { continue };
        for addr in addrs.filter(SocketAddr::is_ipv4) {
            if let Some(IpAddr::V4(ip)) = stun_query(addr) {
                return Some(ip);
            }
        }
    }
    None
}

/// The IPv6 address the OS would use to reach the internet. IPv6 has no NAT,
/// so if the router lets the port in, this is reachable even behind CGNAT.
pub fn public_v6() -> Option<Ipv6Addr> {
    if let Some(v) = override_var("BELFRY_ADVERTISE_V6") {
        return v.and_then(|s| s.parse().ok());
    }
    let sock = UdpSocket::bind("[::]:0").ok()?;
    sock.connect("[2001:4860:4860::8888]:53").ok()?;
    match sock.local_addr().ok()?.ip() {
        // 2000::/3 is global unicast.
        IpAddr::V6(ip) if (ip.segments()[0] & 0xe000) == 0x2000 => Some(ip),
        _ => None,
    }
}

/// `Some(None)` = explicitly disabled ("none"), `Some(Some(x))` = forced value.
fn override_var(name: &str) -> Option<Option<String>> {
    let v = std::env::var(name).ok()?;
    Some(if v.is_empty() || v == "none" { None } else { Some(v) })
}

fn stun_query(server: SocketAddr) -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    let txid: [u8; 12] = belfry_proto::code::random_bytes();
    let mut req = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    req.extend_from_slice(&txid);
    for _ in 0..2 {
        sock.send_to(&req, server).ok()?;
        let mut buf = [0u8; 512];
        let Ok((n, _)) = sock.recv_from(&mut buf) else { continue };
        if let Some(ip) = parse_binding_response(&buf[..n], &txid) {
            return Some(ip);
        }
    }
    None
}

fn parse_binding_response(b: &[u8], txid: &[u8; 12]) -> Option<IpAddr> {
    if b.len() < 20 || b[0..2] != [0x01, 0x01] || &b[8..20] != txid {
        return None;
    }
    let mut i = 20;
    let mut mapped = None;
    while i + 4 <= b.len() {
        let kind = u16::from_be_bytes([b[i], b[i + 1]]);
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let v = b.get(i + 4..i + 4 + len)?;
        if (kind == 0x0020 || kind == 0x0001) && len >= 8 && v[1] == 0x01 {
            let mut o = [v[4], v[5], v[6], v[7]];
            if kind == 0x0020 {
                for (k, m) in o.iter_mut().zip([0x21u8, 0x12, 0xA4, 0x42]) {
                    *k ^= m;
                }
                return Some(IpAddr::V4(o.into()));
            }
            mapped = Some(IpAddr::V4(o.into()));
        }
        i += 4 + len.div_ceil(4) * 4;
    }
    mapped
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_xor_mapped() {
        let txid = [7u8; 12];
        let mut b = vec![0x01, 0x01, 0, 12, 0x21, 0x12, 0xA4, 0x42];
        b.extend_from_slice(&txid);
        // XOR-MAPPED-ADDRESS for 201.89.94.91
        let ip = [201u8 ^ 0x21, 89 ^ 0x12, 94 ^ 0xA4, 91 ^ 0x42];
        b.extend_from_slice(&[0x00, 0x20, 0, 8, 0, 1, 0, 0]);
        b.extend_from_slice(&ip);
        assert_eq!(
            super::parse_binding_response(&b, &txid),
            Some("201.89.94.91".parse().unwrap())
        );
    }
}
