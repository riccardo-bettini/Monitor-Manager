use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use windows_sys::Win32::Devices::Display::{
    DestroyPhysicalMonitors, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
    GetVCPFeatureAndVCPFeatureReply, QueryDisplayConfig, SetVCPFeature,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, PHYSICAL_MONITOR, QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Foundation::{BOOL, HANDLE, LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};

// Codici VESA MCCS VCP standard per la v1.00
pub const VCP_BRIGHTNESS: u8 = 0x10;
pub const VCP_CONTRAST: u8 = 0x12;
pub const VCP_COLOR_TEMP: u8 = 0x14;
pub const VCP_RED_GAIN: u8 = 0x16;
pub const VCP_GREEN_GAIN: u8 = 0x18;
pub const VCP_BLUE_GAIN: u8 = 0x1A;
pub const VCP_INPUT_SOURCE: u8 = 0x60;
pub const VCP_VOLUME: u8 = 0x62;
pub const VCP_SHARPNESS: u8 = 0x87;
pub const VCP_SATURATION: u8 = 0x8A;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcpFeatureState {
    pub code: u8,
    pub supported: bool,
    pub current: u32,
    pub max: u32,
}

impl VcpFeatureState {
    pub fn unsupported(code: u8, default_max: u32) -> Self {
        Self {
            code,
            supported: false,
            current: 50.min(default_max),
            max: default_max,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: usize,
    pub name: String,
    pub brand: String,
    pub model_code: String,
    pub gdi_device: String,
    pub resolution: String,
    pub is_primary: bool,
    pub ddc_supported: bool,
    pub brightness: VcpFeatureState,
    pub contrast: VcpFeatureState,
    pub saturation: VcpFeatureState,
    pub sharpness: VcpFeatureState,
    pub red_gain: VcpFeatureState,
    pub green_gain: VcpFeatureState,
    pub blue_gain: VcpFeatureState,
    pub color_temp: VcpFeatureState,
    pub volume: VcpFeatureState,
    pub input_source: VcpFeatureState,
}

struct PhysicalHandleWrap(HANDLE);
unsafe impl Send for PhysicalHandleWrap {}
unsafe impl Sync for PhysicalHandleWrap {}

struct ManagedMonitor {
    pub info: MonitorInfo,
    pub handle: PhysicalHandleWrap,
}

impl Drop for ManagedMonitor {
    fn drop(&mut self) {
        unsafe {
            let pm = PHYSICAL_MONITOR {
                hPhysicalMonitor: self.handle.0,
                szPhysicalMonitorDescription: [0; 128],
            };
            let _ = DestroyPhysicalMonitors(1, &pm);
        }
    }
}

/// Coda con coalescenza per applicare le modifiche VCP in tempo reale senza accumulare lag I2C
struct CoalescedQueue {
    /// Mappa (monitor_id, vcp_code) -> valore_piu_recente
    pending: HashMap<(usize, u8), u32>,
    shutdown: bool,
}

pub struct DdcEngine {
    monitors: Arc<Mutex<Vec<ManagedMonitor>>>,
    queue: Arc<(Mutex<CoalescedQueue>, Condvar)>,
}

impl DdcEngine {
    pub fn new() -> Self {
        let monitors: Arc<Mutex<Vec<ManagedMonitor>>> = Arc::new(Mutex::new(Vec::new()));
        let queue = Arc::new((
            Mutex::new(CoalescedQueue {
                pending: HashMap::new(),
                shutdown: false,
            }),
            Condvar::new(),
        ));

        let engine = Self {
            monitors: Arc::clone(&monitors),
            queue: Arc::clone(&queue),
        };

        engine.spawn_worker();
        engine
    }

    fn spawn_worker(&self) {
        let monitors_ref = Arc::clone(&self.monitors);
        let queue_ref = Arc::clone(&self.queue);

        thread::spawn(move || loop {
            let next_batch: Vec<((usize, u8), u32)> = {
                let (lock, cvar) = &*queue_ref;
                let mut q = lock.lock().unwrap();
                while q.pending.is_empty() && !q.shutdown {
                    q = cvar.wait(q).unwrap();
                }
                if q.shutdown {
                    return;
                }
                q.pending.drain().collect()
            };

            for ((monitor_id, vcp_code), value) in next_batch {
                // Prendiamo l'handle senza bloccare la UI durante la chiamata I2C
                let raw_handle = {
                    let guard = monitors_ref.lock().unwrap();
                    guard
                        .iter()
                        .find(|m| m.info.id == monitor_id)
                        .map(|m| m.handle.0 as isize)
                };

                if let Some(h_int) = raw_handle {
                    unsafe {
                        let _ = SetVCPFeature(h_int as HANDLE, vcp_code, value);
                    }
                }
            }
        });
    }

    /// Scansiona tutti i monitor fisici collegati e legge i loro valori VCP attuali
    pub fn scan_monitors(&self) -> Vec<MonitorInfo> {
        let discovered = enumerate_all_monitors();
        let mut guard = self.monitors.lock().unwrap();
        *guard = discovered;
        guard.iter().map(|m| m.info.clone()).collect()
    }

    /// Restituisce l'elenco dei monitor attualmente in cache senza attendere il bus I2C
    #[allow(dead_code)]
    pub fn get_cached_monitors(&self) -> Vec<MonitorInfo> {
        let guard = self.monitors.lock().unwrap();
        guard.iter().map(|m| m.info.clone()).collect()
    }

    /// Imposta in modo istantaneo (con coalescing) un valore VCP su un monitor (o su tutti se sync è attivo)
    pub fn set_vcp_value(&self, monitor_id: usize, vcp_code: u8, value: u32, sync_all: bool) {
        let target_ids: Vec<usize> = {
            let mut guard = self.monitors.lock().unwrap();
            let mut ids = Vec::new();
            for m in guard.iter_mut() {
                if sync_all || m.info.id == monitor_id {
                    update_cached_vcp(&mut m.info, vcp_code, value);
                    ids.push(m.info.id);
                }
            }
            ids
        };

        let (lock, cvar) = &*self.queue;
        let mut q = lock.lock().unwrap();
        for id in target_ids {
            q.pending.insert((id, vcp_code), value);
        }
        cvar.notify_one();
    }
}

fn update_cached_vcp(info: &mut MonitorInfo, vcp_code: u8, value: u32) {
    let feature = match vcp_code {
        VCP_BRIGHTNESS => &mut info.brightness,
        VCP_CONTRAST => &mut info.contrast,
        VCP_SATURATION => &mut info.saturation,
        VCP_SHARPNESS => &mut info.sharpness,
        VCP_RED_GAIN => &mut info.red_gain,
        VCP_GREEN_GAIN => &mut info.green_gain,
        VCP_BLUE_GAIN => &mut info.blue_gain,
        VCP_COLOR_TEMP => &mut info.color_temp,
        VCP_VOLUME => &mut info.volume,
        VCP_INPUT_SOURCE => &mut info.input_source,
        _ => return,
    };
    feature.current = value.min(feature.max.max(1));
}

#[derive(Clone, Debug)]
struct EdidDisplayIdentity {
    friendly_name: String,
    device_path: String,
}

/// Usa QueryDisplayConfig per mappare ogni GDI Device (`\\.\DISPLAY1`) al vero nome EDID e produttore
fn query_edid_identities() -> HashMap<String, EdidDisplayIdentity> {
    let mut map = HashMap::new();
    unsafe {
        let mut num_paths: u32 = 0;
        let mut num_modes: u32 = 0;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes) != 0 {
            return map;
        }

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> =
            vec![std::mem::zeroed(); num_paths as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> =
            vec![std::mem::zeroed(); num_modes as usize];

        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        ) != 0
        {
            return map;
        }

        paths.truncate(num_paths as usize);

        for path in paths {
            let mut source_name: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
            source_name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source_name.header.size =
                std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source_name.header.adapterId = path.sourceInfo.adapterId;
            source_name.header.id = path.sourceInfo.id;

            let mut target_name: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
            target_name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target_name.header.size =
                std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target_name.header.adapterId = path.targetInfo.adapterId;
            target_name.header.id = path.targetInfo.id;

            if DisplayConfigGetDeviceInfo(&mut source_name.header) == 0
                && DisplayConfigGetDeviceInfo(&mut target_name.header) == 0
            {
                let gdi = wide_to_string(&source_name.viewGdiDeviceName);
                let friendly = wide_to_string(&target_name.monitorFriendlyDeviceName);
                let dev_path = wide_to_string(&target_name.monitorDevicePath);
                if !gdi.is_empty() {
                    map.insert(
                        gdi,
                        EdidDisplayIdentity {
                            friendly_name: friendly,
                            device_path: dev_path,
                        },
                    );
                }
            }
        }
    }
    map
}

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

/// Decodifica il codice PNP a 3 lettere dal device path (`\\?\DISPLAY#AUS27A1#...`)
fn parse_brand_and_model(
    edid: Option<&EdidDisplayIdentity>,
    fallback_desc: &str,
    index: usize,
) -> (String, String, String) {
    let mut pnp_code = String::new();
    let mut raw_model = String::new();

    if let Some(id) = edid {
        raw_model = id.friendly_name.clone();
        // Esempio device_path: \\?\DISPLAY#AUS2792#5&12345...
        let parts: Vec<&str> = id.device_path.split('#').collect();
        if parts.len() >= 2 {
            let hw_id = parts[1].to_uppercase();
            if hw_id.len() >= 3 {
                pnp_code = hw_id[..3].to_string();
                if raw_model.is_empty() && hw_id.len() > 3 {
                    raw_model = hw_id;
                }
            }
        }
    }

    if raw_model.is_empty() {
        raw_model = fallback_desc.trim().to_string();
    }

    let brand = match pnp_code.as_str() {
        "AUS" | "ACI" | "ASU" => "ASUS".to_string(),
        "AOC" => "AOC".to_string(),
        "KTC" => "KTC".to_string(),
        "SAM" | "SDC" => "Samsung".to_string(),
        "GSM" | "LGD" => "LG".to_string(),
        "DEL" => "Dell".to_string(),
        "BNQ" => "BenQ".to_string(),
        "MSI" => "MSI".to_string(),
        "ACR" => "Acer".to_string(),
        "PHL" => "Philips".to_string(),
        "GBT" => "Gigabyte".to_string(),
        "LEN" => "Lenovo".to_string(),
        "HWP" | "HPQ" => "HP".to_string(),
        "VSC" => "ViewSonic".to_string(),
        _ => {
            let up = raw_model.to_uppercase();
            if up.contains("ASUS") || up.starts_with("VG") || up.starts_with("PG") || up.starts_with("XG") || up.starts_with("TUF") {
                "ASUS".to_string()
            } else if up.contains("AOC") || up.starts_with("AG") || up.starts_with("24G") || up.starts_with("27G") || up.starts_with("Q27G") {
                "AOC".to_string()
            } else if up.contains("KTC") || up.starts_with("H27") || up.starts_with("M27") || up.starts_with("G27") {
                "KTC".to_string()
            } else if !pnp_code.is_empty() {
                pnp_code.clone()
            } else {
                "Display".to_string()
            }
        }
    };

    let display_name = if raw_model.is_empty()
        || raw_model.eq_ignore_ascii_case("Generic PnP Monitor")
        || raw_model.eq_ignore_ascii_case("Monitor generico Plug and Play")
    {
        if brand != "Display" {
            format!("{} Monitor #{}", brand, index + 1)
        } else {
            format!("Monitor #{}", index + 1)
        }
    } else if raw_model.to_uppercase().starts_with(&brand.to_uppercase()) {
        raw_model.clone()
    } else if brand != "Display" {
        format!("{} {}", brand, raw_model)
    } else {
        raw_model.clone()
    };

    let model_code = if !pnp_code.is_empty() {
        pnp_code
    } else {
        brand.clone()
    };

    (brand, display_name, model_code)
}

fn read_vcp(handle: HANDLE, code: u8, default_max: u32) -> VcpFeatureState {
    unsafe {
        let mut current: u32 = 0;
        let mut max: u32 = 0;
        let ok = GetVCPFeatureAndVCPFeatureReply(
            handle,
            code,
            std::ptr::null_mut(),
            &mut current,
            &mut max,
        );
        if ok != 0 {
            let safe_max = if max == 0 { default_max } else { max };
            VcpFeatureState {
                code,
                supported: true,
                current: current.min(safe_max),
                max: safe_max,
            }
        } else {
            VcpFeatureState::unsupported(code, default_max)
        }
    }
}

struct EnumMonitorEntry {
    hmonitor: HMONITOR,
    gdi_device: String,
    resolution: String,
    is_primary: bool,
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let list = &mut *(lparam as *mut Vec<EnumMonitorEntry>);
    let mut mi: MONITORINFOEXW = std::mem::zeroed();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _) != 0 {
        let gdi_device = wide_to_string(&mi.szDevice);
        let width = (mi.monitorInfo.rcMonitor.right - mi.monitorInfo.rcMonitor.left).abs();
        let height = (mi.monitorInfo.rcMonitor.bottom - mi.monitorInfo.rcMonitor.top).abs();
        let resolution = format!("{} × {}", width, height);
        let is_primary = (mi.monitorInfo.dwFlags & 1) != 0;

        list.push(EnumMonitorEntry {
            hmonitor,
            gdi_device,
            resolution,
            is_primary,
        });
    }
    TRUE
}

fn enumerate_all_monitors() -> Vec<ManagedMonitor> {
    let edid_map = query_edid_identities();
    let mut entries: Vec<EnumMonitorEntry> = Vec::new();

    unsafe {
        let _ = EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_proc),
            &mut entries as *mut _ as LPARAM,
        );
    }

    // Prepariamo i task di lettura VCP in parallelo per ogni monitor fisico così la scansione è velocissima
    let mut handles_to_probe: Vec<(usize, EnumMonitorEntry, HANDLE, String)> = Vec::new();
    let mut next_id: usize = 1;

    for entry in entries {
        unsafe {
            let mut count: u32 = 0;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(entry.hmonitor, &mut count) != 0
                && count > 0
            {
                let mut phys: Vec<PHYSICAL_MONITOR> = vec![std::mem::zeroed(); count as usize];
                if GetPhysicalMonitorsFromHMONITOR(entry.hmonitor, count, phys.as_mut_ptr()) != 0 {
                    for pm in phys {
                        let desc_arr = pm.szPhysicalMonitorDescription;
                        let desc = wide_to_string(&desc_arr);
                        let handle_val = pm.hPhysicalMonitor;
                        handles_to_probe.push((
                            next_id,
                            EnumMonitorEntry {
                                hmonitor: entry.hmonitor,
                                gdi_device: entry.gdi_device.clone(),
                                resolution: entry.resolution.clone(),
                                is_primary: entry.is_primary,
                            },
                            handle_val,
                            desc,
                        ));
                        next_id += 1;
                    }
                }
            }
        }
    }

    // Interroghiamo ogni monitor in parallelo su thread dedicati per dimezzare i tempi di avvio
    let mut join_handles = Vec::new();
    for (idx, (id, entry, raw_handle, desc)) in handles_to_probe.into_iter().enumerate() {
        let edid = edid_map.get(&entry.gdi_device).cloned();
        let handle_int = raw_handle as isize;

        join_handles.push(thread::spawn(move || {
            let h = handle_int as HANDLE;
            let (brand, name, model_code) = parse_brand_and_model(edid.as_ref(), &desc, idx);

            let brightness = read_vcp(h, VCP_BRIGHTNESS, 100);
            let contrast = read_vcp(h, VCP_CONTRAST, 100);
            let saturation = read_vcp(h, VCP_SATURATION, 100);
            let sharpness = read_vcp(h, VCP_SHARPNESS, 100);
            let red_gain = read_vcp(h, VCP_RED_GAIN, 100);
            let green_gain = read_vcp(h, VCP_GREEN_GAIN, 100);
            let blue_gain = read_vcp(h, VCP_BLUE_GAIN, 100);
            let color_temp = read_vcp(h, VCP_COLOR_TEMP, 20);
            let volume = read_vcp(h, VCP_VOLUME, 100);
            let input_source = read_vcp(h, VCP_INPUT_SOURCE, 30);

            let ddc_supported = brightness.supported
                || contrast.supported
                || volume.supported
                || input_source.supported
                || red_gain.supported;

            let info = MonitorInfo {
                id,
                name,
                brand,
                model_code,
                gdi_device: entry.gdi_device,
                resolution: entry.resolution,
                is_primary: entry.is_primary,
                ddc_supported,
                brightness,
                contrast,
                saturation,
                sharpness,
                red_gain,
                green_gain,
                blue_gain,
                color_temp,
                volume,
                input_source,
            };

            ManagedMonitor {
                info,
                handle: PhysicalHandleWrap(h),
            }
        }));
    }

    let mut result = Vec::new();
    for jh in join_handles {
        if let Ok(managed) = jh.join() {
            result.push(managed);
        }
    }

    result.sort_by_key(|m| (!m.info.is_primary, m.info.id));
    result
}
