# Architecture

Three layers. `agg-core` has no Tauri and no Windows APIs.

```
AGG UI (Tauri v2, React)     — Phase 2+
        │ commands + events
AGG Core (Rust, agg-core)    — Phase 0
        │ platform shim
Windows Wintun adapter AGG   — working tunnel (UDP AmneziaWG)
Windows NDIS                 — detect/listen only; no inject
Linux TUN+UDP (agg-cli)      — Phase 0 proof
```

## Crates

| Crate | Role |
|---|---|
| `agg-core` | Config, BoringTun, Ethernet intercept state machine, named-pipe IPC types |
| `agg-cli` | Headless harness |
| `agg-platform-windows` | Wintun `AGG` + optional WinpkFilter detect. Stub off-Windows. |
| `agg-svc` | Windows service. Owns Wintun; UI is unprivileged. |
| `agg-ui` | Tauri v2 shell (`ui/src-tauri`) |

## Data flow (Phase 0)

1. Parse `.conf` → `WgConfig`
2. Build `AmneziaConfig` + `ObfuscationRanges` → `Tunn`
3. Open UDP socket to peer endpoint, open TUN, apply routes (saved first)
4. Encapsulate TUN → UDP, decapsulate UDP → TUN
5. Drop/SIGINT/`agg-cli down` restores the saved route table and deletes the TUN

## Data flow (Phase 1, Windows)

1. UDP handshake to the VPS (no NIC yet).
2. Create Wintun adapter `AGG`, set tunnel IPv4 + MTU + DNS.
3. Pin `/32` to the VPS via the LAN gateway; `0.0.0.0/1` + `128.0.0.0/1` via `AGG`.
4. Inner IP from Wintun → BoringTun encapsulate → UDP. Reverse on recv.
5. Ctrl-C / `down` / service stop deletes routes and the adapter.

Daily connect goes UI → named pipe → `AGGService` (SYSTEM). The UI process does not create the adapter.

NDIS `send_packet*` is not used. Homemade `IntermediateBuffer`s bugcheck WinpkFilter.

## Rules

- Windows tunnel NIC is Wintun `AGG`. Must be deleted on exit.
- No game-process injection. Split is destination-IP layouts (up to 3: Steam, Riot, Battle.net, EA, Ubisoft, Discord, or Everything).
- Kill switch: named Windows Firewall rules, deleted on Drop / `down`.
- Auto-reconnect after handshake or UDP failure. Optional MTU sweep.
- Every routing / filter-mode change has a rollback
- Private keys never in `Display`/`Debug`/logs
