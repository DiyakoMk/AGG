use serde::{Deserialize, Serialize};

use crate::status::StatusSnapshot;

pub const PIPE_NAME: &str = r"\\.\pipe\AGGService";
pub const SERVICE_NAME: &str = "AGGService";

/// Talk to AGGService. Windows only; other OS returns Err.
pub fn rpc(req: &IpcRequest) -> Result<IpcResponse, String> {
    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        use std::io::{BufRead, BufReader, Write};
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(PIPE_NAME)
            .map_err(|_| {
                "tunnel helper is not running — reinstall AGG, or as Administrator: agg-svc.exe install".to_string()
            })?;
        let line = serde_json::to_string(req).map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(f);
        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;
        serde_json::from_str(resp.trim()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = req;
        Err("named pipe IPC is Windows-only".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    Connect { path: String },
    Disconnect,
    Status,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub status: Option<StatusSnapshot>,
}
