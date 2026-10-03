//! Firewall rules. Kill switch + DIRECT-app block from the tunnel address.
//! BOOSTED apps keep the default route (Wintun). Named rules, deleted on Drop.

use std::net::{Ipv4Addr, SocketAddr};

use super::PlatformError;

const ALLOW_VPS: &str = "AGG-ks-vps";
const ALLOW_TUN: &str = "AGG-ks-tun";
const BLOCK: &str = "AGG-ks-block";
const DIRECT_PREFIX: &str = "AGG-direct-";

pub struct KillSwitch {
    armed: bool,
}

impl KillSwitch {
    pub fn engage(endpoint: SocketAddr, tun_ip: Ipv4Addr) -> Result<Self, PlatformError> {
        let vps = match endpoint.ip() {
            std::net::IpAddr::V4(v) => v,
            _ => return Err(PlatformError::msg("kill switch needs IPv4 endpoint")),
        };
        fw(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={ALLOW_VPS}"),
            "dir=out",
            "action=allow",
            &format!("remoteip={vps}"),
            "protocol=any",
            "enable=yes",
        ])?;
        fw(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={ALLOW_TUN}"),
            "dir=out",
            "action=allow",
            &format!("localip={tun_ip}"),
            "enable=yes",
        ])?;
        fw(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={BLOCK}"),
            "dir=out",
            "action=block",
            "enable=yes",
        ])?;
        Ok(Self { armed: true })
    }

    fn disarm(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        for name in [BLOCK, ALLOW_TUN, ALLOW_VPS] {
            let _ = fw(&[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                &format!("name={name}"),
            ]);
        }
    }

    pub fn disarm_leftovers() {
        let mut k = Self { armed: true };
        k.disarm();
        SplitRules::disarm_leftovers();
    }
}

impl Drop for KillSwitch {
    fn drop(&mut self) {
        self.disarm();
    }
}

/// Block DIRECT apps from using the tunnel IPv4 so they stay on LAN.
pub struct SplitRules {
    names: Vec<String>,
}

impl SplitRules {
    pub fn engage(tun_ip: Ipv4Addr, exes: &[String]) -> Result<Self, PlatformError> {
        Self::disarm_leftovers();
        let mut names = Vec::new();
        for (i, exe) in exes.iter().enumerate() {
            let p = std::path::Path::new(exe);
            if !p.exists() {
                tracing::info!("skip DIRECT (missing) {exe}");
                continue;
            }
            let name = format!("{DIRECT_PREFIX}{i}");
            if let Err(e) = fw(&[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &format!("name={name}"),
                "dir=out",
                "action=block",
                &format!("localip={tun_ip}"),
                &format!("program={}", p.display()),
                "enable=yes",
            ]) {
                tracing::warn!("DIRECT rule {exe}: {e}");
                continue;
            }
            names.push(name);
        }
        Ok(Self { names })
    }

    pub fn disarm_leftovers() {
        for i in 0..64 {
            let _ = fw(&[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                &format!("name={DIRECT_PREFIX}{i}"),
            ]);
        }
    }
}

impl Drop for SplitRules {
    fn drop(&mut self) {
        for name in &self.names {
            let _ = fw(&[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                &format!("name={name}"),
            ]);
        }
        self.names.clear();
    }
}

fn fw(args: &[&str]) -> Result<(), PlatformError> {
    let out = std::process::Command::new("netsh")
        .args(args)
        .output()
        .map_err(|e| PlatformError::msg(format!("netsh: {e}")))?;
    if !out.status.success() {
        return Err(PlatformError::msg(format!(
            "netsh {} : {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}
