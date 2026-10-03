//! `%LOCALAPPDATA%\AGG\config.json` — source of truth for session flags.

use serde::{Deserialize, Serialize};

use crate::policy::SessionOpts;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "on")]
    pub auto_reconnect: bool,
    #[serde(default = "on")]
    pub mtu_sweep: bool,
    #[serde(default)]
    pub boosted_apps: Vec<String>,
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
            mtu_sweep: true,
            boosted_apps: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn to_opts(&self) -> SessionOpts {
        SessionOpts {
            kill_switch: self.kill_switch,
            auto_reconnect: self.auto_reconnect,
            mtu_sweep: self.mtu_sweep,
            boosted_apps: self.boosted_apps.clone(),
            direct_exes: Vec::new(),
        }
    }

    pub fn from_opts(opts: &SessionOpts, boosted: Vec<String>) -> Self {
        Self {
            version: 1,
            kill_switch: opts.kill_switch,
            auto_reconnect: opts.auto_reconnect,
            mtu_sweep: opts.mtu_sweep,
            boosted_apps: boosted,
        }
    }
}
