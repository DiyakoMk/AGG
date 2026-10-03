mod profiles;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use agg_core::ipc::{rpc, IpcRequest};
use agg_core::{run_udp_session, ConnectionState, StatusSnapshot, WgConfig};
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

    let mut connecting = StatusSnapshot::connecting(endpoint.clone());
    connecting.server = name.clone();
    connecting.profile_id = Some(profile_id.clone());
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
            match rpc(&IpcRequest::Connect { path: conf_path }) {
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
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            helper_ok,
            list_profiles,
            active_profile,
            import_files,
            import_text,
            rename_profile,
            delete_profile,
            connect,
            disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
