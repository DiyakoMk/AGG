//! `%LOCALAPPDATA%\AGG\config.json`

use serde::{Deserialize, Serialize};

use crate::policy::{BypassApp, SessionOpts, SiteMode};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "on")]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub site_mode: SiteMode,
    #[serde(default)]
    pub split_sites: Vec<String>,
    #[serde(default)]
    pub bypass_apps: Vec<BypassApp>,
}

fn one() -> u32 {
    1
}
fn on() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            kill_switch: false,
            auto_reconnect: true,
            site_mode: SiteMode::All,
            split_sites: Vec::new(),
            bypass_apps: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn to_opts(&self) -> SessionOpts {
        SessionOpts {
            kill_switch: self.kill_switch,
            auto_reconnect: self.auto_reconnect,
            site_mode: self.site_mode,
            split_sites: self.split_sites.clone(),
            bypass_apps: self.bypass_apps.clone(),
        }
    }
}
