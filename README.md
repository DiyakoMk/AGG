# AGG

Personal Windows gaming VPN. Rust core, phone-sized Tauri shell.

Tunnel: Wintun adapter `AGG` + UDP AmneziaWG. Official `wintun.dll` is bundled.

Daily use is **not** Administrator. The NSIS installer (per-machine) drops `agg-svc.exe` + `wintun.dll`, then `agg-svc.exe install` registers Windows service `AGGService` (LocalSystem). The UI talks to it over `\\.\pipe\AGGService`.

## Windows

Installer (one package):

```bash
cd ui
npm install
npm run tauri build
```

Output: `ui\src-tauri\target\release\bundle\nsis\`. Run the setup elevated once. After that, launch AGG normally.

Dev without the installer:

```bash
cargo build -p agg-cli --release
cargo build -p agg-svc --release
# once, elevated:
target\release\agg-svc.exe install

cd ui
npm install
npm run tauri dev
```

Phone-sized HUD, centered on the monitor. Tap **Boost**. Routes: drop or paste a `.conf`. Always-on-top lives in Settings.

Uninstall helper: `agg-svc.exe uninstall` (elevated), or uninstall AGG from Apps — the NSIS pre-uninstall hook stops and deletes the service.

## Linux (handshake / CLI only)

```bash
cargo test -p agg-core
cargo run -p agg-cli -- handshake --config awg.conf
```

Do not commit real `.conf` files.
