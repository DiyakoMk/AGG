# AGG

Windows AmneziaWG client. Import a `.conf`, Connect, optional split tunneling and KillSwitch. No self-host.

```bash
cd ui
npm install
npm run tauri build
```

Installer: `ui\src-tauri\target\release\bundle\nsis\`. One elevated setup; daily use is not Administrator.

Dev: `agg-svc.exe install` once, then `npm run tauri dev`.
