# Limitations

AGG is an AmneziaWG **client**, not a self-host installer. No SSH, Docker, OpenVPN, XRay, IKEv2, or Amnezia Premium.

## Split tunneling (Windows, Amnezia semantics)

**Sites (IPv4):**

- All traffic through VPN
- Only listed IPs through VPN
- Listed IPs bypass VPN (more-specific LAN routes)

**Apps:** selected executables work **without** VPN (Amnezia Windows exceptions). Opposite mode (only listed apps through VPN) is not on Windows Amnezia either.

## KillSwitch

Blocks the internet if the tunnel drops. Manual disconnect does not. Crash leftover: Settings → Off, or `agg-cli kill-switch off`.

## Protocol

AmneziaWG 2.0 configs (Jc/Jmin/Jmax, S1–S4, H1–H4). No I1–I5 / 3.x header protection.

## Vanguard

No process injection. Virtual NIC + firewall may still be flagged. Not evaded.
