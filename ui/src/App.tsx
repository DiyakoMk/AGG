import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Sparkline } from "./Sparkline";
import { IDLE, Profile, StatusSnapshot } from "./types";
import "./App.css";

function stateLabel(s: StatusSnapshot["state"]): string {
  switch (s) {
    case "idle":
      return "disconnected";
    case "connecting":
      return "connecting";
    case "connected":
      return "connected";
    case "disconnecting":
      return "disconnecting";
    case "error":
      return "fault";
  }
}

function fmt(n: number | null | undefined, digits = 0): string {
  if (n == null) return "—";
  return n.toFixed(digits);
}

export default function App() {
  const [status, setStatus] = useState<StatusSnapshot>(IDLE);
  const [history, setHistory] = useState<number[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pasteOpen, setPasteOpen] = useState(false);
  const [pasteName, setPasteName] = useState("");
  const [pasteBody, setPasteBody] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");
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
      if (e.payload.rtt_history?.length) {
        setHistory(e.payload.rtt_history);
      } else if (e.payload.rtt_ms != null) {
        setHistory((h) => [...h, e.payload.rtt_ms as number].slice(-60));
      }
      if (e.payload.state === "idle") setHistory([]);
    });
    let dropUn: (() => void) | undefined;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) => {
        getCurrentWebview()
          .onDragDropEvent((ev) => {
            if (ev.payload.type === "over") setDragOver(true);
            if (ev.payload.type === "leave" || ev.payload.type === "drop") {
              setDragOver(false);
            }
            if (ev.payload.type === "drop") {
              importPaths(ev.payload.paths);
            }
          })
          .then((u) => {
            dropUn = u;
          })
          .catch(() => {});
      })
      .catch(() => {});
    return () => {
      un.then((f) => f()).catch(() => {});
      dropUn?.();
    };
  }, [refresh]);

  const live = status.state === "connected" || status.state === "connecting";
  const current = profiles.find((p) => p.id === (status.profile_id ?? selected));

  async function importPaths(paths: string[]) {
    if (!paths.length) return;
    setBusy(true);
    setNote(null);
    try {
      const added = await invoke<Profile[]>("import_files", { paths });
      await refresh();
      if (added[0]) setSelected(added[0].id);
      setNote(
        added.length === 1
          ? `added ${added[0].name}`
          : `added ${added.length} configs`
      );
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "AmneziaWG / WireGuard", extensions: ["conf"] }],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await importPaths(paths);
  }

  async function onPaste(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const p = await invoke<Profile>("import_text", {
        name: pasteName || null,
        body: pasteBody,
      });
      setPasteOpen(false);
      setPasteName("");
      setPasteBody("");
      await refresh();
      setSelected(p.id);
      setNote(`added ${p.name}`);
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onConnect() {
    if (!selected) return;
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
    setBusy(true);
    try {
      await invoke("delete_profile", { id });
      await refresh();
      setSelected((s) => (s === id ? null : s));
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function commitRename() {
    if (!renameId) return;
    setBusy(true);
    try {
      await invoke("rename_profile", { id: renameId, name: renameVal });
      setRenameId(null);
      await refresh();
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }

  const rttSeries = useMemo(
    () => (status.rtt_history?.length ? status.rtt_history : history),
    [status.rtt_history, history]
  );

  const verdict =
    status.state === "connected"
      ? status.rtt_ms != null
        ? `tunnel ${status.rtt_ms} ms · handshake ${(status.handshake_age_ms ?? 0) / 1000 < 180 ? "fresh" : "aging"}`
        : "connected · waiting for rtt"
      : status.state === "error"
        ? status.error ?? "fault"
        : "direct path in use until you connect";

  return (
    <div
      className={`app ${dragOver ? "dragging" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        const files = [...e.dataTransfer.files]
          .map((f) => (f as File & { path?: string }).path)
          .filter((p): p is string => !!p);
        if (files.length) importPaths(files);
        else setNote("drop .conf files from disk — browser drops have no path");
      }}
    >
      <aside className="rail">
        <header className="rail-head">
          <span className="mark">agg</span>
          <span className="count">{profiles.length}</span>
        </header>

        <div className="drop">
          <p>drop .conf files here</p>
          <div className="drop-actions">
            <button type="button" onClick={pickFiles} disabled={busy}>
              browse
            </button>
            <button type="button" onClick={() => setPasteOpen(true)}>
              paste
            </button>
          </div>
        </div>

        <ul className="list">
          {profiles.length === 0 && (
            <li className="empty">no configs yet — drop, browse, or paste</li>
          )}
          {profiles.map((p) => {
            const on = p.id === selected;
            const liveHere = status.profile_id === p.id && live;
            return (
              <li
                key={p.id}
                className={`card ${on ? "on" : ""} ${liveHere ? "live" : ""}`}
                onClick={() => setSelected(p.id)}
              >
                {renameId === p.id ? (
                  <input
                    autoFocus
                    className="rename"
                    value={renameVal}
                    onChange={(e) => setRenameVal(e.target.value)}
                    onBlur={commitRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename();
                      if (e.key === "Escape") setRenameId(null);
                    }}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <button
                    type="button"
                    className="name"
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      setRenameId(p.id);
                      setRenameVal(p.name);
                    }}
                  >
                    {p.name}
                  </button>
                )}
                <span className="meta">
                  {p.endpoint ?? "no endpoint"}
                  {p.obfuscated ? " · awg" : " · wg"}
                </span>
                <button
                  type="button"
                  className="x"
                  title="remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(p.id);
                  }}
                >
                  ×
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      <main className="stage">
        <p className={`state is-${status.state}`}>{stateLabel(status.state)}</p>

        <div className="hero">
          <div>
            <span className="unit">rtt</span>
            <span className="num">{fmt(status.rtt_ms)}</span>
            <span className="unit">ms</span>
          </div>
          <Sparkline values={rttSeries} />
        </div>

        <p className="verdict">{verdict}</p>

        <dl className="strip">
          <div>
            <dt>endpoint</dt>
            <dd>{status.endpoint ?? current?.endpoint ?? "—"}</dd>
          </div>
          <div>
            <dt>handshake</dt>
            <dd>
              {status.handshake_age_ms != null
                ? `${(status.handshake_age_ms / 1000).toFixed(1)} s`
                : "—"}
            </dd>
          </div>
          <div>
            <dt>tx / rx</dt>
            <dd>
              {status.tx_bytes} / {status.rx_bytes}
            </dd>
          </div>
          <div>
            <dt>loss</dt>
            <dd>{status.loss.toFixed(3)}</dd>
          </div>
        </dl>

        {status.error && <p className="fault">{status.error}</p>}
        {note && <p className="note">{note}</p>}

        <button
          type="button"
          className={live ? "go stop" : "go"}
          disabled={busy || (!live && !selected)}
          onClick={live ? onDisconnect : onConnect}
        >
          {live ? "disconnect" : selected ? `connect ${current?.name ?? ""}` : "pick a config"}
        </button>
      </main>

      {pasteOpen && (
        <div className="modal" onClick={() => setPasteOpen(false)}>
          <form
            className="sheet"
            onClick={(e) => e.stopPropagation()}
            onSubmit={onPaste}
          >
            <h2>paste a config</h2>
            <input
              placeholder="name (optional)"
              value={pasteName}
              onChange={(e) => setPasteName(e.target.value)}
            />
            <textarea
              required
              placeholder="[Interface]&#10;PrivateKey = …"
              value={pasteBody}
              onChange={(e) => setPasteBody(e.target.value)}
              rows={14}
            />
            <div className="row">
              <button type="button" onClick={() => setPasteOpen(false)}>
                cancel
              </button>
              <button type="submit" disabled={busy || !pasteBody.trim()}>
                add
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
