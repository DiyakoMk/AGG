//! Snapshot the IPv4 routes we add, restore them without flushing the table.
//!
//! Never `ip route flush`. A failed restore must not blackhole the box.

use std::net::IpAddr;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use ipnet::IpNet;

const DEV: &str = "agg0";

pub struct RouteGuard {
    /// `ip route` argv tails we applied, in reverse-delete order.
    added: Vec<Vec<String>>,
    applied: bool,
}

impl RouteGuard {
    pub fn capture() -> Result<Self> {
        Ok(Self {
            added: Vec::new(),
            applied: false,
        })
    }

    pub fn apply_tunnel_routes(&mut self, endpoint: IpAddr, allowed: &[IpNet]) -> Result<()> {
        let gw = default_via()?;
        self.replace(&[&endpoint.to_string(), "via", &gw])?;
        for net in allowed {
            if net.prefix_len() == 0 {
                self.replace(&["0.0.0.0/1", "dev", DEV])?;
                self.replace(&["128.0.0.0/1", "dev", DEV])?;
            } else {
                self.replace(&[&net.to_string(), "dev", DEV])?;
            }
        }
        self.applied = true;
        Ok(())
    }

    fn replace(&mut self, spec: &[&str]) -> Result<()> {
        let mut args = vec!["route", "replace"];
        args.extend(spec.iter().copied());
        run_ip(&args)?;
        self.added
            .push(spec.iter().map(|s| (*s).to_string()).collect());
        Ok(())
    }

    pub fn persist(&self, path: &Path) -> Result<()> {
        let body = self
            .added
            .iter()
            .map(|s| s.join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(path, body).with_context(|| format!("write {}", path.display()))
    }

    pub fn restore(&mut self) -> Result<()> {
        if !self.applied {
            return Ok(());
        }
        delete_specs(&self.added);
        self.applied = false;
        Ok(())
    }

    pub fn restore_from_file(path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let specs: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split_whitespace().map(str::to_string).collect())
            .collect();
        delete_specs(&specs);
        Ok(())
    }
}

impl Drop for RouteGuard {
    fn drop(&mut self) {
        if self.applied {
            if let Err(e) = self.restore() {
                tracing::error!("route restore failed: {e:#}");
            }
        }
    }
}

fn delete_specs(added: &[Vec<String>]) {
    for spec in added.iter().rev() {
        let mut args = vec!["route".to_string(), "del".to_string()];
        args.extend(spec.iter().cloned());
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(e) = run_ip(&args_ref) {
            tracing::warn!("ip {} failed: {e:#}", args.join(" "));
        }
    }
}

fn default_via() -> Result<String> {
    let out = ip_output(&["-4", "route", "show", "default"])?;
    let text = String::from_utf8_lossy(&out);
    let mut words = text.split_whitespace();
    while let Some(w) = words.next() {
        if w == "via" {
            return words
                .next()
                .map(|s| s.to_string())
                .context("default route has no via");
        }
    }
    bail!("no default IPv4 route; cannot pin VPS endpoint");
}

fn ip_output(args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("ip")
        .args(args)
        .output()
        .with_context(|| format!("ip {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "ip {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out.stdout)
}

fn run_ip(args: &[&str]) -> Result<()> {
    let status = Command::new("ip")
        .args(args)
        .status()
        .with_context(|| format!("ip {}", args.join(" ")))?;
    if !status.success() {
        bail!("ip {} failed: {status}", args.join(" "));
    }
    Ok(())
}
