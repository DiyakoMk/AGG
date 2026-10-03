import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ConnectRing } from "./ConnectRing";
import { Sparkline } from "./Sparkline";
import {
  DEFAULT_OPTS,
  DetectedApp,
  FilterStatus,
  IDLE,
  Profile,
  SessionOpts,
  sourceLabel,
  StatusSnapshot,
} from "./types";
import "./App.css";

type Tab = "home" | "routes" | "apps" | "settings";
type Sheet = "none" | "routes" | "add";

function fmtRtt(n: number | null | undefined): string {
  if (n == null) return "—";
  return String(Math.round(n));
}

function pingTone(n: number | null | undefined): "good" | "ok" | "high" | "off" {
  if (n == null) return "off";
  if (n < 50) return "good";
  if (n < 90) return "ok";
  return "high";
}

function stateCopy(state: StatusSnapshot["state"]): string {
  switch (state) {
    case "connecting":
      return "Finding a better path";
    case "connected":
      return "Ping boosted";
    case "disconnecting":
      return "Dropping the boost";
    case "error":
      return "Boost failed";
    default:
      return "Direct connection";
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
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");
  const [dropOver, setDropOver] = useState(false);
  const [opts, setOpts] = useState<SessionOpts>(DEFAULT_OPTS);
  const [apps, setApps] = useState<DetectedApp[]>([]);
  const [tunneled, setTunneled] = useState<string[]>([]);
  const [appQuery, setAppQuery] = useState("");
  const [appSource, setAppSource] = useState<string>("all");
  const [scanning, setScanning] = useState(false);
  const [filter, setFilter] = useState<FilterStatus | null>(null);

  const refresh = useCallback(async () => {
    const list = await invoke<Profile[]>("list_profiles");
    setProfiles(list);
    const active = await invoke<string | null>("active_profile");
    setSelected((cur) => cur ?? active ?? list[0]?.id ?? null);
  }, []);

  useEffect(() => {
    invoke<StatusSnapshot>("get_status").then(setStatus).catch(() => {});
    invoke("helper_ok").catch((e) => setNote(String(e)));
    invoke<SessionOpts>("get_opts").then(setOpts).catch(() => {});
    invoke<DetectedApp[]>("list_apps").then(setApps).catch(() => {});
    invoke<string[]>("tunneled_ids").then(setTunneled).catch(() => {});
    invoke<FilterStatus>("filter_status").then(setFilter).catch(() => {});
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
  const tone = pingTone(status.rtt_ms);

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
      setNote(
        added.length === 1 ? `${added[0].name} is ready to boost` : `${added.length} routes added`
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
      filters: [{ name: "VPN config", extensions: ["conf"] }],
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
      setNote(`${p.name} is ready to boost`);
    } catch (err) {
      setNote(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDropFiles(files: FileList | File[]) {
    const list = Array.from(files);
    const texts: { name: string; body: string }[] = [];
    for (const f of list) {
      if (!f.name.toLowerCase().endsWith(".conf") && f.type && !f.type.includes("text")) {
        continue;
      }
      texts.push({ name: f.name.replace(/\.conf$/i, ""), body: await f.text() });
    }
    if (!texts.length) {
      setNote("Drop a .conf file from your VPN");
      return;
    }
    setBusy(true);
    try {
      let last: Profile | null = null;
      for (const t of texts) {
        last = await invoke<Profile>("import_text", { name: t.name, body: t.body });
      }
      await refresh();
      if (last) {
        setSelected(last.id);
        await invoke("select_profile", { id: last.id }).catch(() => {});
      }
      setSheet("none");
      setTab("home");
      setNote(
        texts.length === 1 ? `${texts[0].name} is ready to boost` : `${texts.length} routes added`
      );
    } catch (e) {
      setNote(String(e));
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
      setSheet("add");
      setTab("routes");
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

  async function saveOpts(next: SessionOpts) {
    try {
      const saved = await invoke<SessionOpts>("set_opts", { opts: next });
      setOpts(saved);
    } catch (e) {
      setNote(String(e));
    }
  }

  async function saveTunneled(ids: string[]) {
    const next = await invoke<string[]>("set_tunneled", { ids });
    setTunneled(next);
  }

  async function toggleApp(id: string) {
    const next = tunneled.includes(id)
      ? tunneled.filter((x) => x !== id)
      : [...tunneled, id];
    await saveTunneled(next);
  }

  async function rescan() {
    setScanning(true);
    try {
      const list = await invoke<DetectedApp[]>("refresh_apps");
      setApps(list);
    } catch (e) {
      setNote(String(e));
    } finally {
      setScanning(false);
    }
  }

  async function addManual() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "App", extensions: ["exe"] }],
    });
    if (!picked || Array.isArray(picked)) return;
    try {
      const a = await invoke<DetectedApp>("add_manual_app", { path: picked });
      setApps((cur) => (cur.some((x) => x.id === a.id) ? cur : [...cur, a]));
    } catch (e) {
      setNote(String(e));
    }
  }

  const shownApps = apps.filter((a) => {
    if (appSource !== "all" && a.source !== appSource) return false;
    if (!appQuery.trim()) return true;
    const q = appQuery.toLowerCase();
    return a.name.toLowerCase().includes(q) || a.executable.toLowerCase().includes(q);
  });

  const addPanel = (
    <AddConfig
      dropOver={dropOver}
      setDropOver={setDropOver}
      pasteName={pasteName}
      pasteBody={pasteBody}
      setPasteName={setPasteName}
      setPasteBody={setPasteBody}
      busy={busy}
      onPick={pickFiles}
      onPaste={onPaste}
      onDropFiles={onDropFiles}
    />
  );

  return (
    <div className="shell">
      {tab === "home" && (
        <main className="home">
          <header className="hud-top">
            <span className="brand">AGG</span>
            <span className="tag">ping booster</span>
          </header>

          <p className={`status is-${status.state}`}>{stateCopy(status.state)}</p>

          <div className={`hud-ping is-${tone}`}>
            <span className="n">{fmtRtt(status.rtt_ms)}</span>
            <span className="u">ms</span>
          </div>
          <p className="ping-hint">
            {status.state === "connected"
              ? tone === "good"
                ? "Low ping"
                : tone === "ok"
                  ? "Playable"
                  : "High ping — try another route"
              : "Tap Boost before a match"}
          </p>
          <Sparkline values={series} width={280} height={44} />

          <ConnectRing
            state={status.state}
            disabled={busy}
            onClick={live ? onDisconnect : onConnect}
          />

          <button type="button" className="server-chip" onClick={() => setSheet("routes")}>
            <span className="server-chip-k">Route</span>
            <strong>{current?.name ?? "Add a game route"}</strong>
            <em>{current?.endpoint ?? "Drop in a .conf from your VPN"}</em>
          </button>

          <p className="split-line">
            {tunneled.length
              ? `${tunneled.length} boosted`
              : "Pick games in Apps"}
          </p>

          {status.error && <p className="fault">{status.error}</p>}
          {note && <p className="note">{note}</p>}
        </main>
      )}

      {tab === "routes" && (
        <main className="page">
          <header className="page-h">
            <h1>Routes</h1>
            <button type="button" className="ghost" onClick={() => setSheet("add")}>
              Add
            </button>
          </header>
          {profiles.length === 0 ? (
            addPanel
          ) : (
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
          )}
        </main>
      )}

      {tab === "apps" && (
        <main className="page">
          <header className="page-h">
            <h1>Apps</h1>
            <div className="page-actions">
              <button type="button" className="ghost" onClick={addManual}>
                Add
              </button>
              <button type="button" className="ghost" disabled={scanning} onClick={rescan}>
                {scanning ? "Scanning…" : "Refresh"}
              </button>
            </div>
          </header>
          <div className="app-tools">
            <input
              className="search"
              placeholder="Search"
              value={appQuery}
              onChange={(e) => setAppQuery(e.target.value)}
            />
            <select value={appSource} onChange={(e) => setAppSource(e.target.value)}>
              <option value="all">All</option>
              <option value="steam">Steam</option>
              <option value="epic">Epic</option>
              <option value="riot">Riot</option>
              <option value="battle_net">Battle.net</option>
              <option value="gog">GOG</option>
              <option value="discord">Discord</option>
              <option value="manual">Manual</option>
            </select>
          </div>
          <div className="page-actions bulk">
            <button
              type="button"
              className="ghost"
              onClick={() =>
                saveTunneled(
                  apps.filter((a) => a.source !== "discord" && !a.missing).map((a) => a.id)
                )
              }
            >
              Select games
            </button>
            <button type="button" className="ghost" onClick={() => saveTunneled([])}>
              Clear
            </button>
          </div>
          <p className="hint">
            {filter?.present
              ? "Boosted apps go through AGG. Direct stays on your ISP."
              : "Install Windows Packet Filter (personal use) so Boosted apps can go through AGG."}
          </p>
          <ul className="apps">
            {shownApps.length === 0 && (
              <li className="empty">No games yet — Refresh or add an .exe</li>
            )}
            {shownApps.map((a) => {
              const on = tunneled.includes(a.id);
              return (
                <li key={a.id} className={`${on ? "boosted" : ""} ${a.missing ? "missing" : ""}`}>
                  <span className="app-ico" aria-hidden>
                    {a.name.slice(0, 1).toUpperCase()}
                  </span>
                  <div className="app-meta">
                    <strong>
                      {a.name}
                      {a.missing ? " — missing" : ""}
                    </strong>
                    <em>
                      {sourceLabel(a.source)}
                    </em>
                  </div>
                  <button
                    type="button"
                    className={`boost-sw ${on ? "on" : "off"}`}
                    disabled={a.missing}
                    onClick={() => toggleApp(a.id)}
                  >
                    {on ? "BOOSTED" : "DIRECT"}
                  </button>
                </li>
              );
            })}
          </ul>
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
                <strong>Packet filter</strong>
                <em>{filter?.present ? "Installed" : filter?.hint ?? "Checking…"}</em>
              </div>
              {!filter?.present && (
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() =>
                    openUrl(filter?.download ?? "https://github.com/wiresock/ndisapi/releases")
                  }
                >
                  Install
                </button>
              )}
            </li>
            <li>
              <div>
                <strong>Kill switch</strong>
                <em>Block the internet if the tunnel drops</em>
              </div>
              <Toggle
                on={opts.kill_switch}
                onClick={() => saveOpts({ ...opts, kill_switch: !opts.kill_switch })}
              />
            </li>
            <li>
              <div>
                <strong>Auto reconnect</strong>
                <em>After Wi-Fi, sleep, or network change</em>
              </div>
              <Toggle
                on={opts.auto_reconnect}
                onClick={() => saveOpts({ ...opts, auto_reconnect: !opts.auto_reconnect })}
              />
            </li>
            <li>
              <div>
                <strong>MTU sweep</strong>
                <em>Pick a path MTU so packets do not fragment</em>
              </div>
              <Toggle
                on={opts.mtu_sweep}
                onClick={() => saveOpts({ ...opts, mtu_sweep: !opts.mtu_sweep })}
              />
            </li>
            <li>
              <div>
                <strong>About</strong>
                <em>AGG 0.1 · personal Windows ping booster</em>
              </div>
            </li>
          </ul>
        </main>
      )}

      <nav className="tabs four">
        <button type="button" className={tab === "home" ? "on" : ""} onClick={() => setTab("home")}>
          Boost
        </button>
        <button
          type="button"
          className={tab === "routes" ? "on" : ""}
          onClick={() => setTab("routes")}
        >
          Routes
        </button>
        <button type="button" className={tab === "apps" ? "on" : ""} onClick={() => setTab("apps")}>
          Apps
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
            {sheet === "routes" && (
              <>
                <div className="sheet-h">
                  <h2>Routes</h2>
                  <button type="button" className="ghost" onClick={() => setSheet("add")}>
                    Add
                  </button>
                </div>
                {profiles.length === 0 ? (
                  addPanel
                ) : (
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
                )}
              </>
            )}
            {sheet === "add" && (
              <>
                <h2>Add a route</h2>
                {addPanel}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function AddConfig({
  dropOver,
  setDropOver,
  pasteName,
  pasteBody,
  setPasteName,
  setPasteBody,
  busy,
  onPick,
  onPaste,
  onDropFiles,
}: {
  dropOver: boolean;
  setDropOver: (v: boolean) => void;
  pasteName: string;
  pasteBody: string;
  setPasteName: (v: string) => void;
  setPasteBody: (v: string) => void;
  busy: boolean;
  onPick: () => void;
  onPaste: (e: FormEvent) => void;
  onDropFiles: (files: FileList | File[]) => void;
}) {
  return (
    <div className="add">
      <button
        type="button"
        className={`drop ${dropOver ? "over" : ""}`}
        onClick={onPick}
        onDragOver={(e) => {
          e.preventDefault();
          setDropOver(true);
        }}
        onDragLeave={() => setDropOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDropOver(false);
          if (e.dataTransfer.files.length) onDropFiles(e.dataTransfer.files);
        }}
      >
        <strong>Drop a .conf here</strong>
        <em>or click to browse — the file your VPN emailed you</em>
      </button>
      <p className="or">or paste it</p>
      <form onSubmit={onPaste} className="paste">
        <input
          placeholder="Name this route (EU West, NA East…)"
          value={pasteName}
          onChange={(e) => setPasteName(e.target.value)}
        />
        <textarea
          required
          rows={8}
          placeholder="Paste the whole config, starting with [Interface]"
          value={pasteBody}
          onChange={(e) => setPasteBody(e.target.value)}
        />
        <button type="submit" disabled={busy || !pasteBody.trim()}>
          Save route
        </button>
      </form>
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
                <em>{p.endpoint ?? "ready"}</em>
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

function Toggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  return (
    <button type="button" className={`seg ${on ? "on" : "off"}`} onClick={onClick}>
      <span className={on ? "lit" : ""}>ON</span>
      <span className={!on ? "lit" : ""}>OFF</span>
    </button>
  );
}
