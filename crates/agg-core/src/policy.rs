//! Split by destination IP. No process injection, no WinpkFilter.
//!
//! Pick up to [`MAX_LAYOUTS`] layouts (Steam, Riot, Discord, …). Their
//! prefixes are unioned onto the tunnel; everything else stays on LAN.
//! `all` is exclusive and full-tunnels.

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

pub const MAX_LAYOUTS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionOpts {
    #[serde(default = "default_layouts")]
    pub layouts: Vec<String>,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_true")]
    pub mtu_sweep: bool,
}

fn default_layouts() -> Vec<String> {
    vec!["steam".into(), "discord".into()]
}

fn default_true() -> bool {
    true
}

impl Default for SessionOpts {
    fn default() -> Self {
        Self {
            layouts: default_layouts(),
            kill_switch: false,
            auto_reconnect: true,
            mtu_sweep: true,
        }
    }
}

impl SessionOpts {
    pub fn sanitized(&self) -> Self {
        let mut out = self.clone();
        let mut seen = Vec::new();
        for id in &self.layouts {
            if catalog().iter().any(|l| l.id == id) && !seen.iter().any(|s: &String| s == id) {
                seen.push(id.clone());
            }
            if seen.len() == MAX_LAYOUTS {
                break;
            }
        }
        if seen.iter().any(|s| s == "all") {
            seen = vec!["all".into()];
        }
        if seen.is_empty() {
            seen = default_layouts();
        }
        out.layouts = seen;
        out
    }

    pub fn label(&self) -> String {
        let s = self.sanitized();
        s.layouts
            .iter()
            .filter_map(|id| catalog().iter().find(|l| l.id == id).map(|l| l.name))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Layout {
    pub id: &'static str,
    pub name: &'static str,
    pub hint: &'static str,
    pub cidrs: &'static [&'static str],
    pub processes: &'static [&'static str],
}

pub fn catalog() -> &'static [Layout] {
    LAYOUTS
}

const LAYOUTS: &[Layout] = &[
    Layout {
        id: "steam",
        name: "Steam / Valve",
        hint: "CS, Dota, TF2, Steam download",
        cidrs: &[
            "45.121.184.0/24",
            "103.10.124.0/23",
            "103.28.54.0/24",
            "146.66.152.0/21",
            "155.133.224.0/19",
            "162.254.192.0/21",
            "185.25.180.0/22",
            "192.69.96.0/22",
            "205.196.6.0/24",
            "208.64.200.0/22",
            "208.78.164.0/22",
        ],
        processes: &["steam.exe", "cs2.exe", "dota2.exe", "hl2.exe"],
    },
    Layout {
        id: "riot",
        name: "Riot",
        hint: "League, Valorant, Riot Client",
        cidrs: &["104.160.128.0/19", "192.64.168.0/21"],
        processes: &[
            "leagueclient.exe",
            "league of legends.exe",
            "valorant.exe",
            "riotclientservices.exe",
        ],
    },
    Layout {
        id: "battlenet",
        name: "Battle.net",
        hint: "WoW, Overwatch, Diablo",
        cidrs: &["24.105.0.0/18", "137.221.64.0/18", "5.42.160.0/19"],
        processes: &["battle.net.exe", "agent.exe"],
    },
    Layout {
        id: "ea",
        name: "EA",
        hint: "EA App, Battlefield, FC",
        cidrs: &["159.153.0.0/16", "8.25.96.0/23"],
        processes: &["eaapp.exe", "origin.exe"],
    },
    Layout {
        id: "ubisoft",
        name: "Ubisoft",
        hint: "Ubisoft Connect",
        cidrs: &["216.98.48.0/20"],
        processes: &["upc.exe", "ubisoftconnect.exe"],
    },
    Layout {
        id: "discord",
        name: "Discord",
        hint: "Voice and client",
        cidrs: &["66.22.192.0/18"],
        processes: &["discord.exe", "discordcanary.exe", "discordptb.exe"],
    },
    Layout {
        id: "all",
        name: "Everything",
        hint: "Full tunnel — every app",
        cidrs: &["0.0.0.0/0"],
        processes: &[],
    },
];

pub fn destinations(opts: &SessionOpts) -> Vec<IpNet> {
    let s = opts.sanitized();
    let mut nets = Vec::new();
    for id in &s.layouts {
        if let Some(l) = LAYOUTS.iter().find(|l| l.id == id) {
            for c in l.cidrs {
                if let Ok(n) = c.parse::<IpNet>() {
                    if !nets.contains(&n) {
                        nets.push(n);
                    }
                }
            }
        }
    }
    nets
}

pub fn processes(opts: &SessionOpts) -> Vec<&'static str> {
    let s = opts.sanitized();
    let mut out = Vec::new();
    for id in &s.layouts {
        if let Some(l) = LAYOUTS.iter().find(|l| l.id == id) {
            for p in l.processes {
                if !out.contains(p) {
                    out.push(*p);
                }
            }
        }
    }
    out
}

/// Nets to install on the tunnel. `all` keeps AllowedIPs; otherwise drop `0.0.0.0/0`.
pub fn tunnel_nets(opts: &SessionOpts, allowed: &[IpNet]) -> Vec<IpNet> {
    let s = opts.sanitized();
    if s.layouts.iter().any(|id| id == "all") {
        return allowed.to_vec();
    }
    let mut nets = destinations(&s);
    for n in allowed {
        if n.prefix_len() != 0 && !nets.contains(n) {
            nets.push(*n);
        }
    }
    nets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses() {
        for l in LAYOUTS {
            for c in l.cidrs {
                c.parse::<IpNet>().expect(c);
            }
        }
    }

    #[test]
    fn caps_at_three_and_all_is_exclusive() {
        let o = SessionOpts {
            layouts: vec![
                "steam".into(),
                "discord".into(),
                "riot".into(),
                "ea".into(),
            ],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(o.layouts.len(), 3);
        let o = SessionOpts {
            layouts: vec!["steam".into(), "all".into()],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(o.layouts, vec!["all".to_string()]);
    }

    #[test]
    fn split_drops_default_route() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let extra: IpNet = "10.8.0.0/24".parse().unwrap();
        let opts = SessionOpts {
            layouts: vec!["steam".into(), "discord".into()],
            ..Default::default()
        };
        let nets = tunnel_nets(&opts, &[full, extra]);
        assert!(!nets.iter().any(|n| n.prefix_len() == 0));
        assert!(nets.contains(&extra));
    }

    #[test]
    fn all_keeps_allowed() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let opts = SessionOpts {
            layouts: vec!["all".into()],
            ..Default::default()
        };
        assert_eq!(tunnel_nets(&opts, &[full]), vec![full]);
    }
}
