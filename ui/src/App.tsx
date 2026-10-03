import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type ConnectionState =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

type StatusSnapshot = {
  state: ConnectionState;
  server: string | null;
  endpoint: string | null;
  rtt_ms: number | null;
  handshake_age_ms: number | null;
  tx_bytes: number;
  rx_bytes: number;
  loss: number;
  error: string | null;
};

const IDLE: StatusSnapshot = {
  state: "idle",
  server: null,
  endpoint: null,
  rtt_ms: null,
  handshake_age_ms: null,
  tx_bytes: 0,
  rx_bytes: 0,
  loss: 0,
  error: null,
};

function label(state: ConnectionState): string {
  switch (state) {
    case "idle":
      return "DISCONNECTED";
    case "connecting":
      return "CONNECTING";
    case "connected":
      return "CONNECTED";
    case "disconnecting":
      return "DISCONNECTING";
    case "error":
      return "ERROR";
  }
}

function fmtMs(n: number | null): string {
  if (n == null) return "—";
  return n.toFixed(0);
}

function App() {
  const [status, setStatus] = useState<StatusSnapshot>(IDLE);
  const [configPath, setConfigPath] = useState("awg.conf");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<StatusSnapshot>("get_status").then(setStatus).catch(() => {});
    const un = listen<StatusSnapshot>("status", (e) => setStatus(e.payload));
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, []);

  const connected = status.state === "connected";
  const canConnect = status.state === "idle" || status.state === "error";
  const canDisconnect =
    status.state === "connected" || status.state === "connecting";

  async function onConnect() {
    setBusy(true);
    try {
      await invoke("connect", { configPath });
    } catch (e) {
      setStatus({ ...IDLE, state: "error", error: String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function onDisconnect() {
    setBusy(true);
    try {
      await invoke("disconnect");
    } catch (e) {
      setStatus({ ...IDLE, state: "error", error: String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <p className={`state state-${status.state}`}>{label(status.state)}</p>

      <p className="metric">
        <span className="metric-label">RTT</span>
        <span className="metric-value">{fmtMs(status.rtt_ms)}</span>
        <span className="metric-unit">ms</span>
      </p>

      <p className="endpoint">{status.endpoint ?? "no endpoint"}</p>

      <dl className="strip">
        <div>
          <dt>handshake</dt>
          <dd>
            {status.handshake_age_ms != null
              ? `${(status.handshake_age_ms / 1000).toFixed(1)}s`
              : "—"}
          </dd>
        </div>
        <div>
          <dt>tx</dt>
          <dd>{status.tx_bytes}</dd>
        </div>
        <div>
          <dt>rx</dt>
          <dd>{status.rx_bytes}</dd>
        </div>
        <div>
          <dt>loss</dt>
          <dd>{status.loss.toFixed(3)}</dd>
        </div>
      </dl>

      {status.error && <p className="err">{status.error}</p>}

      <label className="path">
        config
        <input
          value={configPath}
          onChange={(e) => setConfigPath(e.target.value)}
          disabled={!canConnect}
        />
      </label>

      <button
        className={connected ? "btn disconnect" : "btn connect"}
        disabled={busy || (!canConnect && !canDisconnect)}
        onClick={canDisconnect ? onDisconnect : onConnect}
      >
        {canDisconnect ? "Disconnect" : "Connect"}
      </button>

      <p className="hint">
        Windows: full tunnel via Wintun. Elsewhere: UDP handshake + live RTT
        only. No polling — Rust emits <code>status</code> at 1 Hz.
      </p>
    </main>
  );
}

export default App;
