use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use agg_core::{run_udp_session, ConnectionState, StatusSnapshot, WgConfig};
use tauri::{AppHandle, Emitter, State};

struct Session {
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct AppState {
    session: Mutex<Option<Session>>,
    last: Mutex<StatusSnapshot>,
}

fn emit_status(app: &AppHandle, state: &AppState, snap: StatusSnapshot) {
    if let Ok(mut g) = state.last.lock() {
        *g = snap.clone();
    }
    let _ = app.emit("status", &snap);
}

fn resolve_config(config_path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(config_path);
    if p.exists() {
        return Ok(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(config_path);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "config not found: {config_path} (tried cwd and exe directory)"
    ))
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
fn connect(app: AppHandle, state: State<AppState>, config_path: String) -> Result<(), String> {
    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    if let Some(s) = slot.as_ref() {
        if s.running.load(Ordering::SeqCst) {
            return Err("already connected".into());
        }
    }

    let path = resolve_config(&config_path)?;
    let cfg = WgConfig::from_path(&path).map_err(|e| e.to_string())?;
    let endpoint = cfg
        .peer()
        .ok()
        .and_then(|p| p.endpoint)
        .map(|e| e.to_string());

    emit_status(&app, &state, StatusSnapshot::connecting(endpoint.clone()));

    let running = Arc::new(AtomicBool::new(true));
    let run = running.clone();
    let app2 = app.clone();
    let worker = std::thread::spawn(move || {
        let on_stats = {
            let app2 = app2.clone();
            let endpoint = endpoint.clone();
            move |stats| {
                let snap = StatusSnapshot::from_stats(endpoint.clone(), stats);
                let _ = app2.emit("status", &snap);
            }
        };

        let result = if cfg!(windows) {
            agg_platform_windows::wintun_up_with_stats(&cfg, &run, on_stats)
                .map_err(|e| e.to_string())
        } else {
            run_udp_session(&cfg, &run, on_stats).map_err(|e| e.to_string())
        };

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
            let _ = agg_platform_windows::wintun_down();
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
        .manage(AppState {
            session: Mutex::new(None),
            last: Mutex::new(StatusSnapshot::idle()),
        })
        .invoke_handler(tauri::generate_handler![get_status, connect, disconnect])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
