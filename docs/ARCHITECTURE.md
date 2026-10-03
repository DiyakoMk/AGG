# Architecture

Three layers. `agg-core` has no Tauri and no Windows APIs.

```
AGG UI (Tauri v2, React)
        │ commands + events
AGG Core (Rust, agg-core)
        │
Windows Wintun adapter AGG   — UDP AmneziaWG + route-based split
Linux TUN+UDP (agg-cli)      — handshake / CLI proof
```

## Crates

| Crate | Role |
|---|---|
| `agg-core` | Config, BoringTun, named-pipe IPC, destination-IP layouts |
| `agg-cli` | Headless harness |
| `agg-platform-windows` | Wintun `AGG` + Windows Firewall kill switch. Stub off-Windows. |
| `agg-svc` | Windows service. Owns Wintun; UI is unprivileged. |
| `agg-ui` | Tauri v2 shell (`ui/src-tauri`) |

## Data flow (Windows)

1. UDP handshake to the VPS (no NIC yet).
2. Create Wintun adapter `AGG`, set tunnel IPv4 + MTU + DNS.
3. Pin `/32` to the VPS via the LAN gateway.
4. Install layout prefixes on `AGG` (Steam, Riot, Discord, … — up to 3). `Everything` uses `0.0.0.0/1` + `128.0.0.0/1`.
5. Inner IP from Wintun → BoringTun encapsulate → UDP. Reverse on recv.
6. Stop / service stop deletes routes, firewall rules, and the adapter.

Daily connect goes UI → named pipe → `AGGService` (SYSTEM).

## Rules

- Windows tunnel NIC is Wintun `AGG`. Must be deleted on exit.
- Split is **route-based** (destination prefixes). No process injection, no NDIS, no WinpkFilter.
- Kill switch: named Windows Firewall rules, deleted on Drop / `down`.
- Auto-reconnect after handshake or UDP failure. Optional MTU sweep.
- Every routing / filter-mode change has a rollback
- Private keys never in `Display`/`Debug`/logs
