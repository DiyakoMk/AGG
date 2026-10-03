//! Real Windows tunnel: Wintun adapter + UDP AmneziaWG.
//!
//! Handshake first. Adapter created only after the session exists.
//! Adapter and extra routes are removed on Drop / `wintun_down`.
//! `wintun.dll` is not bundled — official build from https://www.wintun.net/

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agg_core::config::WgConfig;
use agg_core::TunnelEngine;
use boringtun::noise::TunnResult;
use ipnet::IpNet;
use wintun::{Adapter, Session};

use super::PlatformError;

pub const ADAPTER_NAME: &str = "AGG";
pub const TUNNEL_TYPE: &str = "AGG";
pub const ADAPTER_GUID: u128 = 0x4147_4700_0000_4000_8000_0000_0000_0001;
pub const WINTUN_DOWNLOAD: &str = "https://www.wintun.net/";
const BUF: usize = 2048;

pub fn wintun_up(cfg: &WgConfig, running: &AtomicBool) -> Result<(), PlatformError> {
    wintun_up_with_stats(cfg, running, |_| {})
}

pub fn wintun_up_with_stats(
    cfg: &WgConfig,
    running: &AtomicBool,
    mut on_stats: impl FnMut(agg_core::TunnelStats),
) -> Result<(), PlatformError> {
    let peer = cfg
        .peer()
        .map_err(|e| PlatformError::msg(e.to_string()))?
        .clone();
    let endpoint = peer
        .endpoint
        .ok_or_else(|| PlatformError::msg("peer has no Endpoint"))?;
    let (tun_ip, plen) = cfg
        .interface
        .addresses
        .iter()
        .find_map(|n| match n.addr() {
            IpAddr::V4(v) => Some((v, n.prefix_len())),
            _ => None,
        })
        .ok_or_else(|| PlatformError::msg("[Interface] Address must include IPv4"))?;
    let mtu = cfg.interface.mtu.unwrap_or(1280);

    eprintln!("handshake {endpoint} (no adapter yet)");
    let mut engine = TunnelEngine::from_config(cfg).map_err(|e| PlatformError::msg(e.to_string()))?;
    let udp = bind_udp(endpoint)?;
    if !drive_handshake(&mut engine, &udp, endpoint, Duration::from_secs(15))? {
        return Err(PlatformError::msg(
            "handshake failed; AGG adapter was not created",
        ));
    }
    eprintln!("handshake ok — creating Wintun adapter {ADAPTER_NAME}");

    let mut tun = WintunTun::open(tun_ip, plen, mtu, &cfg.interface.dns)?;
    eprintln!("adapter {ADAPTER_NAME} if={}", tun.if_index);

    let mut routes = WinRoutes::apply(endpoint.ip(), tun.if_index, tun_ip, &peer.allowed_ips)?;
    eprintln!("routes on; forwarding until Ctrl-C");

    let mut last_timer = Instant::now();
    let mut last_stats = Instant::now();
    let mut dst = [0u8; BUF];

    while running.load(Ordering::SeqCst) {
        match tun.try_recv() {
            Ok(Some(pkt)) => match engine.encapsulate(&pkt, &mut dst) {
                TunnResult::WriteToNetwork(p) => {
                    let _ = udp.send(p);
                    drain_net(&mut engine, &udp, &mut dst);
                }
                TunnResult::Err(e) => tracing::debug!("encapsulate: {e:?}"),
                _ => {}
            },
            Ok(None) => {}
            Err(e) => tracing::warn!("wintun recv: {e}"),
        }

        match udp.recv(&mut dst) {
            Ok(n) if n > 0 => {
                let incoming = dst[..n].to_vec();
                let mut datagram: &[u8] = &incoming;
                loop {
                    match engine.decapsulate(Some(endpoint.ip()), datagram, &mut dst) {
                        TunnResult::WriteToNetwork(p) => {
                            let _ = udp.send(p);
                            datagram = &[];
                        }
                        TunnResult::WriteToTunnelV4(p, _) | TunnResult::WriteToTunnelV6(p, _) => {
                            tun.send(p);
                            break;
                        }
                        TunnResult::Done | TunnResult::Err(_) => break,
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => tracing::warn!("udp recv: {e}"),
        }

        if last_timer.elapsed() >= Duration::from_millis(250) {
            last_timer = Instant::now();
            match engine.update_timers(&mut dst) {
                TunnResult::WriteToNetwork(p) => {
                    let _ = udp.send(p);
                    drain_net(&mut engine, &udp, &mut dst);
                }
                _ => {}
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            let s = engine.stats();
            on_stats(s);
            eprintln!(
                "agg up  handshake={:?} rtt_ms={:?} tx={} rx={}",
                s.time_since_handshake, s.last_rtt_ms, s.tx_bytes, s.rx_bytes
            );
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    eprintln!("tearing down adapter and routes");
    routes.restore();
    tun.delete();
    Ok(())
}

pub fn wintun_down() -> Result<(), PlatformError> {
    WinRoutes::restore_defaults();
    let wintun = match load_dll() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("wintun.dll not loaded ({e}); nothing to delete");
            return Ok(());
        }
    };
    // open() logs ERROR 0x490 when the NIC is already gone — that is success.
    match Adapter::open(&wintun, ADAPTER_NAME) {
        Ok(adapter) => match Arc::try_unwrap(adapter) {
            Ok(a) => {
                a.delete()
                    .map_err(|e| PlatformError::msg(format!("delete AGG: {e}")))?;
                eprintln!("deleted leftover AGG adapter");
            }
            Err(_) => {
                return Err(PlatformError::msg(
                    "AGG adapter still in use; stop `up` first",
                ));
            }
        },
        Err(_) => eprintln!("no AGG adapter to delete"),
    }
    Ok(())
}

fn bind_udp(endpoint: SocketAddr) -> Result<UdpSocket, PlatformError> {
    let udp = UdpSocket::bind("0.0.0.0:0").map_err(|e| PlatformError::msg(e.to_string()))?;
    udp.connect(endpoint)
        .map_err(|e| PlatformError::msg(format!("udp connect {endpoint}: {e}")))?;
    udp.set_nonblocking(true)
        .map_err(|e| PlatformError::msg(e.to_string()))?;
    Ok(udp)
}

fn drive_handshake(
    engine: &mut TunnelEngine,
    udp: &UdpSocket,
    endpoint: SocketAddr,
    timeout: Duration,
) -> Result<bool, PlatformError> {
    let mut src = [0u8; BUF];
    let mut dst = [0u8; BUF];
    let deadline = Instant::now() + timeout;
    match engine.kick_handshake(&mut dst) {
        TunnResult::WriteToNetwork(p) => {
            let _ = udp.send(p);
        }
        TunnResult::Err(e) => return Err(PlatformError::msg(format!("kick: {e:?}"))),
        _ => {}
    }
    drain_net(engine, udp, &mut dst);

    while Instant::now() < deadline {
        match udp.recv(&mut src) {
            Ok(n) if n > 0 => {
                let incoming = src[..n].to_vec();
                let mut d: &[u8] = &incoming;
                loop {
                    match engine.decapsulate(Some(endpoint.ip()), d, &mut dst) {
                        TunnResult::WriteToNetwork(p) => {
                            let _ = udp.send(p);
                            d = &[];
                        }
                        _ => break,
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => tracing::warn!("udp: {e}"),
        }
        match engine.update_timers(&mut dst) {
            TunnResult::WriteToNetwork(p) => {
                let _ = udp.send(p);
                drain_net(engine, udp, &mut dst);
            }
            _ => {}
        }
        if engine.stats().handshake_ok() {
            eprintln!("handshake ok rtt_ms={:?}", engine.stats().last_rtt_ms);
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(false)
}

fn drain_net(engine: &mut TunnelEngine, udp: &UdpSocket, dst: &mut [u8]) {
    loop {
        match engine.decapsulate(None, &[], dst) {
            TunnResult::WriteToNetwork(p) => {
                let _ = udp.send(p);
            }
            _ => break,
        }
    }
}

/// Official signed amd64 `wintun.dll` from wintun.net (Prebuilt Binaries License).
/// Not GPLv2 source. Redistributed next to our exe via the permitted API.
#[cfg(target_arch = "x86_64")]
const BUNDLED_WINTUN: &[u8] = include_bytes!("../../../third_party/wintun/amd64/wintun.dll");
#[cfg(target_arch = "aarch64")]
const BUNDLED_WINTUN: &[u8] = include_bytes!("../../../third_party/wintun/arm64/wintun.dll");
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const BUNDLED_WINTUN: &[u8] = &[];

fn load_dll() -> Result<wintun::Wintun, PlatformError> {
    let path = ensure_dll()?;
    let lib = unsafe { wintun::load_from_path(&path) }
        .map_err(|e| PlatformError::msg(format!("load {}: {e}", path.display())))?;
    // Adapter::open logs ERROR 0x490 when the NIC is absent. Harmless.
    wintun::reset_logger(&lib);
    Ok(lib)
}

fn ensure_dll() -> Result<PathBuf, PlatformError> {
    if let Ok(p) = std::env::var("WINTUN_DLL") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe().map_err(|e| PlatformError::msg(e.to_string()))?;
    let dir = exe.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let dest = dir.join("wintun.dll");
    if dest.exists() {
        return Ok(dest);
    }
    if BUNDLED_WINTUN.is_empty() {
        return Err(PlatformError::msg(format!(
            "no bundled wintun.dll for this arch. Official zip: {WINTUN_DOWNLOAD}"
        )));
    }
    std::fs::write(&dest, BUNDLED_WINTUN)
        .map_err(|e| PlatformError::msg(format!("write {}: {e}", dest.display())))?;
    Ok(dest)
}

struct WintunTun {
    adapter: Option<Arc<Adapter>>,
    session: Option<Arc<Session>>,
    if_index: u32,
}

impl WintunTun {
    fn open(
        ip: Ipv4Addr,
        prefix: u8,
        mtu: u16,
        dns: &[IpAddr],
    ) -> Result<Self, PlatformError> {
        let wintun = load_dll()?;
        // Do not Adapter::open first: missing NIC logs ERROR 0x490 even when handled.
        let adapter = match Adapter::create(&wintun, ADAPTER_NAME, TUNNEL_TYPE, Some(ADAPTER_GUID)) {
            Ok(a) => a,
            Err(create_err) => match Adapter::open(&wintun, ADAPTER_NAME) {
                Ok(a) => a,
                Err(open_err) => {
                    return Err(PlatformError::msg(format!(
                        "create AGG adapter failed ({create_err}); open also failed ({open_err}). Run the shell as Administrator. If wintun.dll is 32-bit vs 64-bit mismatch, replace it."
                    )));
                }
            }
        };
        adapter
            .set_mtu(mtu as usize)
            .map_err(|e| PlatformError::msg(format!("set mtu: {e}")))?;
        let mask = prefix_to_mask(prefix);
        adapter
            .set_network_addresses_tuple(IpAddr::V4(ip), IpAddr::V4(mask), None)
            .map_err(|e| PlatformError::msg(format!("set address {ip}/{prefix}: {e}")))?;
        if !dns.is_empty() {
            let _ = adapter.set_dns_servers(dns);
        }
        let if_index = adapter
            .get_adapter_index()
            .map_err(|e| PlatformError::msg(format!("adapter index: {e}")))?;
        // 8 MiB ring — MAX_RING_CAPACITY can fail on some machines.
        const RING: u32 = 0x80_0000;
        let session = adapter
            .start_session(RING)
            .or_else(|e| {
                tracing::warn!("start_session({RING:#x}) failed ({e}); trying 2 MiB");
                adapter.start_session(0x20_0000)
            })
            .map_err(|e| PlatformError::msg(format!("start session: {e}")))?;
        Ok(Self {
            adapter: Some(adapter),
            session: Some(Arc::new(session)),
            if_index,
        })
    }

    fn try_recv(&self) -> Result<Option<Vec<u8>>, PlatformError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| PlatformError::msg("session closed"))?;
        match session.try_receive() {
            Ok(Some(pkt)) => Ok(Some(pkt.bytes().to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(PlatformError::msg(format!("try_receive: {e}"))),
        }
    }

    fn send(&self, data: &[u8]) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if data.is_empty() || data.len() > u16::MAX as usize {
            return;
        }
        match session.allocate_send_packet(data.len() as u16) {
            Ok(mut pkt) => {
                pkt.bytes_mut()[..data.len()].copy_from_slice(data);
                session.send_packet(pkt);
            }
            Err(e) => tracing::debug!("allocate_send_packet: {e}"),
        }
    }

    fn delete(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.shutdown();
            drop(s);
        }
        if let Some(a) = self.adapter.take() {
            match Arc::try_unwrap(a) {
                Ok(adapter) => {
                    if let Err(e) = adapter.delete() {
                        tracing::error!("delete AGG adapter: {e}");
                    }
                }
                Err(_) => tracing::error!("AGG adapter still shared; NIC may remain — run agg-cli down"),
            }
        }
    }
}

impl Drop for WintunTun {
    fn drop(&mut self) {
        self.delete();
    }
}

fn prefix_to_mask(prefix: u8) -> Ipv4Addr {
    let bits: u32 = if prefix >= 32 {
        u32::MAX
    } else if prefix == 0 {
        0
    } else {
        !((1u32 << (32 - prefix)) - 1)
    };
    Ipv4Addr::from(bits)
}

struct WinRoutes {
    added: Vec<Vec<String>>,
}

impl WinRoutes {
    fn apply(
        endpoint: IpAddr,
        if_index: u32,
        tun_ip: Ipv4Addr,
        allowed: &[IpNet],
    ) -> Result<Self, PlatformError> {
        let gw = default_gateway()?;
        let mut g = Self { added: Vec::new() };
        g.add(&[
            "add",
            &endpoint.to_string(),
            "mask",
            "255.255.255.255",
            &gw,
            "metric",
            "1",
        ])?;
        for net in allowed {
            if !net.addr().is_ipv4() {
                continue;
            }
            if net.prefix_len() == 0 {
                g.add(&[
                    "add",
                    "0.0.0.0",
                    "mask",
                    "128.0.0.0",
                    &tun_ip.to_string(),
                    "if",
                    &if_index.to_string(),
                    "metric",
                    "1",
                ])?;
                g.add(&[
                    "add",
                    "128.0.0.0",
                    "mask",
                    "128.0.0.0",
                    &tun_ip.to_string(),
                    "if",
                    &if_index.to_string(),
                    "metric",
                    "1",
                ])?;
            } else if let IpAddr::V4(a) = net.addr() {
                let mask = prefix_to_mask(net.prefix_len());
                g.add(&[
                    "add",
                    &a.to_string(),
                    "mask",
                    &mask.to_string(),
                    &tun_ip.to_string(),
                    "if",
                    &if_index.to_string(),
                    "metric",
                    "1",
                ])?;
            }
        }
        Ok(g)
    }

    fn add(&mut self, args: &[&str]) -> Result<(), PlatformError> {
        route(args)?;
        self.added
            .push(args.iter().skip(1).map(|s| (*s).to_string()).collect());
        Ok(())
    }

    fn restore(&mut self) {
        for spec in self.added.iter().rev() {
            let mut args = vec!["delete".to_string()];
            args.extend(spec.iter().cloned());
            // metric/if extras on delete can fail; try full then dest+mask only
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            if route(&refs).is_err() && spec.len() >= 3 {
                let short = ["delete", spec[0].as_str(), "mask", spec[2].as_str()];
                let _ = route(&short);
            }
        }
        self.added.clear();
    }

    fn restore_defaults() {
        let _ = route(&["delete", "0.0.0.0", "mask", "128.0.0.0"]);
        let _ = route(&["delete", "128.0.0.0", "mask", "128.0.0.0"]);
    }
}

impl Drop for WinRoutes {
    fn drop(&mut self) {
        self.restore();
    }
}

fn default_gateway() -> Result<String, PlatformError> {
    if let Ok(list) = wintun::get_active_network_interface_gateways() {
        for ip in list {
            if let IpAddr::V4(v) = ip {
                if !v.is_unspecified() && !v.is_loopback() {
                    return Ok(v.to_string());
                }
            }
        }
    }
    Err(PlatformError::msg(
        "no IPv4 default gateway; cannot pin the VPS route",
    ))
}

fn route(args: &[&str]) -> Result<(), PlatformError> {
    let out = std::process::Command::new("route")
        .args(args)
        .output()
        .map_err(|e| PlatformError::msg(format!("route {}: {e}", args.join(" "))))?;
    if !out.status.success() {
        return Err(PlatformError::msg(format!(
            "route {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}
