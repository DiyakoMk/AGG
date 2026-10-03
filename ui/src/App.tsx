import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { ConnectRing } from "./ConnectRing";
import { Sparkline } from "./Sparkline";
import { DEFAULT_OPTS, IDLE, Profile, SessionOpts, SiteMode, StatusSnapshot } from "./types";
import "./App.css";

type Tab = "home" | "servers" | "split" | "settings";
type Sheet = "none" | "servers" | "add";

function fmtRtt(n: number | null | undefined): string {
  if (n == null) return "—";
  return String(Math.round(n));
}

function stateCopy(state: StatusSnapshot["state"]): string {
  switch (state) {
    case "connecting":
      return "Connecting";
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
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");
  const [dropOver, setDropOver] = useState(false);
  const [opts, setOpts] = useState<SessionOpts>(DEFAULT_OPTS);
  const [cidrDraft, setCidrDraft] = useState("");

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
      setNote(
        added.length === 1 ? `Added ${added[0].name}` : `Added ${added.length} configs`
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
      setNote(`Added ${p.name}`);
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
      setNote("Drop an AmneziaWG .conf");
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
        texts.length === 1 ? `Added ${texts[0].name}` : `Added ${texts.length} configs`
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

  async function setSiteMode(mode: SiteMode) {
    await saveOpts({ ...opts, site_mode: mode });
  }

  async function addCidr(e: FormEvent) {
    e.preventDefault();
    const c = cidrDraft.trim();
    if (!c) return;
    if (opts.split_sites.includes(c)) {
      setCidrDraft("");
      return;
    }
    await saveOpts({ ...opts, split_sites: [...opts.split_sites, c] });
    setCidrDraft("");
  }

  async function removeCidr(c: string) {
    await saveOpts({ ...opts, split_sites: opts.split_sites.filter((x) => x !== c) });
  }

  function splitLine(): string {
    const appsN = opts.bypass_apps?.length ?? 0;
    const sites = opts.site_mode;
    const site =
      sites === "only_listed"
        ? "Listed IPs through VPN"
        : sites === "except_listed"
          ? "Listed IPs bypass VPN"
          : "All traffic";
    if (!appsN) return site;
    return `${site} · ${appsN} app${appsN === 1 ? "" : "s"} bypass`;
  }

  async function onConnect() {
    if (!selected) {
      setSheet("add");
      setTab("servers");
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

  async function addBypass() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "App", extensions: ["exe"] }],
    });
    if (!picked || Array.isArray(picked)) return;
    try {
      const next = await invoke<SessionOpts>("add_bypass_app", { path: picked });
      setOpts(next);
    } catch (e) {
      setNote(String(e));
    }
  }

  async function removeBypass(path: string) {
    await saveOpts({
      ...opts,
      bypass_apps: (opts.bypass_apps ?? []).filter((a) => a.path !== path),
    });
  }

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
            <span className="tag">AmneziaWG</span>
          </header>

          <p className={`status is-${status.state}`}>{stateCopy(status.state)}</p>

          <ConnectRing
            state={status.state}
            disabled={busy}
            onClick={live ? onDisconnect : onConnect}
          />

          {status.state === "connected" && (
            <section className="live">
              <div className="hud-ping">
                <span className="n">{fmtRtt(status.rtt_ms)}</span>
                <span className="u">ms</span>
              </div>
              <Sparkline values={series} width={280} height={40} />
            </section>
          )}

          <button type="button" className="server-chip" onClick={() => setSheet("servers")}>
            <span className="server-chip-k">Server</span>
            <strong>{current?.name ?? "Add a config"}</strong>
            <em>{current?.endpoint ?? "Import an AmneziaWG .conf"}</em>
          </button>

          <p className="split-line">{splitLine()}</p>

          {status.error && <p className="fault">{status.error}</p>}
          {note && <p className="note">{note}</p>}
        </main>
      )}

      {tab === "servers" && (
        <main className="page">
          <header className="page-h">
            <h1>Servers</h1>
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

      {tab === "split" && (
        <main className="page">
          <header className="page-h">
            <h1>Split tunneling</h1>
          </header>
          <p className="hint">
            Sites: IPv4 only. Apps on Windows: selected apps work without VPN (Amnezia exceptions).
          </p>
          <h2 className="sub">Sites</h2>
          <div className="pills">
            {(
              [
                ["all", "All traffic"],
                ["only_listed", "Only listed through VPN"],
                ["except_listed", "Listed bypass VPN"],
              ] as const
            ).map(([id, name]) => (
              <button
                key={id}
                type="button"
                className={`pill ${opts.site_mode === id ? "on" : ""}`}
                onClick={() => setSiteMode(id)}
              >
                {name}
              </button>
            ))}
          </div>
          {opts.site_mode !== "all" && (
            <>
              <form className="cidr" onSubmit={addCidr}>
                <input
                  placeholder="1.1.1.0/24"
                  value={cidrDraft}
                  onChange={(e) => setCidrDraft(e.target.value)}
                />
                <button type="submit">Add</button>
              </form>
              <ul className="cidrs">
                {opts.split_sites.length === 0 && (
                  <li className="empty">No IPs yet</li>
                )}
                {opts.split_sites.map((c) => (
                  <li key={c}>
                    <code>{c}</code>
                    <button type="button" className="ghost" onClick={() => removeCidr(c)}>
                      Remove
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
          <h2 className="sub">Apps without VPN</h2>
          <button type="button" className="ghost" onClick={addBypass}>
            Add .exe
          </button>
          <p className="hint">Selected apps work without VPN. Everything else uses the tunnel.</p>
          <ul className="apps">
            {(opts.bypass_apps ?? []).length === 0 && (
              <li className="empty">Add an .exe that should bypass VPN</li>
            )}
            {(opts.bypass_apps ?? []).map((a) => (
              <li key={a.path} className="boosted">
                <div className="app-meta">
                  <strong>{a.name}</strong>
                  <em>{a.path}</em>
                </div>
                <button type="button" className="ghost" onClick={() => removeBypass(a.path)}>
                  Remove
                </button>
              </li>
            ))}
          </ul>
        </main>
      )}

      {tab === "settings" && (
        <main className="page">
          <header className="page-h">
            <h1>Connection</h1>
          </header>
          <ul className="rows">
            <li>
              <div>
                <strong>KillSwitch</strong>
                <em>Block the internet if the VPN drops. Manual disconnect does not block.</em>
              </div>
              <Toggle
                on={opts.kill_switch}
                onClick={() => saveOpts({ ...opts, kill_switch: !opts.kill_switch })}
              />
            </li>
            <li>
              <div>
                <strong>Clear leftover</strong>
                <em>If AGG crashed with KillSwitch on</em>
              </div>
              <button
                type="button"
                className="ghost-btn"
                onClick={() =>
                  invoke("kill_switch_off")
                    .then(() => setNote("Firewall leftovers cleared"))
                    .catch((e) => setNote(String(e)))
                }
              >
                Off
              </button>
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
                <strong>About</strong>
                <em>AGG · AmneziaWG client · no self-host</em>
              </div>
            </li>
          </ul>
        </main>
      )}

      <nav className="tabs four">
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
        <button type="button" className={tab === "split" ? "on" : ""} onClick={() => setTab("split")}>
          Split
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
                <h2>Add a config</h2>
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
        <em>or click to browse</em>
      </button>
      <p className="or">or paste it</p>
      <form onSubmit={onPaste} className="paste">
        <input
          placeholder="Name"
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
          Save
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
