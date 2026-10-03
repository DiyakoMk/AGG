//! Kill switch. Blocks outbound IPv4 except the VPS and the tunnel address.
//! Rules are named and deleted on Drop so a crash still has `wintun_down`.

use std::net::{Ipv4Addr, SocketAddr};

use super::PlatformError;

const ALLOW_VPS: &str = "AGG-ks-vps";
const ALLOW_TUN: &str = "AGG-ks-tun";
const BLOCK: &str = "AGG-ks-block";

pub struct KillSwitch {
    armed: bool,
}

impl KillSwitch {
    pub fn engage(endpoint: SocketAddr, tun_ip: Ipv4Addr) -> Result<Self, PlatformError> {
        let vps = match endpoint.ip() {
            std::net::IpAddr::V4(v) => v,
            _ => return Err(PlatformError::msg("kill switch needs IPv4 endpoint")),
        };
        // Newest matching rule wins; add allow first, then block.
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
    }
}

impl Drop for KillSwitch {
    fn drop(&mut self) {
        self.disarm();
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
