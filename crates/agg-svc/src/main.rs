//! AGG Windows service. Owns Wintun. The desktop UI is unprivileged.

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("install") => {
            #[cfg(windows)]
            match windows_impl::install() {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!("install is Windows-only");
                std::process::exit(1);
            }
        }
        Some("uninstall") => {
            #[cfg(windows)]
            match windows_impl::uninstall() {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!("uninstall is Windows-only");
                std::process::exit(1);
            }
        }
        Some("console") => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter("agg_svc=info")
                .try_init();
            #[cfg(windows)]
            windows_impl::run_loop();
            #[cfg(not(windows))]
            eprintln!("console mode is Windows-only");
        }
        _ => {
            #[cfg(windows)]
            {
                if let Err(e) = windows_impl::dispatch() {
                    eprintln!("service dispatch: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            eprintln!("agg-svc is the Windows helper. On Linux use agg-cli.");
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::io::FromRawHandle;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use agg_core::ipc::{IpcRequest, IpcResponse, PIPE_NAME, SERVICE_NAME};
    use agg_core::{StatusSnapshot, WgConfig};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_service::service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    windows_service::define_windows_service!(ffi_service_main, service_main);

    struct Tunnel {
        running: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    pub fn dispatch() -> Result<(), Box<dyn std::error::Error>> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
        Ok(())
    }

    fn service_main(_args: Vec<std::ffi::OsString>) {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("agg_svc=info")
            .try_init();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let status_handle = match service_control_handler::register(SERVICE_NAME, move |event| {
            match event {
                ServiceControl::Stop => {
                    stop2.store(true, Ordering::SeqCst);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        }) {
            Ok(h) => h,
            Err(_) => return,
        };
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        });
        run_loop_inner(stop);
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        });
    }

    pub fn run_loop() {
        run_loop_inner(Arc::new(AtomicBool::new(false)));
    }

    fn run_loop_inner(stop: Arc<AtomicBool>) {
        let last = Arc::new(Mutex::new(StatusSnapshot::idle()));
        let tun = Arc::new(Mutex::new(None::<Tunnel>));
        tracing::info!("AGG service listening on {PIPE_NAME}");
        while !stop.load(Ordering::SeqCst) {
            match accept_client() {
                Ok(stream) => {
                    let last = last.clone();
                    let tun = tun.clone();
                    std::thread::spawn(move || handle_client(stream, last, tun));
                }
                Err(e) => {
                    tracing::debug!("pipe accept: {e}");
                    std::thread::sleep(Duration::from_millis(80));
                }
            }
        }
        stop_tunnel(&tun);
    }

    fn stop_tunnel(tun: &Mutex<Option<Tunnel>>) {
        if let Ok(mut g) = tun.lock() {
            if let Some(t) = g.as_mut() {
                t.running.store(false, Ordering::SeqCst);
                let _ = agg_platform_windows::wintun_down();
                if let Some(h) = t.worker.take() {
                    let _ = h.join();
                }
            }
            *g = None;
        }
    }

    fn handle_client(
        stream: std::fs::File,
        last: Arc<Mutex<StatusSnapshot>>,
        tun: Arc<Mutex<Option<Tunnel>>>,
    ) {
        let mut writer = match stream.try_clone() {
            Ok(w) => w,
            Err(_) => return,
        };
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            let req: IpcRequest = match serde_json::from_str(line.trim()) {
                Ok(r) => r,
                Err(e) => {
                    let _ = write_resp(
                        &mut writer,
                        IpcResponse {
                            ok: false,
                            error: Some(e.to_string()),
                            status: None,
                        },
                    );
                    continue;
                }
            };
            let resp = dispatch(req, &last, &tun);
            if write_resp(&mut writer, resp).is_err() {
                break;
            }
        }
    }

    fn write_resp(w: &mut std::fs::File, resp: IpcResponse) -> std::io::Result<()> {
        let line = serde_json::to_string(&resp).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"encode"}"#.into()
        });
        writeln!(w, "{line}")
    }

    fn dispatch(
        req: IpcRequest,
        last: &Arc<Mutex<StatusSnapshot>>,
        tun: &Arc<Mutex<Option<Tunnel>>>,
    ) -> IpcResponse {
        match req {
            IpcRequest::Ping | IpcRequest::Status => IpcResponse {
                ok: true,
                error: None,
                status: last.lock().ok().map(|g| g.clone()),
            },
            IpcRequest::Disconnect => {
                stop_tunnel(tun);
                if let Ok(mut s) = last.lock() {
                    *s = StatusSnapshot::idle();
                }
                IpcResponse {
                    ok: true,
                    error: None,
                    status: Some(StatusSnapshot::idle()),
                }
            }
            IpcRequest::Connect { path } => connect(&path, last, tun),
        }
    }

    fn connect(
        path: &str,
        last: &Arc<Mutex<StatusSnapshot>>,
        tun: &Arc<Mutex<Option<Tunnel>>>,
    ) -> IpcResponse {
        if let Ok(g) = tun.lock() {
            if g.as_ref()
                .map(|t| t.running.load(Ordering::SeqCst))
                .unwrap_or(false)
            {
                return IpcResponse {
                    ok: false,
                    error: Some("already connected".into()),
                    status: last.lock().ok().map(|s| s.clone()),
                };
            }
        }
        let cfg = match WgConfig::from_path(std::path::Path::new(path)) {
            Ok(c) => c,
            Err(e) => {
                return IpcResponse {
                    ok: false,
                    error: Some(e.to_string()),
                    status: None,
                };
            }
        };
        let endpoint = cfg
            .peer()
            .ok()
            .and_then(|p| p.endpoint)
            .map(|e| e.to_string());
        if let Ok(mut s) = last.lock() {
            *s = StatusSnapshot::connecting(endpoint.clone());
        }
        let running = Arc::new(AtomicBool::new(true));
        let run = running.clone();
        let last2 = last.clone();
        let worker = std::thread::spawn(move || {
            let on_stats = {
                let last2 = last2.clone();
                let endpoint = endpoint.clone();
                move |stats| {
                    let snap = StatusSnapshot::from_stats(endpoint.clone(), stats);
                    if let Ok(mut g) = last2.lock() {
                        *g = snap;
                    }
                }
            };
            match agg_platform_windows::wintun_up_with_stats(&cfg, &run, on_stats) {
                Err(e) if run.load(Ordering::SeqCst) => {
                    if let Ok(mut g) = last2.lock() {
                        *g = StatusSnapshot::failed(e.to_string());
                    }
                }
                _ => {
                    if run.load(Ordering::SeqCst) {
                        if let Ok(mut g) = last2.lock() {
                            *g = StatusSnapshot::idle();
                        }
                    }
                }
            }
        });
        if let Ok(mut g) = tun.lock() {
            *g = Some(Tunnel {
                running,
                worker: Some(worker),
            });
        }
        IpcResponse {
            ok: true,
            error: None,
            status: last.lock().ok().map(|s| s.clone()),
        }
    }

    fn accept_client() -> Result<std::fs::File, String> {
        let handle = unsafe { create_pipe() }.map_err(|e| e.to_string())?;
        if unsafe { ConnectNamedPipe(handle, None) }.is_err() {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err("ConnectNamedPipe".into());
        }
        Ok(unsafe { std::fs::File::from_raw_handle(handle.0 as *mut std::ffi::c_void) })
    }

    unsafe fn create_pipe() -> windows::core::Result<HANDLE> {
        let mut name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let sddl: Vec<u16> = "D:(A;;GA;;;BA)(A;;GA;;;SY)(A;;GRGW;;;AU)\0"
            .encode_utf16()
            .collect();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        CreateNamedPipeW(
            PCWSTR(name.as_mut_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            Some(&sa),
        )
    }

    pub fn install() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CREATE_SERVICE,
        )
        .map_err(|e| e.to_string())?;
        let info = ServiceInfo {
            name: SERVICE_NAME.into(),
            display_name: "AGG Tunnel".into(),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::Auto,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe,
            launch_arguments: vec![],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        let service = manager
            .create_service(&info, ServiceAccess::START | ServiceAccess::CHANGE_CONFIG)
            .map_err(|e| e.to_string())?;
        service
            .start::<std::ffi::OsString>(&[])
            .map_err(|e| e.to_string())?;
        println!("installed and started {SERVICE_NAME}");
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
                .map_err(|e| e.to_string())?;
        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE)
            .map_err(|e| e.to_string())?;
        let _ = service.stop();
        service.delete().map_err(|e| e.to_string())?;
        println!("removed {SERVICE_NAME}");
        Ok(())
    }
}
