//! Session options. Tunnel uses config AllowedIPs (full tunnel).
//! BOOSTED apps ride that default route. DIRECT apps are blocked from
//! the tunnel address so Windows falls back to the LAN default.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionOpts {
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_true")]
    pub mtu_sweep: bool,
    #[serde(default)]
    pub boosted_apps: Vec<String>,
    /// Absolute exe paths that must NOT use the Wintun address (DIRECT).
    #[serde(default)]
    pub direct_exes: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for SessionOpts {
    fn default() -> Self {
        Self {
            kill_switch: false,
            auto_reconnect: true,
            mtu_sweep: true,
            boosted_apps: Vec::new(),
            direct_exes: Vec::new(),
        }
    }
}

impl SessionOpts {
    pub fn sanitized(&self) -> Self {
        let mut out = self.clone();
        out.direct_exes.retain(|p| !p.trim().is_empty());
        out.direct_exes.sort();
        out.direct_exes.dedup();
        out
    }

    pub fn label(&self) -> String {
        if self.boosted_apps.is_empty() {
            "Full tunnel".into()
        } else {
            format!("{} boosted", self.boosted_apps.len())
        }
    }
}

pub fn tunnel_nets(_opts: &SessionOpts, allowed: &[ipnet::IpNet]) -> Vec<ipnet::IpNet> {
    allowed.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnet::IpNet;

    #[test]
    fn keeps_allowed() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        assert_eq!(tunnel_nets(&SessionOpts::default(), &[full]), vec![full]);
    }
}
