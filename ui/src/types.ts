export type ConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type SessionOpts = {
  kill_switch: boolean;
  auto_reconnect: boolean;
  mtu_sweep: boolean;
};

export const DEFAULT_OPTS: SessionOpts = {
  kill_switch: false,
  auto_reconnect: true,
  mtu_sweep: true,
};

export type AppSource =
  | "steam"
  | "epic"
  | "riot"
  | "battle_net"
  | "gog"
  | "discord"
  | "manual";

export type DetectedApp = {
  id: string;
  name: string;
  source: AppSource;
  executable: string;
  install_dir: string;
  icon_path: string | null;
  missing: boolean;
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

export type FilterStatus = {
  present: boolean;
  download: string;
  hint: string;
};

export function sourceLabel(s: AppSource): string {
  switch (s) {
    case "steam":
      return "Steam";
    case "epic":
      return "Epic";
    case "riot":
      return "Riot";
    case "battle_net":
      return "Battle.net";
    case "gog":
      return "GOG";
    case "discord":
      return "Discord";
    default:
      return "Manual";
  }
}
