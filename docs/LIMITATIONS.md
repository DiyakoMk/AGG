# Limitations

## Per-process split

The Apps view records which games and Discord you want boosted. **Boost is still a full tunnel.** Per-process redirect needs a packet filter in the kernel. AGG does not ship WinpkFilter / NDISRD (`ndisapi`). Homemade NDIS inject bugchecks Windows.

If you later install Windows Packet Filter yourself from [wiresock/ndisapi releases](https://github.com/wiresock/ndisapi/releases), that is your copy — AGG still does not bundle or call it.

## Vanguard / anti-cheat

No process injection, hooks, or memory reads. The Wintun adapter `AGG` is a normal L3 NIC. Some anti-cheat (including Vanguard VAN 84) may still flag a virtual adapter. Documented, not evaded.

## Scanner

Detection is read-only. Missing library folders are skipped. Steam tools/soundtracks are skipped. Battle.net paths are scraped from Agent data files and can miss titles. Add those with **Add** (browse for an `.exe`).
