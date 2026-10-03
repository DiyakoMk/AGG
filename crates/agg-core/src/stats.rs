use std::time::Duration;

/// Snapshot from BoringTun `Tunn::stats`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TunnelStats {
    pub time_since_handshake: Option<Duration>,
    pub tx_bytes: usize,
    pub rx_bytes: usize,
    pub estimated_loss: f32,
    /// Last handshake RTT in milliseconds, if known.
    pub last_rtt_ms: Option<u32>,
}

impl TunnelStats {
    pub fn handshake_ok(&self) -> bool {
        self.time_since_handshake.is_some()
    }
}
