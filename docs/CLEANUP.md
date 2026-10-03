# Cleanup (Windows leftovers)

**Do not run removal commands from this document.** Inventory only.

Observed on the operator's machine (2026-10-03, `agg-cli adapters`):

| Index | Name | Notes |
|---|---|---|
| 1 | Ethernet | Physical. Safe to keep. Use for `intercept` if on cable. |
| 2 | Bluetooth Network Connection | Physical. Keep. |
| 3 | vEthernet (Default Switch) | Hyper-V. Keep if you use Hyper-V. |
| 4–6 | WAN Network Interface (*) | RRAS. Keep. |
| 7–8 | Local Area Connection* | Often Microsoft hosted-network / virtual Wi-Fi. Skip for intercept. |
| 9 | Main | Likely Wi-Fi. Use for `intercept` if wireless. |
| 10 | OpenVPN Data Channel Offload | Leftover OpenVPN. Candidate to uninstall via OpenVPN, not by deleting the NIC blindly. |
| 11 | DE-1 | Named tunnel leftover. Candidate. |
| 12 | Local Area Connection (`00:FF:4F:ED:50:43`) | Classic TAP-Windows MAC prefix. Candidate. |

**AGG Wintun adapter:** friendly name `AGG`. Created by `agg-cli up` on Windows after handshake. Must disappear after Ctrl-C or `agg-cli down`. If it remains, `agg-cli down` (admin).

NDIS inject is not used. `agg-cli down` also restores WinpkFilter filter mode if a previous LISTEN session was killed.
