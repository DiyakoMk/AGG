import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { ConnectRing } from "./ConnectRing";
import { Sparkline } from "./Sparkline";
import { IDLE, Profile, StatusSnapshot } from "./types";
import "./App.css";

type Tab = "home" | "servers" | "settings";
type Sheet = "none" | "servers" | "paste";

function fmtRtt(n: number | null | undefined): string {
  if (n == null) return "—";
  return String(Math.round(n));
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function stateCopy(state: StatusSnapshot["state"]): string {
  switch (state) {
    case "connecting":
      return "Handshaking";
    case "connected":
      return "Connected";
    case "disconnecting":
      return "Disconnecting";
    case "error":
      return "Connection failed";
    default:
      return "Disconnected";
  }
}

export default function App() {
  const [status, setStatus] = useState<StatusSnapshot>(IDLE);
  const [history, setHistory] = useState<number[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<Tab>("home");
  const [sheet, setSheet] = useState<Sheet>("none");
  const [pasteName, setPasteName] = useState("");
  const [pasteBody, setPasteBody] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [helper, setHelper] = useState<"ok" | "down" | "checking">("checking");
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");

  const refresh = useCallback(async () => {
    const list = await invoke<Profile[]>("list_profiles");
    setProfiles(list);
    const active = await invoke<string | null>("active_profile");
    setSelected((cur) => cur ?? active ?? list[0]?.id ?? null);
  }, []);

  useEffect(() => {
    invoke<StatusSnapshot>("get_status").then(setStatus).catch(() => {});
    invoke("helper_ok")
      .then(() => setHelper("ok"))
      .catch((e) => {
        setHelper("down");
        setNote(String(e));
      });
    refresh().catch(() => {});
    const un = listen<StatusSnapshot>("status", (e) => {
      setStatus(e.payload);
      if (e.payload.rtt_history?.length) setHistory(e.payload.rtt_history);
      else if (e.payload.rtt_ms != null) {
        setHistory((h) => [...h, e.payload.rtt_ms as number].slice(-60));
      }
      if (e.payload.state === "idle") setHistory([]);
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, [refresh]);

  const live = status.state === "connected" || status.state === "connecting";
  const current = profiles.find((p) => p.id === (status.profile_id ?? selected));
  const series = useMemo(
    () => (status.rtt_history?.length ? status.rtt_history : history),
    [status.rtt_history, history]
  );

  async function importPaths(paths: string[]) {
    if (!paths.length) return;
    setBusy(true);
    try {
      const added = await invoke<Profile[]>("import_files", { paths });
      await refresh();
      if (added[0]) {
        setSelected(added[0].id);
        await invoke("select_profile", { id: added[0].id }).catch(() => {});
      }
      setSheet("none");
      setTab("home");
      setNote(added.length === 1 ? `Added ${added[0].name}` : `Added ${added.length} configs`);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "AmneziaWG", extensions: ["conf"] }],
    });
    if (!picked) return;
    await importPaths(Array.isArray(picked) ? picked : [picked]);
  }

  async function onPaste(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const p = await invoke<Profile>("import_text", {
        name: pasteName || null,
        body: pasteBody,
      });
      setPasteName("");
      setPasteBody("");
      await refresh();
      setSelected(p.id);
      await invoke("select_profile", { id: p.id }).catch(() => {});
      setSheet("none");
      setTab("home");
      setNote(`Added ${p.name}`);
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function choose(id: string) {
    setSelected(id);
    await invoke("select_profile", { id }).catch(() => {});
    setSheet("none");
    setTab("home");
  }

  async function onConnect() {
    if (!selected) {
      setSheet("servers");
      return;
    }
    setBusy(true);
    setNote(null);
    try {
      await invoke("connect", { profileId: selected });
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
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onDelete(id: string) {
    await invoke("delete_profile", { id });
    await refresh();
    setSelected((s) => (s === id ? null : s));
  }

  async function onRename(e: FormEvent) {
    e.preventDefault();
    if (!renameId || !renameVal.trim()) return;
    await invoke("rename_profile", { id: renameId, name: renameVal.trim() });
    setRenameId(null);
    setRenameVal("");
    await refresh();
  }

  return (
    <div className="shell">
      {tab === "home" && (
        <main className="home">
          <p className={`status is-${status.state}`}>{stateCopy(status.state)}</p>

          <ConnectRing
            state={status.state}
            disabled={busy}
            onClick={live ? onDisconnect : onConnect}
          />

          <button
            type="button"
            className="server-chip"
            onClick={() => setSheet("servers")}
          >
            <span className="server-chip-k">Server</span>
            <strong>{current?.name ?? "Add a config"}</strong>
            <em>{current ? "AmneziaWG" : "No location"}</em>
          </button>

          {status.state === "connected" && (
            <section className="live">
              <div className="rtt">
                <span className="n">{fmtRtt(status.rtt_ms)}</span>
                <span className="u">ms</span>
              </div>
              <Sparkline values={series} width={280} height={48} />
              <ul className="stats">
                <li>
                  <span>Download</span>
                  <b>{fmtBytes(status.rx_bytes)}</b>
                </li>
                <li>
                  <span>Upload</span>
                  <b>{fmtBytes(status.tx_bytes)}</b>
                </li>
                <li>
                  <span>Handshake</span>
                  <b>
                    {status.handshake_age_ms != null
                      ? `${Math.round(status.handshake_age_ms / 1000)}s`
                      : "—"}
                  </b>
                </li>
              </ul>
            </section>
          )}

          <p className="split">All traffic · split tunnel later</p>
          {status.error && <p className="fault">{status.error}</p>}
          {note && <p className="note">{note}</p>}
        </main>
      )}

      {tab === "servers" && (
        <main className="page">
          <header className="page-h">
            <h1>Servers</h1>
            <div className="page-actions">
              <button type="button" className="ghost" onClick={pickFiles}>
                File
              </button>
              <button type="button" className="ghost" onClick={() => setSheet("paste")}>
                Paste
              </button>
            </div>
          </header>
          <ServerList
            profiles={profiles}
            selected={selected}
            renameId={renameId}
            renameVal={renameVal}
            onChoose={choose}
            onRenameStart={(p) => {
              setRenameId(p.id);
              setRenameVal(p.name);
            }}
            onRenameChange={setRenameVal}
            onRenameSubmit={onRename}
            onRenameCancel={() => setRenameId(null)}
            onDelete={onDelete}
          />
        </main>
      )}

      {tab === "settings" && (
        <main className="page">
          <header className="page-h">
            <h1>Settings</h1>
          </header>
          <ul className="rows">
            <li>
              <div>
                <strong>Tunnel helper</strong>
                <em>
                  {helper === "ok"
                    ? "AGGService is running"
                    : helper === "checking"
                      ? "Checking…"
                      : "Not running — reinstall AGG"}
                </em>
              </div>
              <b className={helper === "ok" ? "ok" : "bad"}>
                {helper === "ok" ? "On" : helper === "checking" ? "…" : "Off"}
              </b>
            </li>
            <li>
              <div>
                <strong>Protocol</strong>
                <em>AmneziaWG 2.0 · Wintun adapter AGG</em>
              </div>
            </li>
            <li>
              <div>
                <strong>Kill switch</strong>
                <em>Not in this build</em>
              </div>
            </li>
            <li>
              <div>
                <strong>Split tunneling</strong>
                <em>All traffic until the next phase</em>
              </div>
            </li>
            <li>
              <div>
                <strong>About</strong>
                <em>AGG 0.1 · personal Windows client</em>
              </div>
            </li>
          </ul>
        </main>
      )}

      <nav className="tabs">
        <button type="button" className={tab === "home" ? "on" : ""} onClick={() => setTab("home")}>
          Home
        </button>
        <button
          type="button"
          className={tab === "servers" ? "on" : ""}
          onClick={() => setTab("servers")}
        >
          Servers
        </button>
        <button
          type="button"
          className={tab === "settings" ? "on" : ""}
          onClick={() => setTab("settings")}
        >
          Settings
        </button>
      </nav>

      {sheet !== "none" && (
        <div className="scrim" onClick={() => setSheet("none")}>
          <div className="sheet" onClick={(e) => e.stopPropagation()}>
            <div className="grab" />
            {sheet === "servers" && (
              <>
                <div className="sheet-h">
                  <h2>Servers</h2>
                  <div>
                    <button type="button" className="ghost" onClick={pickFiles}>
                      File
                    </button>
                    <button type="button" className="ghost" onClick={() => setSheet("paste")}>
                      Paste
                    </button>
                  </div>
                </div>
                <ServerList
                  profiles={profiles}
                  selected={selected}
                  renameId={renameId}
                  renameVal={renameVal}
                  onChoose={choose}
                  onRenameStart={(p) => {
                    setRenameId(p.id);
                    setRenameVal(p.name);
                  }}
                  onRenameChange={setRenameVal}
                  onRenameSubmit={onRename}
                  onRenameCancel={() => setRenameId(null)}
                  onDelete={onDelete}
                />
              </>
            )}
            {sheet === "paste" && (
              <form onSubmit={onPaste} className="paste">
                <h2>Paste config</h2>
                <input
                  placeholder="Name"
                  value={pasteName}
                  onChange={(e) => setPasteName(e.target.value)}
                />
                <textarea
                  required
                  rows={12}
                  placeholder={"[Interface]\nPrivateKey = "}
                  value={pasteBody}
                  onChange={(e) => setPasteBody(e.target.value)}
                />
                <button type="submit" disabled={busy || !pasteBody.trim()}>
                  Save
                </button>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ServerList({
  profiles,
  selected,
  renameId,
  renameVal,
  onChoose,
  onRenameStart,
  onRenameChange,
  onRenameSubmit,
  onRenameCancel,
  onDelete,
}: {
  profiles: Profile[];
  selected: string | null;
  renameId: string | null;
  renameVal: string;
  onChoose: (id: string) => void;
  onRenameStart: (p: Profile) => void;
  onRenameChange: (v: string) => void;
  onRenameSubmit: (e: FormEvent) => void;
  onRenameCancel: () => void;
  onDelete: (id: string) => void;
}) {
  if (profiles.length === 0) {
    return <p className="empty">Import an AmneziaWG .conf to start.</p>;
  }
  return (
    <ul className="locs">
      {profiles.map((p) => (
        <li key={p.id} className={p.id === selected ? "sel" : ""}>
          {renameId === p.id ? (
            <form className="rename" onSubmit={onRenameSubmit}>
              <input
                autoFocus
                value={renameVal}
                onChange={(e) => onRenameChange(e.target.value)}
              />
              <button type="submit">Save</button>
              <button type="button" className="ghost" onClick={onRenameCancel}>
                Cancel
              </button>
            </form>
          ) : (
            <>
              <button type="button" className="loc-pick" onClick={() => onChoose(p.id)}>
                <strong>{p.name}</strong>
                <em>
                  {p.obfuscated ? "AmneziaWG" : "WireGuard"} · {p.endpoint ?? "no endpoint"}
                </em>
              </button>
              <div className="loc-act">
                <button type="button" className="ghost" onClick={() => onRenameStart(p)}>
                  Rename
                </button>
                <button type="button" className="ghost" onClick={() => onDelete(p.id)}>
                  Remove
                </button>
              </div>
            </>
          )}
        </li>
      ))}
    </ul>
  );
}
