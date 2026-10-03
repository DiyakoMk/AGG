//! Probe and failover (Phase 4). Stub so the crate layout matches the brief.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathHealth {
    Unknown,
    Healthy,
    Degraded,
    Down,
}
