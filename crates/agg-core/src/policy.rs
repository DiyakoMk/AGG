//! Amnezia-style split (Windows client).
//!
//! Sites: all / only listed through VPN / listed bypass VPN.
//! Apps: listed executables work *without* VPN (Amnezia Windows exceptions mode).

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SiteMode {
    #[default]
    All,
    OnlyListed,
    ExceptListed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionOpts {
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub site_mode: SiteMode,
    #[serde(default)]
    pub split_sites: Vec<String>,
    /// Apps that work without VPN (Amnezia Windows exceptions).
    #[serde(default)]
    pub bypass_apps: Vec<BypassApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BypassApp {
    pub name: String,
    pub path: String,
}

fn default_true() -> bool {
    true
}

impl Default for SessionOpts {
    fn default() -> Self {
        Self {
            kill_switch: false,
            auto_reconnect: true,
            site_mode: SiteMode::All,
            split_sites: Vec::new(),
            bypass_apps: Vec::new(),
        }
    }
}

impl SessionOpts {
    pub fn sanitized(&self) -> Self {
        let mut out = self.clone();
        out.bypass_apps.retain(|a| !a.path.trim().is_empty());
        out.split_sites.retain(|s| s.parse::<IpNet>().is_ok());
        out
    }

    pub fn direct_exes(&self) -> Vec<String> {
        self.bypass_apps.iter().map(|a| a.path.clone()).collect()
    }

    pub fn label(&self) -> String {
        match self.site_mode {
            SiteMode::All if self.bypass_apps.is_empty() => "All traffic".into(),
            SiteMode::All => format!("All traffic · {} apps bypass", self.bypass_apps.len()),
            SiteMode::OnlyListed => "Listed IPs through VPN".into(),
            SiteMode::ExceptListed => "Listed IPs bypass VPN".into(),
        }
    }
}

/// Nets on the Wintun adapter.
pub fn tunnel_nets(opts: &SessionOpts, allowed: &[IpNet]) -> Vec<IpNet> {
    let s = opts.sanitized();
    match s.site_mode {
        SiteMode::All | SiteMode::ExceptListed => allowed.to_vec(),
        SiteMode::OnlyListed => {
            let listed: Vec<IpNet> = s
                .split_sites
                .iter()
                .filter_map(|x| x.parse().ok())
                .collect();
            if listed.is_empty() {
                allowed.to_vec()
            } else {
                listed
            }
        }
    }
}

/// More-specific LAN routes so listed IPs skip the tunnel.
pub fn bypass_nets(opts: &SessionOpts) -> Vec<IpNet> {
    let s = opts.sanitized();
    if s.site_mode != SiteMode::ExceptListed {
        return Vec::new();
    }
    s.split_sites
        .iter()
        .filter_map(|x| x.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_replaces_default() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let opts = SessionOpts {
            site_mode: SiteMode::OnlyListed,
            split_sites: vec!["1.1.1.0/24".into()],
            ..Default::default()
        };
        let nets = tunnel_nets(&opts, &[full]);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].to_string(), "1.1.1.0/24");
    }

    #[test]
    fn except_listed_keeps_allowed() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let opts = SessionOpts {
            site_mode: SiteMode::ExceptListed,
            split_sites: vec!["10.0.0.0/8".into()],
            ..Default::default()
        };
        assert_eq!(tunnel_nets(&opts, &[full]), vec![full]);
        assert_eq!(bypass_nets(&opts).len(), 1);
    }
}
