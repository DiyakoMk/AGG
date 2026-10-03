# Architecture

AmneziaWG Windows client. No self-host.

```
UI (Tauri)  →  named pipe  →  AGGService (SYSTEM)
                                  Wintun AGG + UDP AmneziaWG 2.0
```

| Crate | Role |
|---|---|
| `agg-core` | Config parse, BoringTun, split policy, IPC |
| `agg-cli` | Handshake / up / down / kill-switch off |
| `agg-platform-windows` | Wintun + firewall KillSwitch + app bypass |
| `agg-svc` | Privileged service |
| `agg-ui` | Home / Servers / Split / Settings |

## Connect

1. Handshake to the VPS (no NIC yet).
2. Create Wintun `AGG`.
3. Pin VPS `/32` via LAN gateway.
4. Sites: AllowedIPs, or listed IPs only, or extra LAN routes for listed bypass.
5. Apps without VPN: firewall block from tunnel IPv4.
6. Stop deletes adapter, routes, firewall rules.

## Out of scope

Self-host, SSH, Docker, OpenVPN, XRay, IKEv2, Premium, AWG 3.x, WinpkFilter, game scanner.
