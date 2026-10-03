use boringtun::noise::amnezia::AmneziaConfig;
use boringtun::noise::handshake::ObfuscationRanges;
use boringtun::noise::{Tunn, TunnResult};
use boringtun::x25519::{PublicKey, StaticSecret};

use crate::config::WgConfig;
use crate::error::AggError;
use crate::stats::TunnelStats;

/// Owns a BoringTun `Tunn` built from an AmneziaWG 2.0 config.
///
/// I/O (UDP, TUN, NDIS) stays in the platform crate / CLI. This type only
/// drives encapsulate / decapsulate / timers.
pub struct TunnelEngine {
    tun: Tunn,
}

impl TunnelEngine {
    pub fn from_config(cfg: &WgConfig) -> Result<Self, AggError> {
        let peer = cfg.peer()?;
        let static_private = StaticSecret::from(*cfg.interface.private_key.as_bytes());
        let peer_public = PublicKey::from(peer.public_key);
        let psk = peer.preshared_key.as_ref().map(|k| *k.as_bytes());
        let keepalive = peer.persistent_keepalive;

        let (h1s, h1e, h2s, h2e, h3s, h3e, h4s, h4e) = header_scalars(&cfg.interface);
        let obf = ObfuscationRanges::new(h1s, h1e, h2s, h2e, h3s, h3e, h4s, h4e)
            .map_err(AggError::config)?;
        let amnezia = amnezia_from_interface(&cfg.interface);

        let tun = Tunn::new_with_obfuscation(
            static_private,
            peer_public,
            psk,
            keepalive,
            0,
            None,
            obf,
            amnezia,
        )
        .map_err(AggError::config)?;

        Ok(Self { tun })
    }

    pub fn encapsulate<'a>(&mut self, src: &[u8], dst: &'a mut [u8]) -> TunnResult<'a> {
        self.tun.encapsulate(src, dst)
    }

    pub fn decapsulate<'a>(
        &mut self,
        src_addr: Option<std::net::IpAddr>,
        datagram: &[u8],
        dst: &'a mut [u8],
    ) -> TunnResult<'a> {
        self.tun.decapsulate(src_addr, datagram, dst)
    }

    pub fn update_timers<'a>(&mut self, dst: &'a mut [u8]) -> TunnResult<'a> {
        self.tun.update_timers(dst)
    }

    /// Start (or restart) a handshake. Emits junk (Jc) then the initiation.
    pub fn kick_handshake<'a>(&mut self, dst: &'a mut [u8]) -> TunnResult<'a> {
        self.tun.format_handshake_initiation(dst, true)
    }

    pub fn stats(&self) -> TunnelStats {
        let (time, tx, rx, loss, rtt) = self.tun.stats();
        TunnelStats {
            time_since_handshake: time,
            tx_bytes: tx,
            rx_bytes: rx,
            estimated_loss: loss,
            last_rtt_ms: rtt,
        }
    }
}

fn header_scalars(iface: &crate::config::Interface) -> (u32, u32, u32, u32, u32, u32, u32, u32) {
    // `(0, 0)` → BoringTun's WireGuard defaults (types 1–4).
    let pair = |h: Option<crate::config::HeaderSpec>| match h {
        Some(spec) => spec.start_end(),
        None => (0, 0),
    };
    let (h1s, h1e) = pair(iface.h1);
    let (h2s, h2e) = pair(iface.h2);
    let (h3s, h3e) = pair(iface.h3);
    let (h4s, h4e) = pair(iface.h4);
    (h1s, h1e, h2s, h2e, h3s, h3e, h4s, h4e)
}

fn amnezia_from_interface(iface: &crate::config::Interface) -> AmneziaConfig {
    let s1 = iface.s1.unwrap_or(0);
    let s2 = iface.s2.unwrap_or(0);
    let s3 = iface.s3.unwrap_or(0);
    let s4 = iface.s4.unwrap_or(0);
    let mut cfg = AmneziaConfig::new(s1, s2, s3, s4);
    if let (Some(jc), Some(jmin), Some(jmax)) = (iface.jc, iface.jmin, iface.jmax) {
        cfg = cfg.with_pre_handshake_junk(jc, jmin, jmax, 0);
    } else if let Some(jc) = iface.jc {
        cfg = cfg.with_pre_handshake_junk(jc, iface.jmin.unwrap_or(0), iface.jmax.unwrap_or(0), 0);
    }
    cfg
}
