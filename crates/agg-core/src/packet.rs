//! Ethernet / IPv4 / UDP helpers. No Windows types.
//!
//! Used by the NDIS intercept path to NAT inner packets onto the tunnel
//! address and to wrap BoringTun datagrams in Ethernet+IP+UDP for injection.

use std::net::Ipv4Addr;

pub const ETH_LEN: usize = 14;
pub const ETH_TYPE_IPV4: u16 = 0x0800;
pub const ETH_TYPE_ARP: u16 = 0x0806;
pub const ETH_TYPE_IPV6: u16 = 0x86dd;
pub const IPPROTO_TCP: u8 = 6;
pub const IPPROTO_UDP: u8 = 17;
pub const IPPROTO_ICMP: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EthType(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearnedLan {
    pub our_mac: [u8; 6],
    pub gw_mac: [u8; 6],
    pub our_ip: Ipv4Addr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Udp4 {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub src_port: u16,
    pub dst_port: u16,
    pub payload_off: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Info {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub proto: u8,
    pub header_len: usize,
    pub total_len: usize,
}

pub fn ethertype(frame: &[u8]) -> Option<u16> {
    if frame.len() < ETH_LEN {
        return None;
    }
    Some(u16::from_be_bytes([frame[12], frame[13]]))
}

pub fn learn_lan_from_outbound_ipv4(frame: &[u8]) -> Option<LearnedLan> {
    if ethertype(frame)? != ETH_TYPE_IPV4 {
        return None;
    }
    let ip = ipv4_info(frame)?;
    let mut our_mac = [0u8; 6];
    let mut gw_mac = [0u8; 6];
    our_mac.copy_from_slice(&frame[6..12]);
    gw_mac.copy_from_slice(&frame[0..6]);
    Some(LearnedLan {
        our_mac,
        gw_mac,
        our_ip: ip.src,
    })
}

pub fn ipv4_info(frame: &[u8]) -> Option<Ipv4Info> {
    if ethertype(frame)? != ETH_TYPE_IPV4 {
        return None;
    }
    let ip = &frame[ETH_LEN..];
    if ip.len() < 20 {
        return None;
    }
    if ip[0] >> 4 != 4 {
        return None;
    }
    let ihl = ((ip[0] & 0x0f) as usize) * 4;
    if ihl < 20 || ip.len() < ihl {
        return None;
    }
    let total_len = u16::from_be_bytes([ip[2], ip[3]]) as usize;
    Some(Ipv4Info {
        src: Ipv4Addr::new(ip[12], ip[13], ip[14], ip[15]),
        dst: Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]),
        proto: ip[9],
        header_len: ihl,
        total_len,
    })
}

pub fn parse_udp4(frame: &[u8]) -> Option<Udp4> {
    let ip = ipv4_info(frame)?;
    if ip.proto != IPPROTO_UDP {
        return None;
    }
    let udp_off = ETH_LEN + ip.header_len;
    if frame.len() < udp_off + 8 {
        return None;
    }
    let udp = &frame[udp_off..];
    Some(Udp4 {
        src: ip.src,
        dst: ip.dst,
        src_port: u16::from_be_bytes([udp[0], udp[1]]),
        dst_port: u16::from_be_bytes([udp[2], udp[3]]),
        payload_off: udp_off + 8,
    })
}

/// True if this frame is UDP to (or from) the WireGuard peer endpoint.
pub fn is_peer_udp(frame: &[u8], peer_ip: Ipv4Addr, peer_port: u16) -> bool {
    match parse_udp4(frame) {
        Some(u) => {
            (u.dst == peer_ip && u.dst_port == peer_port)
                || (u.src == peer_ip && u.src_port == peer_port)
        }
        None => false,
    }
}

/// Rewrite IPv4 source (SNAT) and fix IP/L4 checksums. `frame` is Ethernet+.
pub fn snat_ipv4_source(frame: &mut [u8], new_src: Ipv4Addr) -> Option<()> {
    rewrite_ipv4_addr(frame, true, new_src)
}

/// Rewrite IPv4 destination (DNAT) and fix IP/L4 checksums.
pub fn dnat_ipv4_dest(frame: &mut [u8], new_dst: Ipv4Addr) -> Option<()> {
    rewrite_ipv4_addr(frame, false, new_dst)
}

fn rewrite_ipv4_addr(frame: &mut [u8], source: bool, new: Ipv4Addr) -> Option<()> {
    let info = ipv4_info(frame)?;
    let ip_off = ETH_LEN;
    let addr_off = ip_off + if source { 12 } else { 16 };
    let octets = new.octets();
    frame[addr_off..addr_off + 4].copy_from_slice(&octets);
    set_ipv4_checksum(frame, ip_off, info.header_len)?;
    match info.proto {
        IPPROTO_TCP => set_tcp_checksum(frame, ip_off, info.header_len),
        IPPROTO_UDP => set_udp_checksum(frame, ip_off, info.header_len),
        _ => Some(()),
    }
}

fn set_ipv4_checksum(frame: &mut [u8], ip_off: usize, ihl: usize) -> Option<()> {
    frame[ip_off + 10] = 0;
    frame[ip_off + 11] = 0;
    let sum = checksum(&frame[ip_off..ip_off + ihl]);
    let bytes = sum.to_be_bytes();
    frame[ip_off + 10] = bytes[0];
    frame[ip_off + 11] = bytes[1];
    Some(())
}

fn set_udp_checksum(frame: &mut [u8], ip_off: usize, ihl: usize) -> Option<()> {
    let udp_off = ip_off + ihl;
    if frame.len() < udp_off + 8 {
        return None;
    }
    let udp_len = u16::from_be_bytes([frame[udp_off + 4], frame[udp_off + 5]]) as usize;
    if udp_len < 8 || frame.len() < udp_off + udp_len {
        return None;
    }
    // Keep checksum 0 as "disabled" (valid for IPv4).
    if frame[udp_off + 6] == 0 && frame[udp_off + 7] == 0 {
        return Some(());
    }
    frame[udp_off + 6] = 0;
    frame[udp_off + 7] = 0;
    let sum = l4_checksum(frame, ip_off, ihl, udp_off, udp_len, IPPROTO_UDP);
    let bytes = sum.to_be_bytes();
    frame[udp_off + 6] = bytes[0];
    frame[udp_off + 7] = bytes[1];
    Some(())
}

fn set_tcp_checksum(frame: &mut [u8], ip_off: usize, ihl: usize) -> Option<()> {
    let tcp_off = ip_off + ihl;
    if frame.len() < tcp_off + 20 {
        return None;
    }
    let ip_total = u16::from_be_bytes([frame[ip_off + 2], frame[ip_off + 3]]) as usize;
    if ip_total < ihl + 20 {
        return None;
    }
    let tcp_len = ip_total - ihl;
    if frame.len() < tcp_off + tcp_len {
        return None;
    }
    frame[tcp_off + 16] = 0;
    frame[tcp_off + 17] = 0;
    let sum = l4_checksum(frame, ip_off, ihl, tcp_off, tcp_len, IPPROTO_TCP);
    let bytes = sum.to_be_bytes();
    frame[tcp_off + 16] = bytes[0];
    frame[tcp_off + 17] = bytes[1];
    Some(())
}

fn l4_checksum(
    frame: &[u8],
    ip_off: usize,
    ihl: usize,
    l4_off: usize,
    l4_len: usize,
    proto: u8,
) -> u16 {
    let mut sum: u32 = 0;
    // Pseudo-header: src, dst, zero, proto, length
    sum += u16::from_be_bytes([frame[ip_off + 12], frame[ip_off + 13]]) as u32;
    sum += u16::from_be_bytes([frame[ip_off + 14], frame[ip_off + 15]]) as u32;
    sum += u16::from_be_bytes([frame[ip_off + 16], frame[ip_off + 17]]) as u32;
    sum += u16::from_be_bytes([frame[ip_off + 18], frame[ip_off + 19]]) as u32;
    sum += proto as u32;
    sum += l4_len as u32;
    let _ = ihl;
    fold_checksum(sum + raw_sum(&frame[l4_off..l4_off + l4_len]))
}

fn raw_sum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut chunks = data.chunks_exact(2);
    for c in chunks.by_ref() {
        sum += u16::from_be_bytes([c[0], c[1]]) as u32;
    }
    if let Some(&b) = chunks.remainder().first() {
        sum += (b as u32) << 8;
    }
    sum
}

fn checksum(data: &[u8]) -> u16 {
    fold_checksum(raw_sum(data))
}

fn fold_checksum(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Build Ethernet+IPv4+UDP carrying `payload` (the BoringTun datagram).
pub fn build_udp4_frame(
    lan: &LearnedLan,
    src_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16,
    payload: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let ip_len = 20 + 8 + payload.len();
    let total = ETH_LEN + ip_len;
    if out.len() < total || ip_len > 65535 {
        return None;
    }
    out[..total].fill(0);
    // Ethernet
    out[0..6].copy_from_slice(&lan.gw_mac);
    out[6..12].copy_from_slice(&lan.our_mac);
    out[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());
    // IPv4
    let ip = ETH_LEN;
    out[ip] = 0x45;
    out[ip + 1] = 0;
    out[ip + 2..ip + 4].copy_from_slice(&(ip_len as u16).to_be_bytes());
    out[ip + 4..ip + 6].copy_from_slice(&0u16.to_be_bytes()); // id
    out[ip + 6..ip + 8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF
    out[ip + 8] = 64;
    out[ip + 9] = IPPROTO_UDP;
    out[ip + 12..ip + 16].copy_from_slice(&lan.our_ip.octets());
    out[ip + 16..ip + 20].copy_from_slice(&dst_ip.octets());
    set_ipv4_checksum(out, ip, 20)?;
    // UDP
    let udp = ip + 20;
    out[udp..udp + 2].copy_from_slice(&src_port.to_be_bytes());
    out[udp + 2..udp + 4].copy_from_slice(&dst_port.to_be_bytes());
    let ulen = (8 + payload.len()) as u16;
    out[udp + 4..udp + 6].copy_from_slice(&ulen.to_be_bytes());
    out[udp + 8..udp + 8 + payload.len()].copy_from_slice(payload);
    // force UDP checksum compute (nonzero)
    out[udp + 6] = 0xff;
    out[udp + 7] = 0xff;
    set_udp_checksum(out, ip, 20)?;
    Some(total)
}

/// Wrap an inner IPv4 packet in Ethernet for delivery to the local stack.
pub fn wrap_ipv4_for_stack(lan: &LearnedLan, ipv4: &[u8], out: &mut [u8]) -> Option<usize> {
    let total = ETH_LEN + ipv4.len();
    if out.len() < total {
        return None;
    }
    out[0..6].copy_from_slice(&lan.our_mac);
    out[6..12].copy_from_slice(&lan.gw_mac);
    out[12..14].copy_from_slice(&ETH_TYPE_IPV4.to_be_bytes());
    out[ETH_LEN..total].copy_from_slice(ipv4);
    Some(total)
}

pub fn ipv4_payload<'a>(frame: &'a [u8]) -> Option<&'a [u8]> {
    if ethertype(frame)? != ETH_TYPE_IPV4 {
        return None;
    }
    let info = ipv4_info(frame)?;
    let end = ETH_LEN + info.total_len.min(frame.len() - ETH_LEN);
    Some(&frame[ETH_LEN..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_udp_frame() -> Vec<u8> {
        let lan = LearnedLan {
            our_mac: [0x02, 0, 0, 0, 0, 1],
            gw_mac: [0x02, 0, 0, 0, 0, 2],
            our_ip: Ipv4Addr::new(192, 168, 1, 10),
        };
        let mut buf = [0u8; 128];
        let n = build_udp4_frame(
            &lan,
            51820,
            Ipv4Addr::new(31, 97, 144, 105),
            443,
            b"hello",
            &mut buf,
        )
        .unwrap();
        buf[..n].to_vec()
    }

    #[test]
    fn builds_and_parses_udp() {
        let f = sample_udp_frame();
        let u = parse_udp4(&f).unwrap();
        assert_eq!(u.src_port, 51820);
        assert_eq!(u.dst_port, 443);
        assert_eq!(&f[u.payload_off..], b"hello");
        assert!(is_peer_udp(&f, Ipv4Addr::new(31, 97, 144, 105), 443));
    }

    #[test]
    fn snat_changes_source() {
        let mut f = sample_udp_frame();
        snat_ipv4_source(&mut f, Ipv4Addr::new(10, 10, 0, 2)).unwrap();
        let ip = ipv4_info(&f).unwrap();
        assert_eq!(ip.src, Ipv4Addr::new(10, 10, 0, 2));
        assert_eq!(ip.dst, Ipv4Addr::new(31, 97, 144, 105));
    }

    #[test]
    fn learn_lan() {
        let f = sample_udp_frame();
        let lan = learn_lan_from_outbound_ipv4(&f).unwrap();
        assert_eq!(lan.our_ip, Ipv4Addr::new(192, 168, 1, 10));
        assert_eq!(lan.our_mac[5], 1);
        assert_eq!(lan.gw_mac[5], 2);
    }

    #[test]
    fn ipv4_checksum_sane() {
        let f = sample_udp_frame();
        let ip = &f[ETH_LEN..ETH_LEN + 20];
        // verified checksum: folding the header including checksum field yields 0
        let mut sum = 0u32;
        for c in ip.chunks(2) {
            sum += u16::from_be_bytes([c[0], c[1]]) as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        assert_eq!(sum as u16, 0xffff);
    }
}
