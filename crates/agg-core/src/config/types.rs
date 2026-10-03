use std::fmt;
use std::net::{IpAddr, SocketAddr};

use indexmap::IndexMap;
use ipnet::IpNet;

/// WireGuard message type identifiers. Obfuscated H-values must not collide with these.
pub const WG_TYPE_INIT: u32 = 1;
pub const WG_TYPE_RESPONSE: u32 = 2;
pub const WG_TYPE_COOKIE: u32 = 3;
pub const WG_TYPE_DATA: u32 = 4;

/// 32-byte key that never prints its bytes.
#[derive(Clone)]
pub struct SecretKey(pub(crate) [u8; 32]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey([REDACTED])")
    }
}

impl PartialEq for SecretKey {
    fn eq(&self, other: &Self) -> bool {
        // Constant-time enough for tests; production compare happens inside BoringTun.
        self.0 == other.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderSpec {
    Single(u32),
    Range { start: u32, end: u32 },
}

impl HeaderSpec {
    pub fn start_end(self) -> (u32, u32) {
        match self {
            HeaderSpec::Single(v) => (v, v),
            HeaderSpec::Range { start, end } => (start, end),
        }
    }

    pub fn contains(self, v: u32) -> bool {
        let (s, e) = self.start_end();
        v >= s && v <= e
    }

    pub fn overlaps(self, other: HeaderSpec) -> bool {
        let (a0, a1) = self.start_end();
        let (b0, b1) = other.start_end();
        a0 <= b1 && b0 <= a1
    }
}

impl fmt::Display for HeaderSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HeaderSpec::Single(v) => write!(f, "{v}"),
            HeaderSpec::Range { start, end } => write!(f, "{start}-{end}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    pub private_key: SecretKey,
    pub addresses: Vec<IpNet>,
    pub dns: Vec<IpAddr>,
    pub mtu: Option<u16>,
    pub listen_port: Option<u16>,
    pub jc: Option<u16>,
    pub jmin: Option<u16>,
    pub jmax: Option<u16>,
    pub s1: Option<u16>,
    pub s2: Option<u16>,
    pub s3: Option<u16>,
    pub s4: Option<u16>,
    pub h1: Option<HeaderSpec>,
    pub h2: Option<HeaderSpec>,
    pub h3: Option<HeaderSpec>,
    pub h4: Option<HeaderSpec>,
    /// Keys we do not interpret (including I1–I5). Round-tripped, never dropped.
    pub extra: IndexMap<String, String>,
}

impl Interface {
    pub fn obfuscation_present(&self) -> bool {
        self.jc.is_some()
            || self.jmin.is_some()
            || self.jmax.is_some()
            || self.s1.is_some()
            || self.s2.is_some()
            || self.s3.is_some()
            || self.s4.is_some()
            || self.h1.is_some()
            || self.h2.is_some()
            || self.h3.is_some()
            || self.h4.is_some()
    }

    pub fn headers(&self) -> Option<[HeaderSpec; 4]> {
        match (self.h1, self.h2, self.h3, self.h4) {
            (Some(h1), Some(h2), Some(h3), Some(h4)) => Some([h1, h2, h3, h4]),
            (None, None, None, None) => None,
            _ => None, // incomplete — validate() rejects this
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Peer {
    pub public_key: [u8; 32],
    pub preshared_key: Option<SecretKey>,
    pub endpoint: Option<SocketAddr>,
    pub allowed_ips: Vec<IpNet>,
    pub persistent_keepalive: Option<u16>,
    pub extra: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WgConfig {
    pub interface: Interface,
    pub peers: Vec<Peer>,
}

impl WgConfig {
    /// First peer — Phase 0 is single-peer.
    pub fn peer(&self) -> Result<&Peer, crate::error::AggError> {
        self.peers
            .first()
            .ok_or_else(|| crate::error::AggError::config("config has no [Peer] section"))
    }
}
