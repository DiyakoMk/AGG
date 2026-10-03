//! Windows platform: Wintun virtual adapter for the real tunnel.
//! Optional WinpkFilter detect/listen (no inject — that bugchecks).
//!
//! Off-Windows this crate is stubs. Official `wintun.dll` is bundled (Prebuilt Binaries License).

use thiserror::Error;

pub const DRIVER_NAME: &str = "NDISRD";
pub const DRIVER_DOWNLOAD: &str = "https://github.com/wiresock/ndisapi/releases";
pub const WINTUN_DOWNLOAD: &str = "https://www.wintun.net/";

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("{0}")]
    Message(String),
    #[error("Windows Packet Filter (NDISRD) is not installed. Personal/non-commercial runtime: {DRIVER_DOWNLOAD}")]
    DriverMissing,
    #[error("this command is Windows-only")]
    NotWindows,
}

impl PlatformError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

#[derive(Debug, Clone)]
pub struct DriverStatus {
    pub present: bool,
    pub version: Option<String>,
    pub adapters: Vec<AdapterInfo>,
    pub hint: String,
}

#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub index: usize,
    pub name: String,
    pub friendly: String,
    pub mac: String,
    pub mtu: u16,
}

#[cfg(windows)]
mod session;
#[cfg(windows)]
mod wintun_tun;
#[cfg(windows)]
pub use session::{capture_one, detect, intercept_loop, list_adapters, restore_all_adapters};
#[cfg(windows)]
pub use wintun_tun::{wintun_down, wintun_up, wintun_up_with_stats};

#[cfg(not(windows))]
pub fn detect() -> Result<DriverStatus, PlatformError> {
    Ok(DriverStatus {
        present: false,
        version: None,
        adapters: Vec::new(),
        hint: format!(
            "Windows-only. Tunnel uses Wintun ({WINTUN_DOWNLOAD}). WinpkFilter optional: {DRIVER_DOWNLOAD}"
        ),
    })
}

#[cfg(not(windows))]
pub fn list_adapters() -> Result<Vec<AdapterInfo>, PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn capture_one(_index: usize) -> Result<String, PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn intercept_loop(
    _index: usize,
    _cfg: &agg_core::WgConfig,
    _running: &std::sync::atomic::AtomicBool,
) -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn restore_all_adapters() -> Result<usize, PlatformError> {
    Err(PlatformError::NotWindows)
}

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
    _on_stats: impl FnMut(agg_core::TunnelStats),
) -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}

#[cfg(not(windows))]
pub fn wintun_down() -> Result<(), PlatformError> {
    Err(PlatformError::NotWindows)
}
