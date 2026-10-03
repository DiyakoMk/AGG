# Licenses

Personal / non-commercial use.

| Component | License | Notes |
|---|---|---|
| [wiresock-boringtun](https://github.com/Wiresock-Foundation/wiresock-boringtun) (pinned `ae2ab44e9a68ca1a3232d2e8b13f9db30a9b9dcf`) | BSD-3-Clause | Fork of Cloudflare BoringTun. AmneziaWG + WireSock extensions. **Not** crates.io `boringtun`. |
| Cloudflare BoringTun (upstream) | BSD-3-Clause | Copyright retained in fork files |
| `wintun` crate | MIT | Bindings. |
| Official `wintun.dll` 0.14.1 ([wintun.net](https://www.wintun.net/)) | Prebuilt Binaries License | **Bundled** under §3(d). License text in `third_party/wintun/LICENSE.txt`. |
| `ndisapi` 0.7 | Apache-2.0 / MIT | User-mode bindings only. |
| Windows Packet Filter (WinpkFilter / NDISRD) | Proprietary | Personal / non-commercial. **Not bundled.** User installs from [wiresock/ndisapi releases](https://github.com/wiresock/ndisapi/releases). |
| `indexmap`, `ipnet`, `thiserror`, `base64`, `clap`, `tracing`, `windows-service` | MIT / Apache-2.0 | See `Cargo.lock` |

No GPL / AGPL / SSPL source in this tree.

WireGuard is a registered trademark of Jason A. Donenfeld. AmneziaWG is a project of AmneziaVPN. WireSock BoringTun is not affiliated with Cloudflare or AmneziaVPN.
