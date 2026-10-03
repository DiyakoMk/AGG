//! Platform-agnostic AGG core. No Tauri, no Windows APIs.
//!
//! AmneziaWG 2.0 config + BoringTun `Tunn` lifecycle + stats + route split.

pub mod config;
pub mod error;
pub mod health;
pub mod ipc;
pub mod policy;
pub mod stats;
pub mod status;
pub mod tunnel;
pub mod udp_session;

pub use config::{parse_conf, WgConfig};
pub use error::AggError;
pub use policy::{catalog, destinations, processes, tunnel_nets, Layout, SessionOpts, MAX_LAYOUTS};
pub use stats::TunnelStats;
pub use status::{ConnectionState, StatusSnapshot};
pub use tunnel::TunnelEngine;
pub use udp_session::run_udp_session;
