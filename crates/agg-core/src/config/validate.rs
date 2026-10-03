use super::types::{HeaderSpec, WgConfig, WG_TYPE_COOKIE, WG_TYPE_DATA, WG_TYPE_INIT, WG_TYPE_RESPONSE};
use crate::error::AggError;

/// AWG 2.0 ranges we accept. Missing obfuscation = plain WireGuard (valid).
const JC_MAX: u16 = 128;
const J_MAX: u16 = 1280;
const S_MAX: u16 = 1280;

pub fn validate(cfg: &WgConfig) -> Result<(), AggError> {
    let iface = &cfg.interface;

    if let Some(jc) = iface.jc {
        if jc > JC_MAX {
            return Err(AggError::config(format!("Jc = {jc} exceeds {JC_MAX}")));
        }
    }

    match (iface.jmin, iface.jmax) {
        (Some(jmin), Some(jmax)) => {
            if jmin > jmax {
                return Err(AggError::config(format!(
                    "Jmin ({jmin}) > Jmax ({jmax})"
                )));
            }
            if jmax > J_MAX {
                return Err(AggError::config(format!("Jmax = {jmax} exceeds {J_MAX}")));
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(AggError::config("Jmin and Jmax must both be set, or neither"));
        }
        (None, None) => {}
    }

    for (name, val) in [
        ("S1", iface.s1),
        ("S2", iface.s2),
        ("S3", iface.s3),
        ("S4", iface.s4),
    ] {
        if let Some(v) = val {
            if v > S_MAX {
                return Err(AggError::config(format!("{name} = {v} exceeds {S_MAX}")));
            }
        }
    }

    if let (Some(s1), Some(s2)) = (iface.s1, iface.s2) {
        // Handshake init is 148 bytes, response 92. S1+56 == S2 makes them the same size.
        if (s1 as u32).saturating_add(56) == s2 as u32 {
            return Err(AggError::config(format!(
                "S1 + 56 == S2 ({s2}): initiation and response would have the same length"
            )));
        }
    }

    let h_set = [iface.h1.is_some(), iface.h2.is_some(), iface.h3.is_some(), iface.h4.is_some()];
    let h_count = h_set.iter().filter(|b| **b).count();
    if h_count != 0 && h_count != 4 {
        return Err(AggError::config(
            "H1–H4 must all be set, or none (partial headers are rejected)",
        ));
    }

    if let (Some(h1), Some(h2), Some(h3), Some(h4)) = (iface.h1, iface.h2, iface.h3, iface.h4) {
        validate_headers([h1, h2, h3, h4])?;
    }

    if cfg.peers.is_empty() {
        return Err(AggError::config("config has no [Peer] section"));
    }

    Ok(())
}

fn validate_headers(hs: [HeaderSpec; 4]) -> Result<(), AggError> {
    let names = ["H1", "H2", "H3", "H4"];
    for i in 0..4 {
        for j in (i + 1)..4 {
            if hs[i].overlaps(hs[j]) {
                return Err(AggError::config(format!(
                    "{} ({}) overlaps {} ({})",
                    names[i], hs[i], names[j], hs[j]
                )));
            }
        }
    }

    // Collision with vanilla WireGuard types 1–4 (brief). A range that covers any of
    // 1..=4 is rejected so we never emit a packet that looks like stock WireGuard.
    let wg = [
        ("H1/init", WG_TYPE_INIT),
        ("H2/response", WG_TYPE_RESPONSE),
        ("H3/cookie", WG_TYPE_COOKIE),
        ("H4/data", WG_TYPE_DATA),
    ];
    for (i, h) in hs.iter().enumerate() {
        for (label, t) in wg {
            if h.contains(t) && names[i] != header_for_wg_type(t) {
                return Err(AggError::config(format!(
                    "{} ({}) collides with WireGuard type {t} ({label})",
                    names[i], h
                )));
            }
        }
        // Even matching the "right" WG type is a collision if the operator meant to
        // obfuscate: types 1–4 on the wire are the DPI fingerprint. Reject any
        // header that includes 1..=4.
        for t in 1u32..=4 {
            if h.contains(t) {
                return Err(AggError::config(format!(
                    "{} ({}) includes WireGuard type {t}; obfuscated headers must not",
                    names[i], h
                )));
            }
        }
    }
    Ok(())
}

fn header_for_wg_type(t: u32) -> &'static str {
    match t {
        1 => "H1",
        2 => "H2",
        3 => "H3",
        4 => "H4",
        _ => "",
    }
}
