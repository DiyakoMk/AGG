# Limitations

## Split (BOOSTED / DIRECT)

Default route is the Wintun adapter. **BOOSTED** apps use that route. **DIRECT** apps are blocked from the tunnel IPv4 (`AGG-direct-*` firewall rules) so Windows uses the LAN default instead.

If nothing is BOOSTED, the tunnel is full (no DIRECT rules).

No process injection. No homemade NDIS inject (bugcheck). WinpkFilter is **not** bundled.

## Kill switch

Named firewall rules: allow VPS + tunnel IP, block other outbound. Deleted on Stop / `down` / Drop.

If AGG dies with kill switch on, you may have no internet until:

- Settings → **Clear**, or
- `agg-cli kill-switch off` (elevated), or
- reboot

## Packet filter

`ndisapi` detects NDISRD only. Install yourself if you want it: https://github.com/wiresock/ndisapi/releases

## Vanguard

No injection. Virtual NIC + firewall rules may still be flagged (VAN 84). Documented, not evaded.

## Scanner

Paths come from environment variables and launcher files, not a hardcoded `C:\`. Missing folders skipped. Add leftover games with **Add**.
