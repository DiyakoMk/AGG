//! Full-tunnel Ethernet intercept. Platform-agnostic.
//!
//! Outbound IPv4 (except ARP, broadcast, and the tunnel UDP itself) is SNAT'd
//! onto the tunnel address and encapsulated. Inbound tunnel UDP is
//! decapsulated and DNAT'd back onto the LAN address. No virtual NIC.

use std::net::{Ipv4Addr, SocketAddrV4};

use boringtun::noise::TunnResult;

use crate::config::WgConfig;
use crate::error::AggError;
use crate::packet::{
    self, build_udp4_frame, dnat_ipv4_dest, ethertype, ipv4_info, ipv4_payload, is_peer_udp,
    learn_lan_from_outbound_ipv4, snat_ipv4_source, wrap_ipv4_for_stack, LearnedLan, ETH_TYPE_ARP,
    ETH_TYPE_IPV4, ETH_TYPE_IPV6,
};
use crate::tunnel::TunnelEngine;

const BUF: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Host stack → wire (MSTCP_FLAG_SENT / PACKET_FLAG_ON_SEND)
    Send,
    /// Wire → host stack (PACKET_FLAG_ON_RECEIVE)
    Recv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Re-inject the original frame on the same path.
    Passthrough,
    /// Re-inject the original AND extra frames (handshake junk while fail-open).
    Inject {
        to_wire: Vec<Vec<u8>>,
        to_stack: Vec<Vec<u8>>,
    },
    /// Drop the original. Inject replacements. Never use with empty extras.
    Replace {
        to_wire: Vec<Vec<u8>>,
        to_stack: Vec<Vec<u8>>,
    },
}

pub struct Intercept {
    engine: TunnelEngine,
    lan: Option<LearnedLan>,
    tunnel_ip: Ipv4Addr,
    peer: SocketAddrV4,
    udp_src_port: u16,
    handshake_kicked: bool,
    scratch: [u8; BUF],
}

impl Intercept {
    pub fn from_config(cfg: &WgConfig, lan: Option<LearnedLan>) -> Result<Self, AggError> {
        let engine = TunnelEngine::from_config(cfg)?;
        let tunnel_ip = cfg
            .interface
            .addresses
            .iter()
            .find_map(|n| match n.addr() {
                std::net::IpAddr::V4(v) => Some(v),
                _ => None,
            })
            .ok_or_else(|| AggError::config("[Interface] Address must include an IPv4"))?;
        let endpoint = cfg
            .peer()?
            .endpoint
            .ok_or_else(|| AggError::config("peer has no Endpoint"))?;
        let peer = match endpoint {
            std::net::SocketAddr::V4(v) => v,
            std::net::SocketAddr::V6(_) => {
                return Err(AggError::config("Phase 1 intercept is IPv4-only"));
            }
        };
        let udp_src_port = cfg.interface.listen_port.unwrap_or(51820);
        Ok(Self {
            engine,
            lan,
            tunnel_ip,
            peer,
            udp_src_port,
            handshake_kicked: false,
            scratch: [0u8; BUF],
        })
    }

    pub fn seed_lan(&mut self, lan: LearnedLan) {
        self.lan = Some(lan);
    }

    pub fn lan(&self) -> Option<LearnedLan> {
        self.lan
    }

    pub fn stats(&self) -> crate::stats::TunnelStats {
        self.engine.stats()
    }

    pub fn handshake_ok(&self) -> bool {
        self.engine.stats().handshake_ok()
    }

    /// Drive timers / leftover handshake datagrams. Call ~4 Hz.
    pub fn poll_timers(&mut self) -> Verdict {
        let pkt = match self.engine.update_timers(&mut self.scratch) {
            TunnResult::WriteToNetwork(p) => Some(p.to_vec()),
            _ => None,
        };
        match pkt {
            Some(pkt) => self.frames_for_wg_datagrams(&[pkt]),
            None => Verdict::Passthrough,
        }
    }

    pub fn on_frame(&mut self, dir: Direction, frame: &[u8]) -> Verdict {
        match dir {
            Direction::Send => self.on_send(frame),
            Direction::Recv => self.on_recv(frame),
        }
    }

    fn on_send(&mut self, frame: &[u8]) -> Verdict {
        if let Some(lan) = learn_lan_from_outbound_ipv4(frame) {
            if self.lan.is_none() {
                self.lan = Some(lan);
            }
        }
        match ethertype(frame) {
            Some(ETH_TYPE_ARP) | Some(ETH_TYPE_IPV6) => Verdict::Passthrough,
            Some(ETH_TYPE_IPV4) => self.on_send_ipv4(frame),
            _ => Verdict::Passthrough,
        }
    }

    fn on_send_ipv4(&mut self, frame: &[u8]) -> Verdict {
        let peer_ip = *self.peer.ip();
        let peer_port = self.peer.port();
        if is_peer_udp(frame, peer_ip, peer_port) {
            return Verdict::Passthrough;
        }
        if let Some(ip) = ipv4_info(frame) {
            if is_broadcast_or_multicast(ip.dst) || ip.dst.is_loopback() {
                return Verdict::Passthrough;
            }
            if let Some(lan) = self.lan {
                if ip.dst == lan.our_ip {
                    return Verdict::Passthrough;
                }
            }
        }

        // Fail-open until the tunnel session exists. Dropping IPv4 here
        // blackholes DHCP/DNS/Windows and hard-locks the machine.
        if !self.handshake_ok() {
            let mut to_wire = Vec::new();
            if !self.handshake_kicked && self.lan.is_some() {
                self.drain_handshake(&mut to_wire);
                self.handshake_kicked = true;
            }
            if to_wire.is_empty() {
                return Verdict::Passthrough;
            }
            return Verdict::Inject {
                to_wire,
                to_stack: Vec::new(),
            };
        }

        let mut owned = frame.to_vec();
        if snat_ipv4_source(&mut owned, self.tunnel_ip).is_none() {
            return Verdict::Passthrough;
        }
        let inner = match ipv4_payload(&owned) {
            Some(p) => p.to_vec(),
            None => return Verdict::Passthrough,
        };

        let mut to_wire = Vec::new();
        let encap = match self.engine.encapsulate(&inner, &mut self.scratch) {
            TunnResult::WriteToNetwork(pkt) => Some(pkt.to_vec()),
            _ => None,
        };
        if let Some(pkt) = encap {
            if let Some(f) = self.wrap_wg(&pkt) {
                to_wire.push(f);
            }
            self.drain_wg(&mut to_wire);
        }
        if to_wire.is_empty() {
            return Verdict::Passthrough;
        }
        Verdict::Replace {
            to_wire,
            to_stack: Vec::new(),
        }
    }

    fn on_recv(&mut self, frame: &[u8]) -> Verdict {
        let peer_ip = *self.peer.ip();
        let peer_port = self.peer.port();
        if !is_peer_udp(frame, peer_ip, peer_port) {
            return Verdict::Passthrough;
        }
        let Some(udp) = packet::parse_udp4(frame) else {
            return Verdict::Passthrough;
        };
        if frame.len() < udp.payload_off {
            return Verdict::Passthrough;
        }
        let datagram = frame[udp.payload_off..].to_vec();
        let mut to_wire = Vec::new();
        let mut to_stack = Vec::new();
        self.handle_decap(
            &datagram,
            Some(std::net::IpAddr::V4(peer_ip)),
            &mut to_wire,
            &mut to_stack,
        );
        if to_wire.is_empty() && to_stack.is_empty() {
            // Handshake response consumed by Tunn: drop it from the stack, inject nothing.
            return Verdict::Replace {
                to_wire: Vec::new(),
                to_stack: Vec::new(),
            };
        }
        Verdict::Replace { to_wire, to_stack }
    }

    fn handle_decap(
        &mut self,
        datagram: &[u8],
        src: Option<std::net::IpAddr>,
        to_wire: &mut Vec<Vec<u8>>,
        to_stack: &mut Vec<Vec<u8>>,
    ) {
        let mut cur: &[u8] = datagram;
        loop {
            enum Step {
                ToNet(Vec<u8>),
                ToTun(Vec<u8>),
                Stop,
            }
            let step = match self.engine.decapsulate(src, cur, &mut self.scratch) {
                TunnResult::WriteToNetwork(pkt) => Step::ToNet(pkt.to_vec()),
                TunnResult::WriteToTunnelV4(pkt, _) | TunnResult::WriteToTunnelV6(pkt, _) => {
                    Step::ToTun(pkt.to_vec())
                }
                TunnResult::Done | TunnResult::Err(_) => Step::Stop,
            };
            match step {
                Step::ToNet(pkt) => {
                    if let Some(f) = self.wrap_wg(&pkt) {
                        to_wire.push(f);
                    }
                    cur = &[];
                }
                Step::ToTun(pkt) => {
                    if let Some(f) = self.wrap_inner(&pkt) {
                        to_stack.push(f);
                    }
                    break;
                }
                Step::Stop => break,
            }
        }
    }

    fn drain_handshake(&mut self, to_wire: &mut Vec<Vec<u8>>) {
        let pkt = match self.engine.kick_handshake(&mut self.scratch) {
            TunnResult::WriteToNetwork(p) => Some(p.to_vec()),
            _ => None,
        };
        if let Some(pkt) = pkt {
            if let Some(f) = self.wrap_wg(&pkt) {
                to_wire.push(f);
            }
        }
        self.drain_wg(to_wire);
    }

    fn drain_wg(&mut self, to_wire: &mut Vec<Vec<u8>>) {
        loop {
            let pkt = match self.engine.decapsulate(None, &[], &mut self.scratch) {
                TunnResult::WriteToNetwork(p) => p.to_vec(),
                _ => break,
            };
            if let Some(f) = self.wrap_wg(&pkt) {
                to_wire.push(f);
            }
        }
    }

    fn wrap_wg(&self, datagram: &[u8]) -> Option<Vec<u8>> {
        let lan = self.lan?;
        let mut buf = vec![0u8; BUF];
        let n = build_udp4_frame(
            &lan,
            self.udp_src_port,
            *self.peer.ip(),
            self.peer.port(),
            datagram,
            &mut buf,
        )?;
        buf.truncate(n);
        Some(buf)
    }

    fn wrap_inner(&self, ipv4: &[u8]) -> Option<Vec<u8>> {
        let lan = self.lan?;
        let mut buf = vec![0u8; BUF];
        let n = wrap_ipv4_for_stack(&lan, ipv4, &mut buf)?;
        buf.truncate(n);
        dnat_ipv4_dest(&mut buf, lan.our_ip)?;
        Some(buf)
    }

    fn frames_for_wg_datagrams(&self, datagrams: &[Vec<u8>]) -> Verdict {
        let mut to_wire = Vec::new();
        for d in datagrams {
            if let Some(f) = self.wrap_wg(d) {
                to_wire.push(f);
            }
        }
        if to_wire.is_empty() {
            Verdict::Passthrough
        } else {
            Verdict::Replace {
                to_wire,
                to_stack: Vec::new(),
            }
        }
    }
}

fn is_broadcast_or_multicast(ip: Ipv4Addr) -> bool {
    ip.is_broadcast() || ip.is_multicast() || ip.octets()[3] == 255
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WgConfig;
    use crate::packet::LearnedLan;

    fn cfg() -> WgConfig {
        WgConfig::from_str(include_str!("../tests/fixtures/awg2.conf")).unwrap()
    }

    fn lan() -> LearnedLan {
        LearnedLan {
            our_mac: [2, 0, 0, 0, 0, 1],
            gw_mac: [2, 0, 0, 0, 0, 2],
            our_ip: Ipv4Addr::new(192, 168, 1, 10),
        }
    }

    #[test]
    fn arp_passthrough() {
        let mut ix = Intercept::from_config(&cfg(), Some(lan())).unwrap();
        let mut arp = vec![0u8; 42];
        arp[12..14].copy_from_slice(&ETH_TYPE_ARP.to_be_bytes());
        assert_eq!(ix.on_frame(Direction::Send, &arp), Verdict::Passthrough);
    }

    #[test]
    fn ipv6_passthrough() {
        let mut ix = Intercept::from_config(&cfg(), Some(lan())).unwrap();
        let mut f = vec![0u8; 54];
        f[12..14].copy_from_slice(&ETH_TYPE_IPV6.to_be_bytes());
        assert_eq!(ix.on_frame(Direction::Send, &f), Verdict::Passthrough);
    }

    #[test]
    fn outbound_ipv4_fail_open_until_handshake() {
        let mut ix = Intercept::from_config(&cfg(), Some(lan())).unwrap();
        let lan = lan();
        let mut buf = [0u8; 128];
        let n = crate::packet::build_udp4_frame(
            &lan,
            12345,
            Ipv4Addr::new(8, 8, 8, 8),
            53,
            b"dns?",
            &mut buf,
        )
        .unwrap();
        match ix.on_frame(Direction::Send, &buf[..n]) {
            Verdict::Inject { to_wire, to_stack } => {
                assert!(to_stack.is_empty());
                assert!(!to_wire.is_empty(), "handshake/junk should go to the wire");
                let u = crate::packet::parse_udp4(&to_wire[0]).unwrap();
                assert_eq!(u.dst, Ipv4Addr::new(203, 0, 113, 10));
                assert_eq!(u.dst_port, 39743);
            }
            Verdict::Passthrough => {}
            other => panic!("{other:?}"),
        }
    }
}
