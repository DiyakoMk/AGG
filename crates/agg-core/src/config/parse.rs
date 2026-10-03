use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use indexmap::IndexMap;
use ipnet::IpNet;

use super::types::{HeaderSpec, Interface, Peer, SecretKey, WgConfig};
use crate::error::AggError;

pub fn parse_conf(input: &str) -> Result<WgConfig, AggError> {
    let mut interface: Option<InterfaceBuilder> = None;
    let mut peers: Vec<PeerBuilder> = Vec::new();
    let mut current = Section::None;

    for (lineno, raw) in input.lines().enumerate() {
        let line_no = lineno + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("[Interface]") {
            if interface.is_some() {
                return Err(AggError::config(format!(
                    "line {line_no}: duplicate [Interface] section"
                )));
            }
            interface = Some(InterfaceBuilder::default());
            current = Section::Interface;
            continue;
        }
        if line.eq_ignore_ascii_case("[Peer]") {
            peers.push(PeerBuilder::default());
            current = Section::Peer;
            continue;
        }
        let (key, value) = split_kv(line, line_no)?;
        match current {
            Section::None => {
                return Err(AggError::config(format!(
                    "line {line_no}: key `{key}` before a section header"
                )));
            }
            Section::Interface => {
                let iface = interface.as_mut().expect("section set");
                iface.apply(key, value, line_no)?;
            }
            Section::Peer => {
                let peer = peers.last_mut().expect("peer pushed");
                peer.apply(key, value, line_no)?;
            }
        }
    }

    let interface = interface
        .ok_or_else(|| AggError::config("missing [Interface] section"))?
        .build()?;
    let peers = peers
        .into_iter()
        .map(PeerBuilder::build)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WgConfig { interface, peers })
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn split_kv(line: &str, line_no: usize) -> Result<(&str, &str), AggError> {
    let (k, v) = line.split_once('=').ok_or_else(|| {
        AggError::config(format!("line {line_no}: expected `Key = value`, got `{line}`"))
    })?;
    Ok((k.trim(), v.trim()))
}

enum Section {
    None,
    Interface,
    Peer,
}

#[derive(Default)]
struct InterfaceBuilder {
    private_key: Option<SecretKey>,
    addresses: Vec<IpNet>,
    dns: Vec<IpAddr>,
    mtu: Option<u16>,
    listen_port: Option<u16>,
    jc: Option<u16>,
    jmin: Option<u16>,
    jmax: Option<u16>,
    s1: Option<u16>,
    s2: Option<u16>,
    s3: Option<u16>,
    s4: Option<u16>,
    h1: Option<HeaderSpec>,
    h2: Option<HeaderSpec>,
    h3: Option<HeaderSpec>,
    h4: Option<HeaderSpec>,
    extra: IndexMap<String, String>,
}

impl InterfaceBuilder {
    fn apply(&mut self, key: &str, value: &str, line_no: usize) -> Result<(), AggError> {
        match key.to_ascii_lowercase().as_str() {
            "privatekey" => self.private_key = Some(parse_secret(value, line_no)?),
            "address" => self.addresses.extend(parse_list(value, parse_ipnet, line_no)?),
            "dns" => self.dns.extend(parse_list(value, parse_ip, line_no)?),
            "mtu" => self.mtu = Some(parse_u16(value, "MTU", line_no)?),
            "listenport" => self.listen_port = Some(parse_u16(value, "ListenPort", line_no)?),
            "jc" => self.jc = Some(parse_u16(value, "Jc", line_no)?),
            "jmin" => self.jmin = Some(parse_u16(value, "Jmin", line_no)?),
            "jmax" => self.jmax = Some(parse_u16(value, "Jmax", line_no)?),
            "s1" => self.s1 = Some(parse_u16(value, "S1", line_no)?),
            "s2" => self.s2 = Some(parse_u16(value, "S2", line_no)?),
            "s3" => self.s3 = Some(parse_u16(value, "S3", line_no)?),
            "s4" => self.s4 = Some(parse_u16(value, "S4", line_no)?),
            "h1" => self.h1 = Some(parse_header(value, "H1", line_no)?),
            "h2" => self.h2 = Some(parse_header(value, "H2", line_no)?),
            "h3" => self.h3 = Some(parse_header(value, "H3", line_no)?),
            "h4" => self.h4 = Some(parse_header(value, "H4", line_no)?),
            _ => {
                self.extra.insert(key.to_string(), value.to_string());
            }
        }
        Ok(())
    }

    fn build(self) -> Result<Interface, AggError> {
        let private_key = self
            .private_key
            .ok_or_else(|| AggError::config("[Interface] missing PrivateKey"))?;
        Ok(Interface {
            private_key,
            addresses: self.addresses,
            dns: self.dns,
            mtu: self.mtu,
            listen_port: self.listen_port,
            jc: self.jc,
            jmin: self.jmin,
            jmax: self.jmax,
            s1: self.s1,
            s2: self.s2,
            s3: self.s3,
            s4: self.s4,
            h1: self.h1,
            h2: self.h2,
            h3: self.h3,
            h4: self.h4,
            extra: self.extra,
        })
    }
}

#[derive(Default)]
struct PeerBuilder {
    public_key: Option<[u8; 32]>,
    preshared_key: Option<SecretKey>,
    endpoint: Option<SocketAddr>,
    allowed_ips: Vec<IpNet>,
    persistent_keepalive: Option<u16>,
    extra: IndexMap<String, String>,
}

impl PeerBuilder {
    fn apply(&mut self, key: &str, value: &str, line_no: usize) -> Result<(), AggError> {
        match key.to_ascii_lowercase().as_str() {
            "publickey" => self.public_key = Some(parse_key_bytes(value, line_no)?),
            "presharedkey" => self.preshared_key = Some(parse_secret(value, line_no)?),
            "endpoint" => {
                self.endpoint = Some(parse_endpoint(value, line_no)?);
            }
            "allowedips" => self
                .allowed_ips
                .extend(parse_list(value, parse_ipnet, line_no)?),
            "persistentkeepalive" => {
                self.persistent_keepalive = Some(parse_u16(value, "PersistentKeepalive", line_no)?);
            }
            _ => {
                self.extra.insert(key.to_string(), value.to_string());
            }
        }
        Ok(())
    }

    fn build(self) -> Result<Peer, AggError> {
        let public_key = self
            .public_key
            .ok_or_else(|| AggError::config("[Peer] missing PublicKey"))?;
        Ok(Peer {
            public_key,
            preshared_key: self.preshared_key,
            endpoint: self.endpoint,
            allowed_ips: self.allowed_ips,
            persistent_keepalive: self.persistent_keepalive,
            extra: self.extra,
        })
    }
}

fn parse_list<T>(
    value: &str,
    parse: fn(&str, usize) -> Result<T, AggError>,
    line_no: usize,
) -> Result<Vec<T>, AggError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| parse(s, line_no))
        .collect()
}

fn parse_ipnet(s: &str, line_no: usize) -> Result<IpNet, AggError> {
    IpNet::from_str(s).or_else(|_| {
        // Bare IP → /32 or /128
        let ip = IpAddr::from_str(s).map_err(|_| {
            AggError::config(format!("line {line_no}: invalid address `{s}`"))
        })?;
        Ok(IpNet::from(ip))
    })
}

fn parse_ip(s: &str, line_no: usize) -> Result<IpAddr, AggError> {
    IpAddr::from_str(s)
        .map_err(|_| AggError::config(format!("line {line_no}: invalid IP `{s}`")))
}

fn parse_u16(s: &str, name: &str, line_no: usize) -> Result<u16, AggError> {
    s.parse::<u16>()
        .map_err(|_| AggError::config(format!("line {line_no}: {name} is not a u16: `{s}`")))
}

fn parse_header(s: &str, name: &str, line_no: usize) -> Result<HeaderSpec, AggError> {
    if let Some((a, b)) = s.split_once('-') {
        let start = parse_u32(a.trim(), name, line_no)?;
        let end = parse_u32(b.trim(), name, line_no)?;
        if start > end {
            return Err(AggError::config(format!(
                "line {line_no}: {name} range start ({start}) > end ({end})"
            )));
        }
        Ok(HeaderSpec::Range { start, end })
    } else {
        Ok(HeaderSpec::Single(parse_u32(s, name, line_no)?))
    }
}

fn parse_u32(s: &str, name: &str, line_no: usize) -> Result<u32, AggError> {
    s.parse::<u32>()
        .map_err(|_| AggError::config(format!("line {line_no}: {name} is not a u32: `{s}`")))
}

fn parse_secret(s: &str, line_no: usize) -> Result<SecretKey, AggError> {
    Ok(SecretKey::from_bytes(parse_key_bytes(s, line_no)?))
}

fn parse_key_bytes(s: &str, line_no: usize) -> Result<[u8; 32], AggError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let bytes = STANDARD.decode(s.trim()).map_err(|_| {
        AggError::config(format!("line {line_no}: key is not valid base64"))
    })?;
    bytes.try_into().map_err(|_| {
        AggError::config(format!("line {line_no}: key must decode to 32 bytes"))
    })
}

fn parse_endpoint(s: &str, line_no: usize) -> Result<SocketAddr, AggError> {
    SocketAddr::from_str(s).or_else(|_| {
        // host:port without brackets — last colon is the port.
        let (host, port) = s.rsplit_once(':').ok_or_else(|| {
            AggError::config(format!("line {line_no}: invalid Endpoint `{s}`"))
        })?;
        let port: u16 = port.parse().map_err(|_| {
            AggError::config(format!("line {line_no}: invalid Endpoint port in `{s}`"))
        })?;
        let ip = IpAddr::from_str(host).map_err(|_| {
            AggError::config(format!(
                "line {line_no}: Endpoint host `{host}` is not an IP (DNS names are Phase 4)"
            ))
        })?;
        Ok(SocketAddr::new(ip, port))
    })
}
