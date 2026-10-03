# Limitations

## Per-process boost

Apps you mark **BOOSTED** are the ones that should go through AGG. That redirect uses Windows Packet Filter (NDISRD) via the `ndisapi` crate (MIT/Apache).

**The driver is not in the AGG installer.** Personal / non-commercial use of WinpkFilter is allowed; redistribution is not. Install the runtime yourself:

https://github.com/wiresock/ndisapi/releases

Settings shows **Install** when NDISRD is missing. Homemade packet inject (forged `IntermediateBuffer`) is not used — it bugchecks Windows.

Until the driver is present, Boost is a full tunnel (config AllowedIPs).

## Vanguard / anti-cheat

No process injection, hooks, or memory reads. Wintun `AGG` is a normal L3 NIC. Vanguard may still flag a virtual adapter or a third-party NDIS filter (VAN 84). Documented, not evaded.

## Scanner

Read-only. Missing folders are skipped. Steam tools/soundtracks skipped. Battle.net paths are scraped and can miss titles — add those with **Add**.
