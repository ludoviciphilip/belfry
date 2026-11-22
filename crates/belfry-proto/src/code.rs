//! Room codes: everything a friend needs to join, packed into one string.
//!
//! Layout (version 1), then Crockford base32 as one word prefixed `BLF`:
//! `ver | id[6] | secret[12] | subnet[2] | port[2] | flags | v4[4]? | v6[16]? |
//!  name_len | name | relay_len? | relay? | crc16[2]`

use std::net::{Ipv4Addr, Ipv6Addr};

const VERSION: u8 = 1;
const PREFIX: &str = "BLF";
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const F_V4: u8 = 1;
const F_V6: u8 = 2;
const F_RELAY: u8 = 4;
pub const MAX_NAME: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomCode {
    pub id: [u8; 6],
    pub secret: [u8; 12],
    /// The room uses 10.<a>.<b>.0/24.
    pub subnet: [u8; 2],
    pub port: u16,
    pub v4: Option<Ipv4Addr>,
    pub v6: Option<Ipv6Addr>,
    pub name: String,
    pub relay: Option<String>,
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("no OS random source");
    b
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

pub fn base32(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.chars() {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        let v = ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .ok_or_else(|| format!("invalid character '{c}'"))? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// CRC-16/CCITT-FALSE: catches typos and truncated pastes.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn truncate_utf8(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

impl RoomCode {
    /// A fresh room with random identity, secret and subnet.
    pub fn generate(name: &str, port: u16) -> Self {
        let sub = random_bytes::<2>();
        RoomCode {
            id: random_bytes(),
            secret: random_bytes(),
            // Avoid 10.0.x and 10.255.x, which home routers and VPNs like to use.
            subnet: [1 + sub[0] % 254, sub[1]],
            port,
            v4: None,
            v6: None,
            name: truncate_utf8(name.trim(), MAX_NAME).to_string(),
            relay: None,
        }
    }

    pub fn network_name(&self) -> String {
        format!("belfry-{}", hex(&self.id))
    }

    pub fn network_secret(&self) -> String {
        base32(&self.secret)
    }

    pub fn host_ip(&self) -> Ipv4Addr {
        Ipv4Addr::new(10, self.subnet[0], self.subnet[1], 1)
    }

    pub fn cidr(&self) -> String {
        format!("10.{}.{}.0/24", self.subnet[0], self.subnet[1])
    }

    /// Where a guest should try to reach the host (and the relay, if any).
    pub fn peer_urls(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(ip) = self.v6 {
            v.push(format!("udp://[{ip}]:{}", self.port));
            v.push(format!("tcp://[{ip}]:{}", self.port));
        }
        if let Some(ip) = self.v4 {
            v.push(format!("udp://{ip}:{}", self.port));
            v.push(format!("tcp://{ip}:{}", self.port));
        }
        if let Some(r) = &self.relay {
            v.push(r.clone());
        }
        v
    }

    pub fn encode(&self) -> String {
        let mut b = vec![VERSION];
        b.extend_from_slice(&self.id);
        b.extend_from_slice(&self.secret);
        b.extend_from_slice(&self.subnet);
        b.extend_from_slice(&self.port.to_be_bytes());
        let mut flags = 0;
        if self.v4.is_some() {
            flags |= F_V4;
        }
        if self.v6.is_some() {
            flags |= F_V6;
        }
        if self.relay.is_some() {
            flags |= F_RELAY;
        }
        b.push(flags);
        if let Some(ip) = self.v4 {
            b.extend_from_slice(&ip.octets());
        }
        if let Some(ip) = self.v6 {
            b.extend_from_slice(&ip.octets());
        }
        let name = truncate_utf8(&self.name, MAX_NAME);
        b.push(name.len() as u8);
        b.extend_from_slice(name.as_bytes());
        if let Some(r) = &self.relay {
            let r = truncate_utf8(r, 200);
            b.push(r.len() as u8);
            b.extend_from_slice(r.as_bytes());
        }
        let crc = crc16(&b);
        b.extend_from_slice(&crc.to_be_bytes());
        // One unbroken word: a double-click selects all of it in any chat app.
        format!("{PREFIX}{}", base32(&b))
    }

    pub fn decode(code: &str) -> Result<Self, String> {
        let compact: String = code
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        let upper = compact.to_ascii_uppercase();
        let body = upper
            .strip_prefix(PREFIX)
            .ok_or("this is not a Belfry code (it should start with BLF)")?;
        let b = unbase32(body)?;
        if b.len() < 28 {
            return Err("the code is incomplete".into());
        }
        let (data, crc) = b.split_at(b.len() - 2);
        // base32 padding can leave one extra zero byte; accept both lengths.
        let (data, crc) = if crc16(data) == u16::from_be_bytes([crc[0], crc[1]]) {
            (data, true)
        } else if b.len() >= 29 && b[b.len() - 1] == 0 {
            let b2 = &b[..b.len() - 1];
            let (d2, c2) = b2.split_at(b2.len() - 2);
            if crc16(d2) == u16::from_be_bytes([c2[0], c2[1]]) {
                (d2, true)
            } else {
                (data, false)
            }
        } else {
            (data, false)
        };
        if !crc {
            return Err("the code has a typo or was cut off (checksum failed)".into());
        }
        let mut r = Reader { b: data, i: 0 };
        let ver = r.u8()?;
        if ver != VERSION {
            return Err(format!("code version {ver} needs a newer Belfry"));
        }
        let id: [u8; 6] = r.take(6)?.try_into().unwrap();
        let secret: [u8; 12] = r.take(12)?.try_into().unwrap();
        let subnet: [u8; 2] = r.take(2)?.try_into().unwrap();
        let port = u16::from_be_bytes(r.take(2)?.try_into().unwrap());
        let flags = r.u8()?;
        let v4 = if flags & F_V4 != 0 {
            let o: [u8; 4] = r.take(4)?.try_into().unwrap();
            Some(Ipv4Addr::from(o))
        } else {
            None
        };
        let v6 = if flags & F_V6 != 0 {
            let o: [u8; 16] = r.take(16)?.try_into().unwrap();
            Some(Ipv6Addr::from(o))
        } else {
            None
        };
        let n = r.u8()? as usize;
        let name = String::from_utf8(r.take(n)?.to_vec()).map_err(|_| "bad room name")?;
        let relay = if flags & F_RELAY != 0 {
            let n = r.u8()? as usize;
            let s = String::from_utf8(r.take(n)?.to_vec()).map_err(|_| "bad relay")?;
            if !crate::valid_peer_url(&s) {
                return Err("the code carries an invalid relay address".into());
            }
            Some(s)
        } else {
            None
        };
        Ok(RoomCode { id, secret, subnet, port, v4, v6, name, relay })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or("the code is incomplete")?;
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RoomCode {
        let mut c = RoomCode::generate("Bloodborne coop ✦ Yharnam", 11010);
        c.v4 = Some("201.89.94.91".parse().unwrap());
        c.v6 = Some("2804:d55:860e:700:ed13:e8c0:ff5a:39e8".parse().unwrap());
        c
    }

    #[test]
    fn round_trip_variants() {
        for i in 0..200 {
            let mut c = sample();
            if i % 2 == 0 {
                c.v6 = None;
            }
            if i % 3 == 0 {
                c.v4 = None;
            }
            if i % 5 == 0 {
                c.relay = Some("tcp://relay.example.com:11010".into());
            }
            c.name = "x".repeat(i % 45);
            let enc = c.encode();
            let mut expect = c.clone();
            expect.name = truncate_utf8(&c.name, MAX_NAME).into();
            assert_eq!(RoomCode::decode(&enc).unwrap(), expect, "{enc}");
            // Case, spaces and look-alike letters don't matter.
            let messy = format!(" {} ", enc.to_lowercase().replace('0', "o"));
            assert_eq!(RoomCode::decode(&messy).unwrap(), expect);
        }
    }

    #[test]
    fn typo_is_caught() {
        let enc = sample().encode();
        let mut chars: Vec<char> = enc.chars().collect();
        let k = chars.len() / 2;
        chars[k] = if chars[k] == 'A' { 'B' } else { 'A' };
        let bad: String = chars.into_iter().collect();
        assert!(RoomCode::decode(&bad).is_err());
        assert!(RoomCode::decode(&enc[..enc.len() - 5]).is_err());
        // Pasted with stray dashes/spaces in the middle still works.
        let split = format!("{}-{} {}", &enc[..10], &enc[10..30], &enc[30..]);
        assert!(RoomCode::decode(&split).is_ok());
        assert!(RoomCode::decode("hello").is_err());
    }

    #[test]
    fn peers_and_names() {
        let c = sample();
        assert!(c.network_name().starts_with("belfry-"));
        assert_eq!(c.host_ip().octets()[3], 1);
        let urls = c.peer_urls();
        assert_eq!(urls.len(), 4);
        assert!(urls.iter().all(|u| crate::valid_peer_url(u)), "{urls:?}");
        println!("{}  ({} chars)", c.encode(), c.encode().len());
    }
}
