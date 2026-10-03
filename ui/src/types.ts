export type ConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type SessionOpts = {
  layouts: string[];
  kill_switch: boolean;
  auto_reconnect: boolean;
  mtu_sweep: boolean;
};

export type LayoutView = {
  id: string;
  name: string;
  hint: string;
};

export const DEFAULT_OPTS: SessionOpts = {
  layouts: ["steam", "discord"],
  kill_switch: false,
  auto_reconnect: true,
  mtu_sweep: true,
};

export type StatusSnapshot = {
  state: ConnectionState;
  server: string | null;
  endpoint: string | null;
  rtt_ms: number | null;
  handshake_age_ms: number | null;
  tx_bytes: number;
  rx_bytes: number;
  loss: number;
  error: string | null;
  profile_id: string | null;
  rtt_history: number[];
  opts: SessionOpts;
};

export type Profile = {
  id: string;
  name: string;
  endpoint: string | null;
  address: string | null;
  mtu: number | null;
  obfuscated: boolean;
  source: string;
};

export const IDLE: StatusSnapshot = {
  state: "idle",
  server: null,
  endpoint: null,
  rtt_ms: null,
  handshake_age_ms: null,
  tx_bytes: 0,
  rx_bytes: 0,
  loss: 0,
  error: null,
  profile_id: null,
  rtt_history: [],
  opts: DEFAULT_OPTS,
};
