//! Linux TUN. Windows path is Phase 1 (`agg-platform-windows` + ndisapi).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{bail, Context, Result};
use ipnet::IpNet;
use libc::{c_char, c_short, ioctl, IFF_NO_PI, IFF_TUN, IFNAMSIZ};

const TUNSETIFF: u64 = 0x4004_54ca;

#[repr(C)]
struct IfReq {
    name: [c_char; IFNAMSIZ],
    flags: c_short,
    _pad: [u8; 16],
}

pub struct TunDevice {
    file: File,
    name: String,
}

impl TunDevice {
    pub fn open(name: &str, addr: IpNet, mtu: u16) -> Result<Self> {
        if cfg!(not(target_os = "linux")) {
            bail!("Phase 0 TUN is Linux-only; Windows is Phase 1");
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/net/tun")
            .context("open /dev/net/tun")?;

        let mut req = IfReq {
            name: [0; IFNAMSIZ],
            flags: (IFF_TUN | IFF_NO_PI) as c_short,
            _pad: [0; 16],
        };
        let bytes = name.as_bytes();
        if bytes.len() >= IFNAMSIZ {
            bail!("interface name too long");
        }
        for (i, b) in bytes.iter().enumerate() {
            req.name[i] = *b as c_char;
        }

        let rc = unsafe { ioctl(file.as_raw_fd(), TUNSETIFF, &mut req) };
        if rc < 0 {
            return Err(io::Error::last_os_error()).context("TUNSETIFF");
        }

        let tun_name = {
            let end = req
                .name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(IFNAMSIZ);
            let raw = &req.name[..end];
            let bytes: Vec<u8> = raw.iter().map(|c| *c as u8).collect();
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let ip = match addr.addr() {
            IpAddr::V4(v) => v.to_string(),
            IpAddr::V6(_) => bail!("Phase 0 TUN is IPv4-only"),
        };
        let plen = addr.prefix_len();
        run(&["ip", "link", "set", &tun_name, "mtu", &mtu.to_string(), "up"])?;
        run(&[
            "ip",
            "addr",
            "add",
            &format!("{ip}/{plen}"),
            "dev",
            &tun_name,
        ])?;

        Ok(Self {
            file,
            name: tun_name,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_nonblocking(&self, nb: bool) -> Result<()> {
        let fd = self.file.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags < 0 {
            return Err(io::Error::last_os_error()).context("F_GETFL");
        }
        let new = if nb {
            flags | libc::O_NONBLOCK
        } else {
            flags & !libc::O_NONBLOCK
        };
        let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, new) };
        if rc < 0 {
            return Err(io::Error::last_os_error()).context("F_SETFL");
        }
        Ok(())
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut f = unsafe { File::from_raw_fd(self.file.as_raw_fd()) };
        let n = f.read(buf);
        let _ = f.into_raw_fd();
        n
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut f = unsafe { File::from_raw_fd(self.file.as_raw_fd()) };
        let n = f.write(buf);
        let _ = f.into_raw_fd();
        n
    }
}

impl Drop for TunDevice {
    fn drop(&mut self) {
        let _ = std::process::Command::new("ip")
            .args(["link", "del", &self.name])
            .status();
    }
}

fn run(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new(args[0])
        .args(&args[1..])
        .status()
        .with_context(|| format!("spawn {}", args[0]))?;
    if !status.success() {
        bail!("{} failed: {status}", args.join(" "));
    }
    Ok(())
}
