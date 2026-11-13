use crate::monitor_profiles::{
    collect_probe_codes, describe_vcp, is_write_only_reset, parse_capabilities_resilient,
    DiscoveredVcp,
};
use crate::win_display::{
    apply_software_lut, query_hdr_map, query_refresh_rates, reset_software_lut, set_hdr_enabled,
    set_refresh_rate, DisplayOsInfo, SoftwareLutSettings,
};
use ddc::Ddc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use windows_sys::Win32::Devices::Display::{
    CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, DisplayConfigGetDeviceInfo,
    GetCapabilitiesStringLength, GetDisplayConfigBufferSizes,
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
    #[serde(default)]
    pub is_internal_wmi: bool,
    #[serde(default)]
    pub wmi_instance: String,
    #[serde(default)]
    pub os_info: DisplayOsInfo,
    #[serde(default)]
    pub discovered_vcps: Vec<DiscoveredVcp>,
    #[serde(default)]
    pub capabilities_raw: String,
    #[serde(default)]
    pub mccs_version: String,
    #[serde(default)]
    pub software_lut: SoftwareLutSettings,
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
        if !self.handle.0.is_null() {
            unsafe {
                let pm = PHYSICAL_MONITOR {
                    hPhysicalMonitor: self.handle.0,
                    szPhysicalMonitorDescription: [0; 128],
                };
                let _ = DestroyPhysicalMonitors(1, &pm);
            }
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

        thread::spawn(move || {
            let mut wmi_session: Option<crate::wmi_brightness::WmiSession> = None;

            loop {
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
                    // Prendiamo l'handle senza bloccare la UI durante la chiamata I2C o WMI
                    let target = {
                        let guard = monitors_ref.lock().unwrap();
                        guard.iter().find(|m| m.info.id == monitor_id).map(|m| {
                            (
                                m.handle.0 as isize,
                                m.info.is_internal_wmi,
                                m.info.wmi_instance.clone(),
                            )
                        })
                    };

                    if let Some((h_int, is_wmi, wmi_instance)) = target {
                        if is_wmi {
                            if vcp_code == VCP_BRIGHTNESS {
                                if wmi_session.is_none() {
                                    wmi_session = crate::wmi_brightness::WmiSession::connect();
                                }
                                if let Some(ref session) = wmi_session {
                                    let _ = session.set_brightness(&wmi_instance, value);
                                }
                            }
                        } else if h_int != 0 {
                            unsafe {
                                let _ = SetVCPFeature(h_int as HANDLE, vcp_code, value);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Scansiona tutti i monitor fisici collegati e legge i loro valori VCP attuali
    #[allow(dead_code)]
    pub fn scan_monitors(&self) -> Vec<MonitorInfo> {
        self.scan_monitors_ordered(&[])
    }

    /// Scansiona tutti i monitor fisici collegati ordinandoli in base alla lista salvata
    pub fn scan_monitors_ordered(&self, preferred_order: &[String]) -> Vec<MonitorInfo> {
        let mut discovered = enumerate_all_monitors();
        if !preferred_order.is_empty() {
            discovered.sort_by_key(|m| {
                let key = format!("{} {}", m.info.brand, m.info.name);
                preferred_order
                    .iter()
                    .position(|o| o == &key || o == &m.info.name)
                    .unwrap_or(usize::MAX)
            });
        }
        let mut guard = self.monitors.lock().unwrap();
        *guard = discovered;
        guard.iter().map(|m| m.info.clone()).collect()
    }

    /// Sposta un monitor nell'ordine personalizzato e restituisce la lista aggiornata
    pub fn reorder_monitors(&self, from_idx: usize, to_idx: usize) -> Vec<MonitorInfo> {
        let mut guard = self.monitors.lock().unwrap();
        if from_idx < guard.len() && to_idx < guard.len() && from_idx != to_idx {
            let item = guard.remove(from_idx);
            guard.insert(to_idx, item);
        }
        guard.iter().map(|m| m.info.clone()).collect()
    }

    /// Restituisce il numero di monitor fisici rilevati
    pub fn monitor_count(&self) -> usize {
        self.monitors.lock().unwrap().len()
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
                    if m.info.is_internal_wmi && vcp_code != VCP_BRIGHTNESS {
                        continue;
                    }
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

    /// Esegue in background la scoperta completa delle Capabilities MCCS e di tutti i VCP supportati.
    pub fn probe_capabilities_for_all(&self) -> Vec<MonitorInfo> {
        let targets: Vec<(usize, isize, String, String, bool)> = {
            let guard = self.monitors.lock().unwrap();
            guard
                .iter()
                .filter(|m| !m.info.is_internal_wmi && m.handle.0 as isize != 0)
                .map(|m| {
                    (
                        m.info.id,
                        m.handle.0 as isize,
                        m.info.brand.clone(),
                        m.info.model_code.clone(),
                        m.info.ddc_supported,
                    )
                })
                .collect()
        };

        // Anche ddc-hi può scoprire monitor extra o leggere capabilities quando Dxva2 standard fatica
        let hi_caps_fallback: Vec<(String, String)> = std::panic::catch_unwind(|| {
            ddc_hi::Display::enumerate()
                .into_iter()
                .filter_map(|mut d| {
                    let label = format!(
                        "{} {}",
                        d.info.manufacturer_id.clone().unwrap_or_default(),
                        d.info.model_name.clone().unwrap_or_else(|| d.info.id.clone())
                    );
                    d.handle
                        .capabilities_string()
                        .ok()
                        .map(|bytes| (label.to_uppercase(), String::from_utf8_lossy(&bytes).into_owned()))
                })
                .collect()
        })
        .unwrap_or_default();

        let mut joins = Vec::new();
        for (id, h_int, brand, model_code, _already_ddc) in targets {
            let fallback_pool = hi_caps_fallback.clone();
            joins.push(thread::spawn(move || {
                let h = h_int as HANDLE;
                let mut raw_caps = read_capabilities_string(h).unwrap_or_default();
                if raw_caps.is_empty() {
                    let b_up = brand.to_uppercase();
                    if let Some((_, s)) = fallback_pool
                        .iter()
                        .find(|(lbl, _)| !b_up.is_empty() && lbl.contains(&b_up))
                    {
                        raw_caps = s.clone();
                    }
                }

                let (caps_opt, db) = parse_capabilities_resilient(&raw_caps);
                let mccs_ver = caps_opt
                    .as_ref()
                    .and_then(|c| c.mccs_version.as_ref())
                    .map(|v| format!("{}.{}", v.major, v.minor))
                    .unwrap_or_default();

                let probe_codes = collect_probe_codes(caps_opt.as_ref(), &brand, &model_code);
                let mut discovered_vcps = Vec::new();

                for code in probe_codes {
                    let cap_values = caps_opt
                        .as_ref()
                        .and_then(|c| c.vcp_features.get(&code))
                        .map(|d| &d.values);
                    let declared_in_caps = caps_opt
                        .as_ref()
                        .is_some_and(|c| c.vcp_features.contains_key(&code));

                    if is_write_only_reset(code) {
                        if declared_in_caps {
                            let meta = describe_vcp(code, &brand, &db, cap_values);
                            discovered_vcps.push(DiscoveredVcp {
                                code,
                                name_it: meta.name_it,
                                name_en: meta.name_en,
                                category: meta.category,
                                is_continuous: false,
                                current: 1,
                                max: 1,
                                is_read_only: false,
                                is_critical: true,
                                from_capabilities: true,
                                options: vec![crate::monitor_profiles::VcpValueOption {
                                    value: 1,
                                    label: "Esegui Reset".to_string(),
                                }],
                            });
                        }
                        continue;
                    }

                    let vcp = read_vcp(h, code, 100);
                    if vcp.supported {
                        let meta = describe_vcp(code, &brand, &db, cap_values);
                        discovered_vcps.push(DiscoveredVcp {
                            code,
                            name_it: meta.name_it,
                            name_en: meta.name_en,
                            category: meta.category,
                            is_continuous: meta.is_continuous,
                            current: vcp.current,
                            max: vcp.max.max(1),
                            is_read_only: meta.is_read_only,
                            is_critical: meta.is_critical,
                            from_capabilities: declared_in_caps,
                            options: meta.options,
                        });
                    }
                }

                discovered_vcps.sort_by_key(|v| {
                    let cat_rank = match v.category.as_str() {
                        "image" => 0,
                        "color" => 1,
                        "gaming" => 2,
                        "audio_input" => 3,
                        _ => 4,
                    };
                    (cat_rank, v.code)
                });

                (id, raw_caps, mccs_ver, discovered_vcps)
            }));
        }

        let mut results = Vec::new();
        for j in joins {
            if let Ok(item) = j.join() {
                results.push(item);
            }
        }

        let mut guard = self.monitors.lock().unwrap();
        for (id, raw_caps, mccs_ver, vcps) in results {
            if let Some(m) = guard.iter_mut().find(|m| m.info.id == id) {
                if !vcps.is_empty() {
                    m.info.ddc_supported = true;
                }
                m.info.capabilities_raw = raw_caps;
                m.info.mccs_version = mccs_ver;
                m.info.discovered_vcps = vcps;
            }
        }
        guard.iter().map(|m| m.info.clone()).collect()
    }

    pub fn set_monitor_refresh_rate(&self, monitor_id: usize, hz: u32) -> Result<Vec<MonitorInfo>, String> {
        let gdi_device = {
            let guard = self.monitors.lock().unwrap();
            guard
                .iter()
                .find(|m| m.info.id == monitor_id)
                .map(|m| m.info.gdi_device.clone())
                .ok_or_else(|| "Monitor non trovato".to_string())?
        };
        set_refresh_rate(&gdi_device, hz)?;
        let (cur_hz, avail) = query_refresh_rates(&gdi_device);
        let mut guard = self.monitors.lock().unwrap();
        if let Some(m) = guard.iter_mut().find(|m| m.info.id == monitor_id) {
            m.info.os_info.current_refresh_rate = cur_hz;
            m.info.os_info.available_refresh_rates = avail;
            if let Some((base_res, _)) = m.info.resolution.split_once(" | ") {
                if cur_hz > 0 {
                    m.info.resolution = format!("{} | {}Hz", base_res, cur_hz);
                }
            }
        }
        Ok(guard.iter().map(|m| m.info.clone()).collect())
    }

    pub fn set_monitor_hdr(&self, monitor_id: usize, enabled: bool) -> Result<Vec<MonitorInfo>, String> {
        let gdi_device = {
            let guard = self.monitors.lock().unwrap();
            guard
                .iter()
                .find(|m| m.info.id == monitor_id)
                .map(|m| m.info.gdi_device.clone())
                .ok_or_else(|| "Monitor non trovato".to_string())?
        };
        set_hdr_enabled(&gdi_device, enabled)?;
        let hdr_map = query_hdr_map();
        let mut guard = self.monitors.lock().unwrap();
        if let Some(m) = guard.iter_mut().find(|m| m.info.id == monitor_id) {
            if let Some(&(sup, en)) = hdr_map.get(&gdi_device) {
                m.info.os_info.hdr_supported = sup;
                m.info.os_info.hdr_enabled = en;
            } else {
                m.info.os_info.hdr_enabled = enabled;
            }
        }
        Ok(guard.iter().map(|m| m.info.clone()).collect())
    }

    pub fn update_software_lut(&self, monitor_id: usize, cfg: SoftwareLutSettings) -> Vec<MonitorInfo> {
        let gdi_device = {
            let mut guard = self.monitors.lock().unwrap();
            let mut dev = String::new();
            if let Some(m) = guard.iter_mut().find(|m| m.info.id == monitor_id) {
                m.info.software_lut = cfg.clone();
                dev = m.info.gdi_device.clone();
            }
            dev
        };
        if !gdi_device.is_empty() {
            let _ = apply_software_lut(&gdi_device, &cfg);
        }
        self.get_cached_monitors()
    }

    pub fn reset_monitor_software_lut(&self, monitor_id: usize) -> Vec<MonitorInfo> {
        let gdi_device = {
            let mut guard = self.monitors.lock().unwrap();
            let mut dev = String::new();
            if let Some(m) = guard.iter_mut().find(|m| m.info.id == monitor_id) {
                m.info.software_lut = SoftwareLutSettings::default();
                dev = m.info.gdi_device.clone();
            }
            dev
        };
        if !gdi_device.is_empty() {
            let _ = reset_software_lut(&gdi_device);
        }
        self.get_cached_monitors()
    }
}

fn read_capabilities_string(handle: HANDLE) -> Option<String> {
    if handle.is_null() {
        return None;
    }
    unsafe {
        let mut len: u32 = 0;
        if GetCapabilitiesStringLength(handle, &mut len) == 0 || len == 0 || len > 16384 {
            return None;
        }
        let mut buf: Vec<u8> = vec![0u8; len as usize];
        if CapabilitiesRequestAndCapabilitiesReply(handle, buf.as_mut_ptr(), len) == 0 {
            return None;
        }
        let nul_pos = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = String::from_utf8_lossy(&buf[..nul_pos]).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

fn update_cached_vcp(info: &mut MonitorInfo, vcp_code: u8, value: u32) {
    for dv in &mut info.discovered_vcps {
        if dv.code == vcp_code {
            dv.current = if dv.is_continuous {
                value.min(dv.max.max(1))
            } else {
                value
            };
        }
    }
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
    refresh_hz: u32,
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
                let refresh_hz = if path.targetInfo.refreshRate.Denominator > 0 {
                    (path.targetInfo.refreshRate.Numerator as f64
                        / path.targetInfo.refreshRate.Denominator as f64)
                        .round() as u32
                } else {
                    0
                };
                if !gdi.is_empty() {
                    map.insert(
                        gdi,
                        EdidDisplayIdentity {
                            friendly_name: friendly,
                            device_path: dev_path,
                            refresh_hz,
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
            } else if up.contains("PHILIPS") || up.contains("EVNIA") || up.starts_with("PHL") {
                "Philips".to_string()
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
            let safe_max = if max == 0 {
                default_max.max(current).max(1)
            } else {
                max.max(current).max(1)
            };
            VcpFeatureState {
                code,
                supported: true,
                current,
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
        let resolution = format!("{}×{}", width, height);
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
    let wmi_panels = crate::wmi_brightness::query_wmi_panels();
    let hdr_map = query_hdr_map();
    let mut join_handles = Vec::new();
    for (idx, (id, entry, raw_handle, desc)) in handles_to_probe.into_iter().enumerate() {
        let edid = edid_map.get(&entry.gdi_device).cloned();
        let edid_path = edid
            .as_ref()
            .map(|e| e.device_path.clone())
            .unwrap_or_default();
        let hdr_entry = hdr_map.get(&entry.gdi_device).copied().unwrap_or((false, false));
        let handle_int = raw_handle as isize;

        join_handles.push(thread::spawn(move || {
            let h = handle_int as HANDLE;
            let (brand, name, model_code) = parse_brand_and_model(edid.as_ref(), &desc, idx);
            let (cur_hz, avail_hz) = query_refresh_rates(&entry.gdi_device);
            let effective_hz = if cur_hz > 0 {
                cur_hz
            } else {
                edid.as_ref().map(|e| e.refresh_hz).unwrap_or(0)
            };
            let full_resolution = match effective_hz {
                hz if hz > 0 => format!("{} | {}Hz", entry.resolution, hz),
                _ => entry.resolution,
            };

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
                resolution: full_resolution,
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
                is_internal_wmi: false,
                wmi_instance: String::new(),
                os_info: DisplayOsInfo {
                    current_refresh_rate: effective_hz,
                    available_refresh_rates: avail_hz,
                    hdr_supported: hdr_entry.0,
                    hdr_enabled: hdr_entry.1,
                },
                discovered_vcps: Vec::new(),
                capabilities_raw: String::new(),
                mccs_version: String::new(),
                software_lut: SoftwareLutSettings::default(),
            };

            (
                ManagedMonitor {
                    info,
                    handle: PhysicalHandleWrap(h),
                },
                edid_path,
            )
        }));
    }

    let mut probed: Vec<(ManagedMonitor, String)> = Vec::new();
    for jh in join_handles {
        if let Ok(item) = jh.join() {
            probed.push(item);
        }
    }

    // Abbiniamo gli schermi integrati dei laptop (eDP/LVDS) tramite WMI quando DDC/CI non è presente
    if !wmi_panels.is_empty() {
        let mut used_wmi = vec![false; wmi_panels.len()];

        // Primo passaggio: abbinamento deterministico EDID device path <-> WMI InstanceName
        for (mon, edid_path) in &mut probed {
            if !mon.info.brightness.supported {
                if let Some((p_idx, panel)) = wmi_panels.iter().enumerate().find(|(idx, p)| {
                    !used_wmi[*idx]
                        && crate::wmi_brightness::matches_device_path(&p.instance_name, edid_path)
                }) {
                    used_wmi[p_idx] = true;
                    mon.info.brightness = VcpFeatureState {
                        code: VCP_BRIGHTNESS,
                        supported: true,
                        current: panel.current.min(panel.max.max(100)),
                        max: panel.max.max(100),
                    };
                    mon.info.is_internal_wmi = true;
                    mon.info.wmi_instance = panel.instance_name.clone();
                }
            }
        }

        // Fallback sicuro: se rimane esattamente 1 pannello WMI e 1 schermo privo di DDC/CI
        let unmatched_wmi: Vec<usize> = used_wmi
            .iter()
            .enumerate()
            .filter_map(|(i, &u)| if !u { Some(i) } else { None })
            .collect();
        let unmatched_non_ddc: Vec<usize> = probed
            .iter()
            .enumerate()
            .filter_map(|(i, (m, _))| {
                if !m.info.ddc_supported && !m.info.is_internal_wmi {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if unmatched_wmi.len() == 1 && unmatched_non_ddc.len() == 1 {
            let panel = &wmi_panels[unmatched_wmi[0]];
            let mon = &mut probed[unmatched_non_ddc[0]].0;
            mon.info.brightness = VcpFeatureState {
                code: VCP_BRIGHTNESS,
                supported: true,
                current: panel.current.min(panel.max.max(100)),
                max: panel.max.max(100),
            };
            mon.info.is_internal_wmi = true;
            mon.info.wmi_instance = panel.instance_name.clone();
        }
    }

    let mut result: Vec<ManagedMonitor> = probed.into_iter().map(|(m, _)| m).collect();
    result.sort_by_key(|m| (!m.info.is_primary, m.info.id));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_monitor(id: usize, name: &str, is_internal_wmi: bool) -> ManagedMonitor {
        ManagedMonitor {
            info: MonitorInfo {
                id,
                name: name.to_string(),
                brand: if is_internal_wmi { "AUO" } else { "AOC" }.to_string(),
                model_code: name.to_string(),
                gdi_device: format!("\\\\.\\DISPLAY{}", id),
                resolution: "1920x1080".to_string(),
                is_primary: id == 1,
                ddc_supported: !is_internal_wmi,
                brightness: VcpFeatureState {
                    code: VCP_BRIGHTNESS,
                    supported: true,
                    current: 70,
                    max: 100,
                },
                contrast: VcpFeatureState::unsupported(VCP_CONTRAST, 100),
                saturation: VcpFeatureState::unsupported(VCP_SATURATION, 100),
                sharpness: VcpFeatureState::unsupported(VCP_SHARPNESS, 100),
                red_gain: VcpFeatureState::unsupported(VCP_RED_GAIN, 100),
                green_gain: VcpFeatureState::unsupported(VCP_GREEN_GAIN, 100),
                blue_gain: VcpFeatureState::unsupported(VCP_BLUE_GAIN, 100),
                color_temp: VcpFeatureState::unsupported(VCP_COLOR_TEMP, 20),
                volume: VcpFeatureState::unsupported(VCP_VOLUME, 100),
                input_source: VcpFeatureState::unsupported(VCP_INPUT_SOURCE, 30),
                is_internal_wmi,
                wmi_instance: if is_internal_wmi {
                    "DISPLAY\\AUOD2A2\\test_0".to_string()
                } else {
                    String::new()
                },
                os_info: DisplayOsInfo::default(),
                discovered_vcps: Vec::new(),
                capabilities_raw: String::new(),
                mccs_version: String::new(),
                software_lut: SoftwareLutSettings::default(),
            },
            handle: PhysicalHandleWrap(std::ptr::null_mut()),
        }
    }

    #[test]
    fn reorder_monitors_swaps_and_guards_bounds() {
        let engine = DdcEngine::new();
        {
            let mut guard = engine.monitors.lock().unwrap();
            guard.push(sample_monitor(1, "AOC 24G1WG4", false));
            guard.push(sample_monitor(2, "AUO B156HAN15.H", true));
        }

        let after = engine.reorder_monitors(0, 1);
        assert_eq!(after[0].id, 2);
        assert_eq!(after[1].id, 1);

        let unchanged = engine.reorder_monitors(0, 99);
        assert_eq!(unchanged[0].id, 2);
        assert_eq!(unchanged[1].id, 1);

        {
            let mut guard = engine.monitors.lock().unwrap();
            guard.push(sample_monitor(3, "LG UltraGear", false));
        }
        // Sposta #0 (id 2) in coda (#2): deve traslare [1, 3, 2] e non fare semplice swap
        let shifted = engine.reorder_monitors(0, 2);
        assert_eq!(shifted.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1, 3, 2]);
    }

    #[test]
    fn sync_all_only_updates_brightness_on_wmi_panels() {
        let engine = DdcEngine::new();
        {
            let mut guard = engine.monitors.lock().unwrap();
            guard.push(sample_monitor(1, "AOC 24G1WG4", false));
            let mut laptop = sample_monitor(2, "AUO B156HAN15.H", true);
            laptop.info.contrast.current = 50;
            guard.push(laptop);
        }

        engine.set_vcp_value(1, VCP_BRIGHTNESS, 42, true);
        engine.set_vcp_value(1, VCP_CONTRAST, 80, true);

        let cached = engine.get_cached_monitors();
        assert_eq!(cached[0].brightness.current, 42);
        assert_eq!(cached[1].brightness.current, 42);
        // Il pannello WMI non supporta contrasto DDC/CI, quindi sync_all non deve alterarlo
        assert_eq!(cached[1].contrast.current, 50);
    }
}
