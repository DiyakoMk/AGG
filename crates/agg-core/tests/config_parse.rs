use agg_core::config::{HeaderSpec, WgConfig};

const AWG2: &str = include_str!("fixtures/awg2.conf");

#[test]
fn parses_awg2_fixture() {
    let cfg = WgConfig::from_str(AWG2).expect("valid AWG 2.0 fixture");
    assert_eq!(cfg.interface.jc, Some(4));
    assert_eq!(cfg.interface.s4, Some(8));
    assert_eq!(
        cfg.interface.h1,
        Some(HeaderSpec::Range {
            start: 100_000,
            end: 200_000
        })
    );
    assert_eq!(
        cfg.interface.extra.get("X-Vendor").map(String::as_str),
        Some("keep-me")
    );
    assert_eq!(cfg.peers.len(), 1);
    assert!(cfg.peer().unwrap().endpoint.is_some());
}

#[test]
fn missing_obfuscation_is_plain_wireguard() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Address = 10.0.0.2/32
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Endpoint = 203.0.113.10:51820
AllowedIPs = 0.0.0.0/0
"#;
    let cfg = WgConfig::from_str(conf).unwrap();
    assert!(!cfg.interface.obfuscation_present());
}

#[test]
fn rejects_overlapping_headers() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
H1 = 10-50
H2 = 40-80
H3 = 100-200
H4 = 300-400
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let err = WgConfig::from_str(conf).unwrap_err().to_string();
    assert!(err.contains("overlaps"), "{err}");
}

#[test]
fn rejects_h_colliding_with_wireguard_types() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
H1 = 1
H2 = 2
H3 = 3
H4 = 4
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let err = WgConfig::from_str(conf).unwrap_err().to_string();
    assert!(err.contains("WireGuard type"), "{err}");
}

#[test]
fn rejects_jmin_gt_jmax() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Jc = 3
Jmin = 200
Jmax = 50
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let err = WgConfig::from_str(conf).unwrap_err().to_string();
    assert!(err.contains("Jmin"), "{err}");
}

#[test]
fn rejects_s1_plus_56_eq_s2() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
S1 = 10
S2 = 66
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let err = WgConfig::from_str(conf).unwrap_err().to_string();
    assert!(err.contains("S1 + 56"), "{err}");
}

#[test]
fn rejects_partial_headers() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
H1 = 100
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let err = WgConfig::from_str(conf).unwrap_err().to_string();
    assert!(err.contains("H1"), "{err}");
}

#[test]
fn secret_key_debug_is_redacted() {
    let cfg = WgConfig::from_str(AWG2).unwrap();
    let dumped = format!("{:?}", cfg.interface.private_key);
    assert!(dumped.contains("REDACTED"));
    assert!(!dumped.contains("YAAA"));
}

#[test]
fn round_trips_unknown_interface_keys() {
    let cfg = WgConfig::from_str(AWG2).unwrap();
    assert_eq!(cfg.interface.extra.len(), 1);
}

#[test]
fn engine_builds_from_awg2() {
    let cfg = WgConfig::from_str(AWG2).unwrap();
    agg_core::TunnelEngine::from_config(&cfg).expect("Tunn should accept AWG 2.0 params");
}

#[test]
fn single_h_value_parses() {
    let conf = r#"
[Interface]
PrivateKey = YAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
H1 = 100000
H2 = 200000
H3 = 300000
H4 = 400000
[Peer]
PublicKey = YgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
"#;
    let cfg = WgConfig::from_str(conf).unwrap();
    assert_eq!(cfg.interface.h1, Some(HeaderSpec::Single(100_000)));
}
