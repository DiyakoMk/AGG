//! AmneziaWG 2.0 client core. No Tauri, no Windows APIs.

pub mod config;
pub mod error;
pub mod ipc;
pub mod policy;
pub mod stats;
pub mod status;
pub mod store;
pub mod tunnel;
pub mod udp_session;

pub use config::{parse_conf, WgConfig};
pub use error::AggError;
pub use policy::{bypass_nets, tunnel_nets, BypassApp, SessionOpts, SiteMode};
pub use stats::TunnelStats;
pub use status::{ConnectionState, StatusSnapshot};
pub use store::AppConfig;
pub use tunnel::TunnelEngine;
pub use udp_session::run_udp_session;
