export type ConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type SplitMode = "all" | "games" | "launchers" | "both";

export const SPLIT_OPTIONS: { id: SplitMode; name: string; hint: string }[] = [
  { id: "both", name: "Games + launchers", hint: "Match servers and Steam / Battle.net / Riot / EA / Ubisoft" },
  { id: "games", name: "Games", hint: "Match servers only — chat and browser stay direct" },
  { id: "launchers", name: "Launchers", hint: "Steam, Battle.net, Riot, EA, Ubisoft" },
  { id: "all", name: "Everything", hint: "Full tunnel — every app uses the boost" },
];

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
  split: SplitMode;
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
  split: "both",
};
