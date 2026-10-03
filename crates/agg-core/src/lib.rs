//! Platform-agnostic AGG core. No Tauri, no Windows APIs.
//!
//! Phase 0: AmneziaWG 2.0 config + BoringTun `Tunn` lifecycle + stats.

pub mod config;
pub mod error;
pub mod health;
pub mod intercept;
pub mod ipc;
pub mod packet;
pub mod policy;
pub mod stats;
pub mod status;
pub mod tunnel;
pub mod udp_session;

pub use config::{parse_conf, WgConfig};
pub use error::AggError;
pub use intercept::{Direction, Intercept, Verdict};
pub use stats::TunnelStats;
pub use status::{ConnectionState, StatusSnapshot};
pub use tunnel::TunnelEngine;
pub use udp_session::run_udp_session;
