# Threat model

Personal desktop client. Two operator-controlled AmneziaWG 2.0 VPSes. No accounts, no cloud, no telemetry.

## We protect against

- Accidental leak of the private key into logs, `Debug`, crash reports
- Blackholed traffic after crash / Ctrl-C / reboot (route rollback)
- Treating the UI as trusted (Phase 2+: service validates IPC)

## We do not protect against

- A compromised VPS
- A local attacker with admin
- Traffic analysis beyond AWG 2.0 obfuscation
- AWG 1.5 I-chain or AWG 3.0 header protection (out of scope for v1)

## Anti-cheat

All routing at the OS network layer. No injection, hooks, memory reads, or debugger attachment to game processes.
