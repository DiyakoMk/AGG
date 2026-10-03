#[cfg(unix)]
mod platform;
#[cfg(unix)]
mod route_guard;

use std::io::{self, Write};
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agg_core::config::WgConfig;
use agg_core::TunnelEngine;
use anyhow::{bail, Context, Result};
use boringtun::noise::TunnResult;
use clap::{Parser, Subcommand};

#[cfg(unix)]
use crate::platform::TunDevice;
#[cfg(unix)]
use crate::route_guard::RouteGuard;

#[cfg(unix)]
const STATE_FILE: &str = "/tmp/agg-cli.state";
const BUF: usize = 2048;

#[derive(Parser)]
#[command(name = "agg-cli", about = "AGG Phase 0 tunnel harness")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse and validate a config; print a redacted summary.
    Parse { #[arg(long)] config: PathBuf },
    /// UDP handshake only. No TUN, no routes. Safe to run unprivileged.
    Handshake {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value_t = 15)]
        timeout: u64,
    },
    /// Bring the tunnel up. Linux: TUN. Windows: Wintun adapter AGG.
    Up {
        #[arg(long)]
        config: PathBuf,
    },
    /// Tear down AGG adapter / Linux TUN routes.
    Down,
    /// Remove leftover kill-switch / DIRECT firewall rules (Windows, elevated).
    #[command(name = "kill-switch")]
    KillSwitch { action: String },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("agg_cli=info".parse().unwrap()),
        )
        .init();

    match Cli::parse().cmd {
        Cmd::Parse { config } => cmd_parse(&config),
        Cmd::Handshake { config, timeout } => cmd_handshake(&config, Duration::from_secs(timeout)),
        Cmd::Up { config } => cmd_up(&config),
        Cmd::Down => cmd_down(),
        Cmd::KillSwitch { action } => cmd_kill_switch(&action),
    }
}

fn say(msg: &str) {
    let _ = writeln!(io::stderr(), "{msg}");
    let _ = io::stderr().flush();
}

fn cmd_parse(path: &Path) -> Result<()> {
    let cfg = WgConfig::from_path(path)?;
    println!("interface addresses: {:?}", cfg.interface.addresses);
    println!("mtu: {:?}", cfg.interface.mtu);
    println!(
        "jc/jmin/jmax: {:?}/{:?}/{:?}",
        cfg.interface.jc, cfg.interface.jmin, cfg.interface.jmax
    );
    println!(
        "s1-s4: {:?}/{:?}/{:?}/{:?}",
        cfg.interface.s1, cfg.interface.s2, cfg.interface.s3, cfg.interface.s4
    );
    println!(
        "h1-h4: {:?}/{:?}/{:?}/{:?}",
        cfg.interface.h1, cfg.interface.h2, cfg.interface.h3, cfg.interface.h4
    );
    println!(
        "unknown [Interface] keys: {:?}",
        cfg.interface.extra.keys().collect::<Vec<_>>()
    );
    let peer = cfg.peer()?;
    println!("peer endpoint: {:?}", peer.endpoint);
    println!("allowed ips: {:?}", peer.allowed_ips);
    println!("private key: {:?}", cfg.interface.private_key);
    Ok(())
}

fn open_udp(endpoint: SocketAddr) -> Result<UdpSocket> {
    let udp = UdpSocket::bind("0.0.0.0:0").context("bind UDP")?;
    udp.connect(endpoint)
        .with_context(|| format!("connect UDP to {endpoint}"))?;
    udp.set_nonblocking(true)?;
    Ok(udp)
}

fn drain_network(engine: &mut TunnelEngine, udp: &UdpSocket, dst: &mut [u8]) {
    loop {
        match engine.decapsulate(None, &[], dst) {
            TunnResult::WriteToNetwork(pkt) => {
                let _ = udp.send(pkt);
            }
            _ => break,
        }
    }
}

/// Drive handshake over UDP. Returns true if a handshake completed.
fn run_handshake(
    engine: &mut TunnelEngine,
    udp: &UdpSocket,
    endpoint: SocketAddr,
    timeout: Duration,
) -> Result<bool> {
    let mut src = [0u8; BUF];
    let mut dst = [0u8; BUF];
    let deadline = Instant::now() + timeout;
    let mut last_timer = Instant::now();
    let mut last_status = Instant::now();
    let mut rx_datagrams: u64 = 0;
    let mut tx_datagrams: u64 = 0;

    say("kicking handshake (Jc junk + initiation)");
    match engine.kick_handshake(&mut dst) {
        TunnResult::WriteToNetwork(pkt) => {
            tx_datagrams += 1;
            if let Err(e) = udp.send(pkt) {
                say(&format!("udp send: {e}"));
            }
        }
        TunnResult::Err(e) => say(&format!("kick: {e:?}")),
        _ => {}
    }
    drain_network(engine, udp, &mut dst);

    while Instant::now() < deadline {
        match udp.recv(&mut src) {
            Ok(n) if n > 0 => {
                rx_datagrams += 1;
                let mut datagram: &[u8] = &src[..n];
                loop {
                    match engine.decapsulate(Some(endpoint.ip()), datagram, &mut dst) {
                        TunnResult::WriteToNetwork(pkt) => {
                            let _ = udp.send(pkt);
                            tx_datagrams += 1;
                            datagram = &[];
                        }
                        TunnResult::WriteToTunnelV4(_, _) | TunnResult::WriteToTunnelV6(_, _) => {
                            break;
                        }
                        TunnResult::Done => break,
                        TunnResult::Err(e) => {
                            say(&format!("decapsulate: {e:?}"));
                            break;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => say(&format!("udp recv: {e}")),
        }

        if last_timer.elapsed() >= Duration::from_millis(250) {
            last_timer = Instant::now();
            match engine.update_timers(&mut dst) {
                TunnResult::WriteToNetwork(pkt) => {
                    let _ = udp.send(pkt);
                    tx_datagrams += 1;
                    drain_network(engine, udp, &mut dst);
                }
                TunnResult::Err(e) => say(&format!("timers: {e:?}")),
                _ => {}
            }
        }

        let s = engine.stats();
        if s.handshake_ok() {
            say(&format!(
                "HANDSHAKE OK  rtt_ms={:?}  age={:?}  tx_dgrams={tx_datagrams} rx_dgrams={rx_datagrams}",
                s.last_rtt_ms, s.time_since_handshake
            ));
            return Ok(true);
        }

        if last_status.elapsed() >= Duration::from_secs(1) {
            last_status = Instant::now();
            say(&format!(
                "waiting… tx_dgrams={tx_datagrams} rx_dgrams={rx_datagrams} (no handshake yet)"
            ));
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    say(&format!(
        "HANDSHAKE TIMEOUT after {timeout:?}  tx_dgrams={tx_datagrams} rx_dgrams={rx_datagrams}"
    ));
    if rx_datagrams == 0 {
        say("no UDP replies — server down, UDP/443 blocked, or obfuscation params mismatch");
    } else {
        say("got UDP replies but no session — likely H/S param mismatch (bad obfuscation)");
    }
    Ok(false)
}

fn cmd_handshake(path: &Path, timeout: Duration) -> Result<()> {
    let cfg = WgConfig::from_path(path)?;
    let endpoint = cfg
        .peer()?
        .endpoint
        .context("peer has no Endpoint")?;
    say(&format!("peer {endpoint}  (UDP only, no routes)"));
    let mut engine = TunnelEngine::from_config(&cfg)?;
    let udp = open_udp(endpoint)?;
    if run_handshake(&mut engine, &udp, endpoint, timeout)? {
        Ok(())
    } else {
        bail!("handshake failed")
    }
}

fn cmd_up(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        return cmd_up_wintun(path);
    }
    #[cfg(unix)]
    {
        return cmd_up_unix(path);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        bail!("unsupported OS");
    }
}

#[cfg(windows)]
fn cmd_up_wintun(path: &Path) -> Result<()> {
    let cfg = WgConfig::from_path(path)?;
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        })?;
    }
    say("Windows up: UDP handshake, then Wintun adapter AGG. Ctrl-C deletes it.");
    say("wintun.dll is bundled; extracted next to this exe on first run.");
    agg_platform_windows::wintun_up(&cfg, &running)?;
    say("AGG adapter gone");
    Ok(())
}

#[cfg(unix)]
fn cmd_up_unix(path: &Path) -> Result<()> {
    let cfg = WgConfig::from_path(path)?;
    let peer = cfg.peer()?.clone();
    let endpoint = peer
        .endpoint
        .context("peer has no Endpoint (required for Phase 0)")?;
    let addr = cfg
        .interface
        .addresses
        .first()
        .copied()
        .context("[Interface] Address is required")?;
    let mtu = cfg.interface.mtu.unwrap_or(1280);

    say(&format!("building tunn for {endpoint}"));
    let mut engine = TunnelEngine::from_config(&cfg)?;
    let udp = open_udp(endpoint)?;
    say("udp bound");

    if !run_handshake(&mut engine, &udp, endpoint, Duration::from_secs(15))? {
        bail!("refusing to add routes: handshake did not complete");
    }

    say(&format!("opening TUN agg0 {addr} mtu={mtu}"));
    let tun = TunDevice::open("agg0", addr, mtu)?;
    tun.set_nonblocking(true)?;
    say(&format!("tun {}", tun.name()));

    let mut guard = RouteGuard::capture()?;
    let ipv4_allowed: Vec<_> = peer
        .allowed_ips
        .iter()
        .copied()
        .filter(|n| n.addr().is_ipv4())
        .collect();
    say("applying IPv4 tunnel routes (IPv6 ::/0 skipped in Phase 0)");
    guard.apply_tunnel_routes(endpoint.ip(), &ipv4_allowed)?;
    guard.persist(Path::new(STATE_FILE))?;
    say(&format!("routes persisted to {STATE_FILE}"));

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        })?;
    }

    say("forwarding (Ctrl-C to restore routes and exit)");
    let mut last_timer = Instant::now();
    let mut last_stats = Instant::now();
    let mut src = [0u8; BUF];
    let mut dst = [0u8; BUF];

    while running.load(Ordering::SeqCst) {
        match tun.read(&mut src) {
            Ok(n) if n > 0 => match engine.encapsulate(&src[..n], &mut dst) {
                TunnResult::WriteToNetwork(pkt) => {
                    let _ = udp.send(pkt);
                    drain_network(&mut engine, &udp, &mut dst);
                }
                TunnResult::Done => {}
                TunnResult::Err(e) => tracing::debug!("encapsulate: {e:?}"),
                _ => {}
            },
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => say(&format!("tun read: {e}")),
        }

        match udp.recv(&mut src) {
            Ok(n) if n > 0 => {
                let mut datagram: &[u8] = &src[..n];
                loop {
                    match engine.decapsulate(Some(endpoint.ip()), datagram, &mut dst) {
                        TunnResult::WriteToNetwork(pkt) => {
                            let _ = udp.send(pkt);
                            datagram = &[];
                        }
                        TunnResult::WriteToTunnelV4(pkt, _) | TunnResult::WriteToTunnelV6(pkt, _) => {
                            let _ = tun.write(pkt);
                            break;
                        }
                        TunnResult::Done => break,
                        TunnResult::Err(e) => {
                            tracing::debug!("decapsulate: {e:?}");
                            break;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => say(&format!("udp recv: {e}")),
        }

        if last_timer.elapsed() >= Duration::from_millis(250) {
            last_timer = Instant::now();
            match engine.update_timers(&mut dst) {
                TunnResult::WriteToNetwork(pkt) => {
                    let _ = udp.send(pkt);
                    drain_network(&mut engine, &udp, &mut dst);
                }
                TunnResult::Err(e) => tracing::debug!("timers: {e:?}"),
                _ => {}
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            let s = engine.stats();
            say(&format!(
                "status handshake={:?} rtt_ms={:?} tx={} rx={} loss={:.4}",
                s.time_since_handshake, s.last_rtt_ms, s.tx_bytes, s.rx_bytes, s.estimated_loss
            ));
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    say("shutting down — restoring routes");
    drop(tun);
    guard.restore()?;
    let _ = std::fs::remove_file(STATE_FILE);
    say("down");
    Ok(())
}

fn cmd_kill_switch(action: &str) -> Result<()> {
    if action != "off" {
        bail!("use: agg-cli kill-switch off");
    }
    #[cfg(windows)]
    {
        agg_platform_windows::wfp::KillSwitch::disarm_leftovers();
        say("kill switch and DIRECT rules removed");
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        bail!("Windows-only");
    }
}

fn cmd_down() -> Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = agg_platform_windows::wintun_down() {
            say(&format!("wintun down: {e}"));
        }
        return Ok(());
    }
    #[cfg(unix)]
    {
        return cmd_down_unix();
    }
    #[cfg(not(any(unix, windows)))]
    Ok(())
}

#[cfg(unix)]
fn cmd_down_unix() -> Result<()> {
    if Path::new(STATE_FILE).exists() {
        RouteGuard::restore_from_file(Path::new(STATE_FILE))?;
        let _ = std::fs::remove_file(STATE_FILE);
        say(&format!("restored routes from {STATE_FILE}"));
    } else {
        say("no state file; nothing to restore");
    }
    let _ = std::process::Command::new("ip")
        .args(["link", "del", "agg0"])
        .status();
    Ok(())
}

