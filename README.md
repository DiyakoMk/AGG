# AGG

Personal Windows gaming VPN for two AmneziaWG 2.0 VPSes. Rust core, Tauri v2 shell.

Windows datapath: **Wintun adapter `AGG` + UDP AmneziaWG**. Official `wintun.dll` is **bundled** (prebuilt binaries license). NDIS inject is off (bugcheck). VAN 84 ignored per operator.

## Standalone Windows build (on your Windows box)

Needs: Rust (MSVC), Node 20+, WebView2 (comes with Win 10/11).

```bash
# CLI only — one folder, no extra DLL copy:
cargo build -p agg-cli --release
# target\release\agg-cli.exe
# first `up` writes wintun.dll next to the exe

# Desktop app + NSIS installer:
cd ui
npm install
npm run tauri build
# ui\src-tauri\target\release\agg-ui.exe
# ui\src-tauri\target\release\bundle\nsis\*.exe
```

Run **as Administrator**. Config path in the UI is relative to the process cwd (put `awg.conf` next to the exe, or use a full path).

```bash
agg-cli handshake --config awg.conf
agg-cli up --config awg.conf
# Ctrl-C deletes AGG. If it remains:
agg-cli down
```

## Phase 2 UI

Dashboard: drop / browse / paste as many `.conf` files as you want, rename, connect. Live RTT sparkline (last 60 s). No JS polling.

```bash
cd ui
npm install
npm run tauri dev
```

Do not commit real `.conf` files.

## Decisions (locked)

- Accent `#22D3EE` on `#0a0a0c`
- shadcn/ui, visx, tauri-specta (Phase 3+)
- Tunnel: wiresock-boringtun `@ ae2ab44e`, AWG 2.0
- Wintun NIC `AGG`; official DLL bundled
- Do not bundle WinpkFilter
- No code signing
