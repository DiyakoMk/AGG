//! Session options. The tunnel uses the config AllowedIPs (usually full tunnel).
//! App selection lives in `discovery`; it does not change the route table.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionOpts {
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_true")]
    pub mtu_sweep: bool,
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
        }
    }
}

impl SessionOpts {
    pub fn sanitized(&self) -> Self {
        self.clone()
    }

    pub fn label(&self) -> String {
        "Full tunnel".into()
    }
}

/// Nets to install on the Wintun adapter. Always the config AllowedIPs.
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
