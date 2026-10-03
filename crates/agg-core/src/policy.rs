//! Destination-IP split. No process injection.
//!
//! `All` uses the config AllowedIPs (usually `0.0.0.0/0`).
//! Games / Launchers / Both replace a default route with a built-in prefix list.

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    #[default]
    All,
    Games,
    Launchers,
    Both,
}

impl SplitMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All traffic",
            Self::Games => "Games",
            Self::Launchers => "Launchers",
            Self::Both => "Games + launchers",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::All => "Everything goes through the boost",
            Self::Games => "Match servers only — chat and browser stay direct",
            Self::Launchers => "Steam, Battle.net, Riot, EA, Ubisoft",
            Self::Both => "Games and their launchers; the rest stays direct",
        }
    }
}

/// Valve / Riot game-server space (CS, Dota, TF2, League, Valorant).
const GAMES: &[&str] = &[
    "103.10.124.0/23",
    "146.66.152.0/21",
    "155.133.224.0/19",
    "162.254.192.0/21",
    "185.25.180.0/22",
    "208.64.200.0/22",
    "208.78.164.0/22",
    "104.160.128.0/19",
    "192.64.168.0/21",
];

/// Launcher / content CDNs that overlap Valve plus Blizzard, EA, Ubisoft.
const LAUNCHERS: &[&str] = &[
    "103.10.124.0/23",
    "146.66.152.0/21",
    "155.133.224.0/19",
    "162.254.192.0/21",
    "185.25.180.0/22",
    "208.64.200.0/22",
    "208.78.164.0/22",
    "24.105.0.0/18",
    "137.221.64.0/18",
    "5.42.160.0/19",
    "8.25.96.0/23",
    "159.153.0.0/16",
    "216.98.48.0/20",
    "104.160.128.0/19",
];

pub fn destinations(mode: SplitMode) -> Vec<IpNet> {
    match mode {
        SplitMode::All => Vec::new(),
        SplitMode::Games => parse_all(GAMES),
        SplitMode::Launchers => parse_all(LAUNCHERS),
        SplitMode::Both => union(parse_all(GAMES), parse_all(LAUNCHERS)),
    }
}

/// Nets to install on the tunnel. `All` keeps AllowedIPs; split drops `0.0.0.0/0`.
pub fn tunnel_nets(mode: SplitMode, allowed: &[IpNet]) -> Vec<IpNet> {
    match mode {
        SplitMode::All => allowed.to_vec(),
        other => {
            let mut nets = destinations(other);
            for n in allowed {
                if n.prefix_len() != 0 && !nets.contains(n) {
                    nets.push(*n);
                }
            }
            nets
        }
    }
}

fn parse_all(cidrs: &[&str]) -> Vec<IpNet> {
    cidrs
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn union(mut a: Vec<IpNet>, b: Vec<IpNet>) -> Vec<IpNet> {
    for n in b {
        if !a.contains(&n) {
            a.push(n);
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_parse() {
        assert_eq!(parse_all(GAMES).len(), GAMES.len());
        assert_eq!(parse_all(LAUNCHERS).len(), LAUNCHERS.len());
    }

    #[test]
    fn both_is_union() {
        let both = destinations(SplitMode::Both);
        for n in destinations(SplitMode::Games) {
            assert!(both.contains(&n), "missing game {n}");
        }
        for n in destinations(SplitMode::Launchers) {
            assert!(both.contains(&n), "missing launcher {n}");
        }
    }

    #[test]
    fn split_drops_default_route() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let extra: IpNet = "10.8.0.0/24".parse().unwrap();
        let nets = tunnel_nets(SplitMode::Games, &[full, extra]);
        assert!(!nets.iter().any(|n| n.prefix_len() == 0));
        assert!(nets.contains(&extra));
    }

    #[test]
    fn all_keeps_allowed() {
        let full: IpNet = "0.0.0.0/0".parse().unwrap();
        let nets = tunnel_nets(SplitMode::All, &[full]);
        assert_eq!(nets, vec![full]);
    }
}
