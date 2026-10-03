//! Windows platform: Wintun tunnel. Optional ndisapi detect (driver not bundled).

use thiserror::Error;

pub const WINTUN_DOWNLOAD: &str = "https://www.wintun.net/";

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("{0}")]
    Message(String),
    #[error("this command is Windows-only")]
    NotWindows,
}

impl PlatformError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

pub mod filter;

#[cfg(windows)]
pub mod wfp;
#[cfg(windows)]
mod wintun_tun;
#[cfg(windows)]
pub use wintun_tun::{wintun_down, wintun_up, wintun_up_with_stats};

pub use filter::{detect as detect_filter, FilterStatus, DRIVER_DOWNLOAD, DRIVER_NAME};

#[cfg(not(windows))]
pub fn wintun_up(
    _cfg: &agg_core::WgConfig,
    _running: &std::sync::atomic::AtomicBool,
) -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn wintun_up_with_stats(
    _cfg: &agg_core::WgConfig,
    _running: &std::sync::atomic::AtomicBool,
    _opts: &agg_core::SessionOpts,
    _on_stats: impl FnMut(agg_core::TunnelStats),
) -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn wintun_down() -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub mod wfp {
    pub struct KillSwitch;
    impl KillSwitch {
        pub fn disarm_leftovers() {}
    }
}
