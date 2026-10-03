import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Sparkline } from "./Sparkline";
import { IDLE, Profile, StatusSnapshot } from "./types";
import "./App.css";

function fmt(n: number | null | undefined): string {
  if (n == null) return "—";
  return String(Math.round(n));
}

export default function App() {
  const [status, setStatus] = useState<StatusSnapshot>(IDLE);
  const [history, setHistory] = useState<number[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [sheet, setSheet] = useState<"none" | "list" | "paste">("none");
  const [pasteName, setPasteName] = useState("");
  const [pasteBody, setPasteBody] = useState("");
  const [note, setNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const list = await invoke<Profile[]>("list_profiles");
    setProfiles(list);
    const active = await invoke<string | null>("active_profile");
    setSelected((cur) => cur ?? active ?? list[0]?.id ?? null);
  }, []);

  useEffect(() => {
    invoke<StatusSnapshot>("get_status").then(setStatus).catch(() => {});
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
      if (added[0]) setSelected(added[0].id);
      setSheet("none");
      setNote(added.length === 1 ? added[0].name : `${added.length} added`);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "config", extensions: ["conf"] }],
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
      setSheet("list");
      setNote(p.name);
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onConnect() {
    if (!selected) {
      setSheet("list");
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

  return (
    <div className="phone">
      <header className="top">
        <span className="brand">agg</span>
        <button type="button" className="ghost" onClick={() => setSheet("list")}>
          {profiles.length} loc
        </button>
      </header>

      <p className={`pill is-${status.state}`}>
        {status.state === "idle" && "off"}
        {status.state === "connecting" && "handshaking"}
        {status.state === "connected" && "on"}
        {status.state === "disconnecting" && "closing"}
        {status.state === "error" && "fault"}
      </p>

      <div className="rtt">
        <span className="n">{fmt(status.rtt_ms)}</span>
        <span className="u">ms</span>
      </div>
      <Sparkline values={series} width={280} height={56} />

      <p className="loc">{current?.name ?? "no location"}</p>
      <p className="ep">{status.endpoint ?? current?.endpoint ?? ""}</p>

      <ul className="stats">
        <li>
          <span>loss</span>
          <b>{status.loss.toFixed(2)}</b>
        </li>
        <li>
          <span>age</span>
          <b>
            {status.handshake_age_ms != null
              ? `${Math.round(status.handshake_age_ms / 1000)}s`
              : "—"}
          </b>
        </li>
        <li>
          <span>tx</span>
          <b>{status.tx_bytes}</b>
        </li>
      </ul>

      {status.error && <p className="fault">{status.error}</p>}
      {note && <p className="note">{note}</p>}

      <button
        type="button"
        className={`orb ${live ? "on" : ""}`}
        disabled={busy}
        onClick={live ? onDisconnect : onConnect}
      >
        {live ? "stop" : "go"}
      </button>

      {sheet !== "none" && (
        <div className="scrim" onClick={() => setSheet("none")}>
          <div className="sheet" onClick={(e) => e.stopPropagation()}>
            <div className="grab" />
            {sheet === "list" && (
              <>
                <div className="sheet-h">
                  <h2>locations</h2>
                  <div>
                    <button type="button" className="ghost" onClick={pickFiles}>
                      files
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => setSheet("paste")}
                    >
                      paste
                    </button>
                  </div>
                </div>
                <ul className="locs">
                  {profiles.length === 0 && (
                    <li className="empty">add a .conf to start</li>
                  )}
                  {profiles.map((p) => (
                    <li
                      key={p.id}
                      className={p.id === selected ? "sel" : ""}
                      onClick={() => {
                        setSelected(p.id);
                        setSheet("none");
                      }}
                    >
                      <div>
                        <strong>{p.name}</strong>
                        <em>{p.endpoint ?? "—"}</em>
                      </div>
                      <button
                        type="button"
                        className="ghost"
                        onClick={(e) => {
                          e.stopPropagation();
                          onDelete(p.id);
                        }}
                      >
                        remove
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {sheet === "paste" && (
              <form onSubmit={onPaste} className="paste">
                <h2>paste config</h2>
                <input
                  placeholder="name"
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
                  save
                </button>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
