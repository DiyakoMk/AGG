//! Live WinpkFilter session. Restores adapter mode on Drop.

use std::sync::atomic::{AtomicBool, Ordering};

use ndisapi::{
    DirectionFlags, EthMRequestMut, FilterFlags, IntermediateBuffer, MacAddress, Ndisapi,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};

use agg_core::WgConfig;

use super::{AdapterInfo, DriverStatus, PlatformError, DRIVER_DOWNLOAD, DRIVER_NAME};

const BATCH: usize = 64;
const WAIT_MS: u32 = 250;

pub fn detect() -> Result<DriverStatus, PlatformError> {
    let driver = match Ndisapi::new(DRIVER_NAME) {
        Ok(d) => d,
        Err(_) => {
            return Ok(DriverStatus {
                present: false,
                version: None,
                adapters: Vec::new(),
                hint: format!(
                    "Windows Packet Filter is not installed. Personal/non-commercial: {DRIVER_DOWNLOAD}"
                ),
            });
        }
    };
    let version = driver
        .get_version()
        .map(|v| format!("{v:?}"))
        .unwrap_or_else(|_| "unknown".into());
    let adapters = list_from(&driver)?;
    Ok(DriverStatus {
        present: true,
        version: Some(version),
        adapters,
        hint: "NDISRD present. No virtual adapter is created.".into(),
    })
}

pub fn list_adapters() -> Result<Vec<AdapterInfo>, PlatformError> {
    let driver = Ndisapi::new(DRIVER_NAME).map_err(|_| PlatformError::DriverMissing)?;
    list_from(&driver)
}

fn list_from(driver: &Ndisapi) -> Result<Vec<AdapterInfo>, PlatformError> {
    let adapters = driver
        .get_tcpip_bound_adapters_info()
        .map_err(|e| PlatformError::msg(format!("enumerate adapters: {e}")))?;
    Ok(adapters
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = a.get_name().to_string();
            let friendly = Ndisapi::get_friendly_adapter_name(a.get_name()).unwrap_or_else(|_| name.clone());
            let mac = MacAddress::from_slice(a.get_hw_address())
                .map(|m| format!("{m:?}"))
                .unwrap_or_else(|| "??".into());
            AdapterInfo {
                index: i + 1,
                name,
                friendly,
                mac,
                mtu: a.get_mtu(),
            }
        })
        .collect())
}

/// Capture one redirected packet, log it, drop it, restore filter mode.
///
/// Uses the same bulk `read_packets` path as `packthru`. Single-packet
/// `read_packet` returns ERROR_INVALID_PARAMETER (0x80070057) on an empty
/// queue, which is what you get if WaitForSingleObject timed out.
pub fn capture_one(index: usize) -> Result<String, PlatformError> {
    let session = Session::open(index)?;
    let mut packets: Vec<IntermediateBuffer> = vec![Default::default(); BATCH];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);

    while std::time::Instant::now() < deadline {
        session.wait(WAIT_MS);
        if let Some(pkt) = session.read_first(&mut packets) {
            let dir = if pkt.get_device_flags() == DirectionFlags::PACKET_FLAG_ON_SEND {
                "SEND"
            } else {
                "RECV"
            };
            let len = pkt.get_length();
            let preview: String = pkt
                .get_data()
                .iter()
                .take(32)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            return Ok(format!(
                "captured {dir} {len} bytes (dropped, not reinjected)\n{preview}"
            ));
        }
        session.reset_event();
    }

    Err(PlatformError::msg(
        "no packet in 15s on this adapter — try the other of Ethernet (1) vs Main/Wi-Fi (9), as Administrator, with some traffic",
    ))
}

/// LISTEN-only. Drains copies. **Never** `send_packet*` — homemade
/// `IntermediateBuffer`s bugcheck WinpkFilter (machine restart).
/// Handshake belongs on a normal UDP socket (`agg-cli handshake` / `up` in CLI).
pub fn intercept_loop(
    index: usize,
    _cfg: &WgConfig,
    running: &AtomicBool,
) -> Result<(), PlatformError> {
    let _ = restore_all_adapters();
    let session = Session::open(index)?;
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("panic in intercept: {info}");
        let _ = restore_all_adapters();
    }));

    let mut packets: Vec<IntermediateBuffer> = vec![Default::default(); BATCH];
    let mut seen: u64 = 0;
    let mut last_log = std::time::Instant::now();

    tracing::info!("NDIS LISTEN drain on adapter {index}; originals still flow; no inject");

    while running.load(Ordering::SeqCst) {
        session.wait(WAIT_MS);
        loop {
            let n = session.read_batch(&mut packets);
            if n == 0 {
                break;
            }
            seen += n as u64;
        }
        session.reset_event();
        if last_log.elapsed() >= std::time::Duration::from_secs(1) {
            last_log = std::time::Instant::now();
            tracing::info!(seen, "ndis listen (pass-through copies, not tunneling)");
        }
    }
    Ok(())
}

struct Session {
    driver: Ndisapi,
    handle: HANDLE,
    event: HANDLE,
}

impl Session {
    fn open(index: usize) -> Result<Self, PlatformError> {
        if index == 0 {
            return Err(PlatformError::msg("interface index is 1-based"));
        }
        let driver = Ndisapi::new(DRIVER_NAME).map_err(|_| PlatformError::DriverMissing)?;
        let adapters = driver
            .get_tcpip_bound_adapters_info()
            .map_err(|e| PlatformError::msg(e.to_string()))?;
        if index > adapters.len() {
            return Err(PlatformError::msg(format!(
                "interface {index} out of range (have {})",
                adapters.len()
            )));
        }
        let handle = adapters[index - 1].get_handle();
        let event = unsafe {
            CreateEventW(None, true, false, None)
                .map_err(|e| PlatformError::msg(format!("CreateEventW: {e}")))?
        };
        driver
            .set_packet_event(handle, event)
            .map_err(|e| PlatformError::msg(format!("set_packet_event: {e}")))?;
        // LISTEN: original packets still go to the stack. TUNNEL drops them
        // unless we re-inject — that blackholed the box before handshake.
        driver
            .set_adapter_mode(handle, FilterFlags::MSTCP_FLAG_SENT_RECEIVE_LISTEN)
            .map_err(|e| PlatformError::msg(format!("set_adapter_mode: {e}")))?;
        Ok(Self {
            driver,
            handle,
            event,
        })
    }

    fn wait(&self, ms: u32) {
        let _ = unsafe { WaitForSingleObject(self.event, ms) };
    }

    fn read_batch(&self, packets: &mut [IntermediateBuffer]) -> usize {
        let mut to_read = EthMRequestMut::from_iter(self.handle, packets.iter_mut());
        self.driver.read_packets::<BATCH>(&mut to_read).unwrap_or(0)
    }

    fn read_first(&self, packets: &mut [IntermediateBuffer]) -> Option<IntermediateBuffer> {
        let n = self.read_batch(packets);
        if n == 0 {
            None
        } else {
            Some(packets[0].clone())
        }
    }

    fn reset_event(&self) {
        let _ = unsafe { ResetEvent(self.event) };
    }
}

/// Put every TCP/IP-bound adapter back in default (pass-through) filter mode.
/// Escape hatch if `intercept` was killed before Drop ran.
pub fn restore_all_adapters() -> Result<usize, PlatformError> {
    let driver = Ndisapi::new(DRIVER_NAME).map_err(|_| PlatformError::DriverMissing)?;
    let adapters = driver
        .get_tcpip_bound_adapters_info()
        .map_err(|e| PlatformError::msg(e.to_string()))?;
    let mut n = 0usize;
    for a in &adapters {
        if driver
            .set_adapter_mode(a.get_handle(), FilterFlags::default())
            .is_ok()
        {
            n += 1;
        }
    }
    Ok(n)
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self
            .driver
            .set_adapter_mode(self.handle, FilterFlags::default());
        let _ = unsafe { CloseHandle(self.event) };
    }
}
