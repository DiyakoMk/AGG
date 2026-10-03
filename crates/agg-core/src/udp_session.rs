//! UDP-only session: handshake + timers. No TUN. Used by the UI on non-Windows
//! and as the control-plane keepalive wherever the datapath is separate.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use boringtun::noise::TunnResult;

use crate::config::WgConfig;
use crate::error::AggError;
use crate::stats::TunnelStats;
use crate::tunnel::TunnelEngine;

const BUF: usize = 2048;

pub fn run_udp_session(
    cfg: &WgConfig,
    running: &AtomicBool,
    mut on_stats: impl FnMut(TunnelStats),
) -> Result<(), AggError> {
    let endpoint = cfg
        .peer()?
        .endpoint
        .ok_or_else(|| AggError::config("peer has no Endpoint"))?;
    let mut engine = TunnelEngine::from_config(cfg)?;
    let udp = UdpSocket::bind("0.0.0.0:0")?;
    udp.connect(endpoint)?;
    udp.set_nonblocking(true)?;

    let mut src = [0u8; BUF];
    let mut dst = [0u8; BUF];
    match engine.kick_handshake(&mut dst) {
        TunnResult::WriteToNetwork(p) => {
            let _ = udp.send(p);
        }
        TunnResult::Err(e) => return Err(AggError::Tunnel(format!("{e:?}"))),
        _ => {}
    }
    drain(&mut engine, &udp, &mut dst);

    let mut last_timer = Instant::now();
    let mut last_stats = Instant::now();
    on_stats(engine.stats());

    while running.load(Ordering::SeqCst) {
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
            Err(e) => tracing::warn!("udp recv: {e}"),
        }

        if last_timer.elapsed() >= Duration::from_millis(250) {
            last_timer = Instant::now();
            match engine.update_timers(&mut dst) {
                TunnResult::WriteToNetwork(p) => {
                    let _ = udp.send(p);
                    drain(&mut engine, &udp, &mut dst);
                }
                _ => {}
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            on_stats(engine.stats());
        }

        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn drain(engine: &mut TunnelEngine, udp: &UdpSocket, dst: &mut [u8]) {
    loop {
        match engine.decapsulate(None, &[], dst) {
            TunnResult::WriteToNetwork(p) => {
                let _ = udp.send(p);
            }
            _ => break,
        }
    }
}
