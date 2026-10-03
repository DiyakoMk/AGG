# Cleanup (Windows leftovers)

**Do not run removal commands from this document.** Inventory only.

**AGG Wintun adapter:** friendly name `AGG`. Created after handshake. Must disappear after Ctrl-C, UI Stop, or `agg-cli down`. If it remains, `agg-cli down` (admin) or `agg-svc` stop.

Kill-switch leftovers are named Windows Firewall rules `AGG-ks-vps`, `AGG-ks-tun`, `AGG-ks-block`. `wintun_down` deletes them.
