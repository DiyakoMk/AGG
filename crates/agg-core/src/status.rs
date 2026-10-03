use serde::{Deserialize, Serialize};

use crate::stats::TunnelStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    #[default]
    Idle,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

/// Pushed to the UI at 1 Hz. No secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusSnapshot {
    pub state: ConnectionState,
    pub server: Option<String>,
    pub endpoint: Option<String>,
    pub rtt_ms: Option<u32>,
    pub handshake_age_ms: Option<u64>,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub loss: f32,
    pub error: Option<String>,
    pub profile_id: Option<String>,
    #[serde(default)]
    pub rtt_history: Vec<u32>,
}

impl StatusSnapshot {
    pub fn idle() -> Self {
        Self {
            state: ConnectionState::Idle,
            server: None,
            endpoint: None,
            rtt_ms: None,
            handshake_age_ms: None,
            tx_bytes: 0,
            rx_bytes: 0,
            loss: 0.0,
            error: None,
            profile_id: None,
            rtt_history: Vec::new(),
        }
    }

    pub fn connecting(endpoint: Option<String>) -> Self {
        Self {
            state: ConnectionState::Connecting,
            endpoint,
            ..Self::idle()
        }
    }

    pub fn from_stats(endpoint: Option<String>, stats: TunnelStats) -> Self {
        let connected = stats.handshake_ok();
        Self {
            state: if connected {
                ConnectionState::Connected
            } else {
                ConnectionState::Connecting
            },
            server: endpoint.clone(),
            endpoint,
            rtt_ms: stats.last_rtt_ms,
            handshake_age_ms: stats.time_since_handshake.map(|d| d.as_millis() as u64),
            tx_bytes: stats.tx_bytes as u64,
            rx_bytes: stats.rx_bytes as u64,
            loss: stats.estimated_loss,
            error: None,
            profile_id: None,
            rtt_history: Vec::new(),
        }
    }

    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            state: ConnectionState::Error,
            error: Some(msg.into()),
            ..Self::idle()
        }
    }
}
