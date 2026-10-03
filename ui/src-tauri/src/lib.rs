mod profiles;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use agg_core::ipc::{rpc, IpcRequest};
use agg_core::{
    run_udp_session, scan, windows_roots, ConnectionState, DetectedApp, SessionOpts, StatusSnapshot,
    WgConfig,
};
use profiles::{Library, Profile};
use tauri::{AppHandle, Emitter, Manager, State};

struct Session {
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct AppState {
    session: Mutex<Option<Session>>,
    last: Mutex<StatusSnapshot>,
    history: Mutex<VecDeque<u32>>,
    lib: Mutex<Option<Library>>,
    opts: Mutex<SessionOpts>,
    apps: Mutex<Vec<DetectedApp>>,
    tunneled: Mutex<Vec<String>>,
}

const HISTORY: usize = 60;

fn open_lib(app: &AppHandle, state: &AppState) -> Result<Library, String> {
    let mut g = state.lib.lock().map_err(|e| e.to_string())?;
    if g.is_none() {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("profiles");
        *g = Some(Library::open(dir)?);
    }
    Ok(g.as_ref().expect("library").clone())
}

fn emit_status(app: &AppHandle, state: &AppState, mut snap: StatusSnapshot) {
    if let Ok(mut hist) = state.history.lock() {
        if let Some(rtt) = snap.rtt_ms {
            hist.push_back(rtt);
            while hist.len() > HISTORY {
                hist.pop_front();
            }
        }
        snap.rtt_history = hist.iter().copied().collect();
    }
    if let Ok(mut g) = state.last.lock() {
        *g = snap.clone();
    }
    let _ = app.emit("status", &snap);
}

#[tauri::command]
fn filter_status() -> agg_platform_windows::FilterStatus {
    agg_platform_windows::detect_filter()
}

#[tauri::command]
fn kill_switch_off() -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = rpc(&IpcRequest::KillSwitchOff);
        agg_platform_windows::wfp::KillSwitch::disarm_leftovers();
    }
    Ok(())
}

#[tauri::command]
fn helper_ok() -> Result<(), String> {
    #[cfg(windows)]
    {
        let r = rpc(&IpcRequest::Ping)?;
        if r.ok {
            Ok(())
        } else {
            Err(r.error.unwrap_or_else(|| "tunnel helper not ready".into()))
        }
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> StatusSnapshot {
    state
        .last
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| StatusSnapshot::idle())
}

#[tauri::command]
fn list_profiles(app: AppHandle, state: State<AppState>) -> Result<Vec<Profile>, String> {
    open_lib(&app, &state)?.list()
}

#[tauri::command]
fn active_profile(app: AppHandle, state: State<AppState>) -> Result<Option<String>, String> {
    open_lib(&app, &state)?.active_id()
}

#[tauri::command]
fn import_files(
    app: AppHandle,
    state: State<AppState>,
    paths: Vec<String>,
) -> Result<Vec<Profile>, String> {
    let lib = open_lib(&app, &state)?;
    let mut out = Vec::new();
    for p in paths {
        out.push(lib.import_file(std::path::Path::new(&p))?);
    }
    Ok(out)
}

#[tauri::command]
fn import_text(
    app: AppHandle,
    state: State<AppState>,
    name: Option<String>,
    body: String,
) -> Result<Profile, String> {
    open_lib(&app, &state)?.import_text(name, &body, "paste")
}

#[tauri::command]
fn rename_profile(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<Profile, String> {
    open_lib(&app, &state)?.rename(&id, &name)
}

#[tauri::command]
fn delete_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    open_lib(&app, &state)?.remove(&id)
}

#[tauri::command]
fn select_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    open_lib(&app, &state)?.set_active(Some(id))
}

fn apps_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn load_json<T: serde::de::DeserializeOwned>(path: &std::path::Path, fallback: T) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<T>(&s).ok())
        .unwrap_or(fallback)
}

fn save_json<T: serde::Serialize>(path: &std::path::Path, v: &T) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = std::fs::write(path, s);
    }
}

#[tauri::command]
fn list_apps(state: State<AppState>) -> Vec<DetectedApp> {
    state.apps.lock().map(|g| g.clone()).unwrap_or_default()
}

#[tauri::command]
fn tunneled_ids(state: State<AppState>) -> Vec<String> {
    state.tunneled.lock().map(|g| g.clone()).unwrap_or_default()
}

#[tauri::command]
fn set_tunneled(app: AppHandle, state: State<AppState>, ids: Vec<String>) -> Result<Vec<String>, String> {
    if let Ok(mut g) = state.tunneled.lock() {
        *g = ids.clone();
    }
    if let Ok(dir) = apps_dir(&app) {
        save_json(&dir.join("tunneled_apps.json"), &ids);
    }
    Ok(ids)
}

#[tauri::command]
fn add_manual_app(
    app: AppHandle,
    state: State<AppState>,
    path: String,
) -> Result<DetectedApp, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err("file not found".into());
    }
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();
    let entry = DetectedApp {
        id: format!("manual:{}", p.display()),
        name,
        source: agg_core::AppSource::Manual,
        executable: p.clone(),
        install_dir: p.parent().unwrap_or(p.as_path()).to_path_buf(),
        icon_path: None,
        missing: false,
    };
    if let Ok(mut g) = state.apps.lock() {
        if !g.iter().any(|a| a.id == entry.id) {
            g.push(entry.clone());
            if let Ok(dir) = apps_dir(&app) {
                save_json(&dir.join("detected_apps.json"), &*g);
            }
        }
    }
    Ok(entry)
}

#[tauri::command]
fn refresh_apps(app: AppHandle, state: State<AppState>) -> Result<Vec<DetectedApp>, String> {
    let mut found = scan(&windows_roots());
    agg_core::discovery::mark_missing(&mut found);
    if let Ok(dir) = apps_dir(&app) {
        let cached: Vec<DetectedApp> = load_json(&dir.join("detected_apps.json"), Vec::new());
        for c in cached {
            if c.source == agg_core::AppSource::Manual && !found.iter().any(|a| a.id == c.id) {
                found.push(c);
            }
        }
        agg_core::discovery::mark_missing(&mut found);
        save_json(&dir.join("detected_apps.json"), &found);
    }
    if let Ok(mut g) = state.apps.lock() {
        *g = found.clone();
    }
    Ok(found)
}

#[tauri::command]
fn get_opts(state: State<AppState>) -> SessionOpts {
    state
        .opts
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
        .sanitized()
}

#[tauri::command]
fn set_opts(app: AppHandle, state: State<AppState>, opts: SessionOpts) -> Result<SessionOpts, String> {
    let opts = opts.sanitized();
    if let Ok(mut g) = state.opts.lock() {
        *g = opts.clone();
    }
    save_opts(&app, &opts);
    Ok(opts)
}

fn opts_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("opts.json"))
}

fn load_opts(app: &AppHandle) -> SessionOpts {
    let Some(path) = opts_path(app) else {
        return SessionOpts::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<SessionOpts>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

fn save_opts(app: &AppHandle, opts: &SessionOpts) {
    if let Some(path) = opts_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, serde_json::to_string(opts).unwrap_or_default());
    }
}

#[tauri::command]
fn connect(app: AppHandle, state: State<AppState>, profile_id: String) -> Result<(), String> {
    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    if let Some(s) = slot.as_ref() {
        if s.running.load(Ordering::SeqCst) {
            return Err("already connected".into());
        }
    }

    let lib = open_lib(&app, &state)?;
    let path = lib.conf_path(&profile_id);
    if !path.exists() {
        return Err("profile config missing on disk".into());
    }
    lib.set_active(Some(profile_id.clone()))?;
    let cfg = WgConfig::from_path(&path).map_err(|e| e.to_string())?;
    let name = lib
        .list()?
        .into_iter()
        .find(|p| p.id == profile_id)
        .map(|p| p.name);
    let endpoint = cfg
        .peer()
        .ok()
        .and_then(|p| p.endpoint)
        .map(|e| e.to_string());

    let tunneled = state
        .tunneled
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let apps = state.apps.lock().map(|g| g.clone()).unwrap_or_default();
    let mut opts = state
        .opts
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
        .sanitized();
    opts.boosted_apps = tunneled.clone();
    opts.direct_exes = if tunneled.is_empty() {
        Vec::new()
    } else {
        apps.iter()
            .filter(|a| !tunneled.contains(&a.id) && !a.missing)
            .map(|a| a.executable.display().to_string())
            .collect()
    };
    let mut connecting = StatusSnapshot::connecting(endpoint.clone());
    connecting.server = name.clone();
    connecting.profile_id = Some(profile_id.clone());
    connecting.opts = opts.clone();
    emit_status(&app, &state, connecting);
    if let Ok(mut h) = state.history.lock() {
        h.clear();
    }

    let running = Arc::new(AtomicBool::new(true));
    let run = running.clone();
    let app2 = app.clone();
    let conf_path = path.display().to_string();
    let worker = std::thread::spawn(move || {
        if cfg!(windows) {
            match rpc(&IpcRequest::Connect {
                path: conf_path,
                opts,
            }) {
                Err(e) => {
                    let _ = app2.emit("status", StatusSnapshot::failed(e));
                    return;
                }
                Ok(r) if !r.ok => {
                    let _ = app2.emit(
                        "status",
                        StatusSnapshot::failed(r.error.unwrap_or_else(|| "connect failed".into())),
                    );
                    return;
                }
                Ok(_) => {}
            }
            while run.load(Ordering::SeqCst) {
                match rpc(&IpcRequest::Status) {
                    Ok(r) => {
                        if let Some(mut snap) = r.status {
                            snap.server = name.clone();
                            snap.profile_id = Some(profile_id.clone());
                            let _ = app2.emit("status", &snap);
                            if matches!(
                                snap.state,
                                ConnectionState::Idle | ConnectionState::Error
                            ) {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = app2.emit("status", StatusSnapshot::failed(e));
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            return;
        }

        let on_stats = {
            let app2 = app2.clone();
            let endpoint = endpoint.clone();
            let name = name.clone();
            let pid = profile_id.clone();
            move |stats| {
                let mut snap = StatusSnapshot::from_stats(endpoint.clone(), stats);
                snap.server = name.clone();
                snap.profile_id = Some(pid.clone());
                let _ = app2.emit("status", &snap);
            }
        };
        let result = run_udp_session(&cfg, &run, on_stats).map_err(|e| e.to_string());
        if let Err(e) = result {
            if run.load(Ordering::SeqCst) {
                let _ = app2.emit("status", StatusSnapshot::failed(e));
                return;
            }
        }
        let _ = app2.emit("status", StatusSnapshot::idle());
    });

    *slot = Some(Session {
        running,
        worker: Some(worker),
    });
    Ok(())
}

#[tauri::command]
fn disconnect(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    emit_status(
        &app,
        &state,
        StatusSnapshot {
            state: ConnectionState::Disconnecting,
            ..StatusSnapshot::idle()
        },
    );

    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    if let Some(s) = slot.as_mut() {
        s.running.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        {
            let _ = rpc(&IpcRequest::Disconnect);
        }
        if let Some(h) = s.worker.take() {
            let _ = h.join();
        }
    }
    *slot = None;
    emit_status(&app, &state, StatusSnapshot::idle());
    Ok(())
}

fn fit_to_monitor(win: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = win.current_monitor() else {
        let _ = win.center();
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let work_w = area.size.width as f64 / scale;
    let work_h = area.size.height as f64 / scale;
    let width = 400.0_f64.min(work_w * 0.92).max(360.0);
    let height = (work_h * 0.86).min(work_h - 32.0).clamp(520.0, 900.0);
    let _ = win.set_min_size(Some(tauri::LogicalSize::new(360.0, 520.0)));
    let _ = win.set_size(tauri::LogicalSize::new(width, height));
    let _ = win.center();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("agg_ui=info".parse().unwrap()),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            session: Mutex::new(None),
            last: Mutex::new(StatusSnapshot::idle()),
            history: Mutex::new(VecDeque::new()),
            lib: Mutex::new(None),
            opts: Mutex::new(SessionOpts::default()),
            apps: Mutex::new(Vec::new()),
            tunneled: Mutex::new(Vec::new()),
        })
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                fit_to_monitor(&win);
            }
            let handle = app.handle().clone();
            let opts = load_opts(&handle);
            if let Ok(mut g) = app.state::<AppState>().opts.lock() {
                *g = opts;
            }
            if let Ok(dir) = apps_dir(&handle) {
                let mut found: Vec<DetectedApp> =
                    load_json(&dir.join("detected_apps.json"), Vec::new());
                if found.is_empty() {
                    found = scan(&windows_roots());
                    agg_core::discovery::mark_missing(&mut found);
                    save_json(&dir.join("detected_apps.json"), &found);
                } else {
                    agg_core::discovery::mark_missing(&mut found);
                }
                if let Ok(mut g) = app.state::<AppState>().apps.lock() {
                    *g = found;
                }
                let tun: Vec<String> = load_json(&dir.join("tunneled_apps.json"), Vec::new());
                if let Ok(mut g) = app.state::<AppState>().tunneled.lock() {
                    *g = tun;
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            helper_ok,
            filter_status,
            kill_switch_off,
            list_profiles,
            active_profile,
            import_files,
            import_text,
            rename_profile,
            delete_profile,
            select_profile,
            get_opts,
            set_opts,
            list_apps,
            tunneled_ids,
            set_tunneled,
            add_manual_app,
            refresh_apps,
            connect,
            disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
