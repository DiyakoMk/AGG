//! AmneziaWG 2.0 + WireGuard INI parse and validate.

mod parse;
mod types;
mod validate;

pub use parse::parse_conf;
pub use types::{
    HeaderSpec, Interface, Peer, SecretKey, WgConfig, WG_TYPE_COOKIE, WG_TYPE_DATA, WG_TYPE_INIT,
    WG_TYPE_RESPONSE,
};

use crate::error::AggError;

impl WgConfig {
    pub fn from_str(s: &str) -> Result<Self, AggError> {
        let cfg = parse_conf(s)?;
        validate::validate(&cfg)?;
        Ok(cfg)
    }

    pub fn from_path(path: &std::path::Path) -> Result<Self, AggError> {
        let s = std::fs::read_to_string(path)?;
        Self::from_str(&s)
    }
}
