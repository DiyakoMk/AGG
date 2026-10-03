//! Detect Windows Packet Filter (NDISRD). Do not bundle the driver.
//! Official runtime: https://github.com/wiresock/ndisapi/releases

use serde::{Deserialize, Serialize};

pub const DRIVER_NAME: &str = "NDISRD";
pub const DRIVER_DOWNLOAD: &str = "https://github.com/wiresock/ndisapi/releases";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterStatus {
    pub present: bool,
    pub download: String,
    pub hint: String,
}

pub fn detect() -> FilterStatus {
    #[cfg(windows)]
    {
        match ndisapi::Ndisapi::new(DRIVER_NAME) {
            Ok(_) => FilterStatus {
                present: true,
                download: DRIVER_DOWNLOAD.into(),
                hint: "Windows Packet Filter is installed.".into(),
            },
            Err(_) => missing(),
        }
    }
    #[cfg(not(windows))]
    {
        missing()
    }
}

fn missing() -> FilterStatus {
    FilterStatus {
        present: false,
        download: DRIVER_DOWNLOAD.into(),
        hint: format!(
            "Install Windows Packet Filter for personal use, then Refresh. {DRIVER_DOWNLOAD}"
        ),
    }
}
