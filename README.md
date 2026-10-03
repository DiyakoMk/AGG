# AGG

Personal Windows gaming VPN. Rust core, phone-sized Tauri shell.

Tunnel: Wintun adapter `AGG` + UDP AmneziaWG. Official `wintun.dll` is bundled. Creating that adapter needs elevation **once** — after `agg-svc.exe install`, the app itself is not Administrator.

## Windows

```bash
cargo build -p agg-cli --release
cargo build -p agg-svc --release
# once, elevated:
target\release\agg-svc.exe install

cd ui
npm install
npm run tauri dev
```

The window is phone-sized. Tap **go**. Locations sheet: files / paste as many `.conf` as you want.

Uninstall helper: `agg-svc.exe uninstall` (elevated).

Standalone UI: `npm run tauri build` → `ui\src-tauri\target\release\bundle\nsis\`.

## Linux (handshake / CLI only)

```bash
cargo test -p agg-core
cargo run -p agg-cli -- handshake --config awg.conf
```

Do not commit real `.conf` files.
