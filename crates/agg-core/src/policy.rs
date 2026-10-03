//! Route / DNS / kill-switch decisions (Phase 4). Stub.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    FullTunnel,
    SplitByDestination,
}
