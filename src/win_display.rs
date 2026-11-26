use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use windows_sys::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes,
    QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, CreateDCW, DeleteDC, EnumDisplaySettingsExW, CDS_UPDATEREGISTRY,
    DEVMODEW, DISP_CHANGE_SUCCESSFUL, ENUM_CURRENT_SETTINGS,
};
use windows_sys::Win32::UI::ColorSystem::SetDeviceGammaRamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayModeOption {
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOsInfo {
    pub current_refresh_rate: u32,
    pub available_refresh_rates: Vec<u32>,
    pub hdr_supported: bool,
    pub hdr_enabled: bool,
    #[serde(default)]
    pub current_width: u32,
    #[serde(default)]
    pub current_height: u32,
    #[serde(default)]
    pub orientation_deg: u32,
    #[serde(default)]
    pub available_modes: Vec<DisplayModeOption>,
}

impl Default for DisplayOsInfo {
    fn default() -> Self {
        Self {
            current_refresh_rate: 0,
            available_refresh_rates: Vec::new(),
            hdr_supported: false,
            hdr_enabled: false,
            current_width: 0,
            current_height: 0,
            orientation_deg: 0,
            available_modes: Vec::new(),
        }
    }
}

fn default_shadow_control() -> i32 {
    50
}
fn default_game_color() -> i32 {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftwareLutSettings {
    pub enabled: bool,
    pub gamma_pct: i32,
    pub warmth: i32,
    pub contrast_pct: i32,
    pub red_pct: i32,
    pub green_pct: i32,
    pub blue_pct: i32,
    #[serde(default = "default_shadow_control")]
    pub shadow_control: i32,
    #[serde(default = "default_game_color")]
    pub game_color: i32,
    #[serde(default)]
    pub dcb_mode: i32,
}

impl Default for SoftwareLutSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            gamma_pct: 100,
            warmth: 0,
            contrast_pct: 100,
            red_pct: 100,
            green_pct: 100,
            blue_pct: 100,
            shadow_control: 50,
            game_color: 10,
            dcb_mode: 0,
        }
    }
}

fn utf16_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

pub fn query_refresh_rates(gdi_device: &str) -> (u32, Vec<u32>) {
    if gdi_device.is_empty() {
        return (0, Vec::new());
    }
    let dev_w = utf16_z(gdi_device);
    let mut current_dm: DEVMODEW = unsafe { std::mem::zeroed() };
    current_dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

    let ok = unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut current_dm, 0) };
    if ok == 0 {
        return (0, Vec::new());
    }

    let cur_w = current_dm.dmPelsWidth;
    let cur_h = current_dm.dmPelsHeight;
    let cur_bpp = current_dm.dmBitsPerPel;
    let cur_hz = current_dm.dmDisplayFrequency;

    let mut rates = BTreeSet::new();
    if cur_hz > 0 {
        rates.insert(cur_hz);
    }

    let mut mode_idx: u32 = 0;
    loop {
        let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), mode_idx, &mut dm, 0) } == 0 {
            break;
        }
        if dm.dmPelsWidth == cur_w
            && dm.dmPelsHeight == cur_h
            && dm.dmBitsPerPel == cur_bpp
            && dm.dmDisplayFrequency > 0
        {
            rates.insert(dm.dmDisplayFrequency);
        }
        mode_idx += 1;
        if mode_idx > 2048 {
            break;
        }
    }

    (cur_hz, rates.into_iter().collect())
}

pub fn set_refresh_rate(gdi_device: &str, hz: u32) -> Result<(), String> {
    if gdi_device.is_empty() || hz == 0 {
        return Err("Dispositivo o frequenza non valida".to_string());
    }
    let dev_w = utf16_z(gdi_device);
    let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm, 0) } == 0 {
        return Err("Impossibile leggere modalità video corrente".to_string());
    }

    const DM_DISPLAYFREQUENCY_FLAG: u32 = 0x00400000;
    dm.dmDisplayFrequency = hz;
    dm.dmFields |= DM_DISPLAYFREQUENCY_FLAG;

    let res = unsafe {
        ChangeDisplaySettingsExW(
            dev_w.as_ptr(),
            &dm,
            std::ptr::null_mut(),
            CDS_UPDATEREGISTRY,
            std::ptr::null(),
        )
    };
    if res == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("Cambio refresh rate {}Hz fallito (codice {})", hz, res))
    }
}

pub fn query_display_modes(gdi_device: &str) -> (u32, u32, u32, Vec<DisplayModeOption>) {
    if gdi_device.is_empty() {
        return (0, 0, 0, Vec::new());
    }
    let dev_w = utf16_z(gdi_device);
    let mut current_dm: DEVMODEW = unsafe { std::mem::zeroed() };
    current_dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut current_dm, 0) } == 0 {
        return (0, 0, 0, Vec::new());
    }

    let cur_w = current_dm.dmPelsWidth;
    let cur_h = current_dm.dmPelsHeight;
    let cur_orient = unsafe { current_dm.Anonymous1.Anonymous2.dmDisplayOrientation };
    let orientation_deg = match cur_orient {
        1 => 90,
        2 => 180,
        3 => 270,
        _ => 0,
    };

    let mut seen: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    let mut mode_idx: u32 = 0;
    loop {
        let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), mode_idx, &mut dm, 0) } == 0 {
            break;
        }
        if dm.dmBitsPerPel >= 32
            && dm.dmPelsWidth >= 800
            && dm.dmPelsHeight >= 600
            && dm.dmDisplayFrequency > 0
        {
            seen.insert((dm.dmPelsWidth, dm.dmPelsHeight, dm.dmDisplayFrequency));
        }
        mode_idx += 1;
        if mode_idx > 4096 {
            break;
        }
    }

    let mut list: Vec<DisplayModeOption> = seen
        .into_iter()
        .map(|(w, h, hz)| DisplayModeOption {
            width: w,
            height: h,
            hz,
            label: format!("{}x{}-{}HZ", w, h, hz),
        })
        .collect();
    // Sort largest area first, then highest refresh rate
    list.sort_by(|a, b| {
        let area_a = (a.width as u64) * (a.height as u64);
        let area_b = (b.width as u64) * (b.height as u64);
        area_b
            .cmp(&area_a)
            .then_with(|| b.hz.cmp(&a.hz))
            .then_with(|| b.width.cmp(&a.width))
    });

    (cur_w, cur_h, orientation_deg, list)
}

pub fn set_display_mode(gdi_device: &str, width: u32, height: u32, hz: u32) -> Result<(), String> {
    if gdi_device.is_empty() || width == 0 || height == 0 || hz == 0 {
        return Err("Parametri modalità video non validi".to_string());
    }
    let dev_w = utf16_z(gdi_device);
    let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm, 0) } == 0 {
        return Err("Impossibile leggere modalità video corrente".to_string());
    }

    const DM_PELSWIDTH_FLAG: u32 = 0x00080000;
    const DM_PELSHEIGHT_FLAG: u32 = 0x00100000;
    const DM_DISPLAYFREQUENCY_FLAG: u32 = 0x00400000;
    dm.dmPelsWidth = width;
    dm.dmPelsHeight = height;
    dm.dmDisplayFrequency = hz;
    dm.dmFields |= DM_PELSWIDTH_FLAG | DM_PELSHEIGHT_FLAG | DM_DISPLAYFREQUENCY_FLAG;

    let res = unsafe {
        ChangeDisplaySettingsExW(
            dev_w.as_ptr(),
            &dm,
            std::ptr::null_mut(),
            CDS_UPDATEREGISTRY,
            std::ptr::null(),
        )
    };
    if res == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("Cambio risoluzione {}x{} {}Hz fallito ({})", width, height, hz, res))
    }
}

pub fn set_orientation(gdi_device: &str, degrees: u32) -> Result<(), String> {
    if gdi_device.is_empty() {
        return Err("Dispositivo GDI non valido".to_string());
    }
    let target_orient: u32 = match degrees {
        90 => 1,
        180 => 2,
        270 => 3,
        _ => 0,
    };

    let dev_w = utf16_z(gdi_device);
    let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsExW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm, 0) } == 0 {
        return Err("Impossibile leggere orientamento corrente".to_string());
    }

    let cur_orient = unsafe { dm.Anonymous1.Anonymous2.dmDisplayOrientation };
    if cur_orient == target_orient {
        return Ok(());
    }

    let cur_portrait = cur_orient == 1 || cur_orient == 3;
    let target_portrait = target_orient == 1 || target_orient == 3;
    if cur_portrait != target_portrait {
        std::mem::swap(&mut dm.dmPelsWidth, &mut dm.dmPelsHeight);
    }

    const DM_DISPLAYORIENTATION_FLAG: u32 = 0x00000080;
    const DM_PELSWIDTH_FLAG: u32 = 0x00080000;
    const DM_PELSHEIGHT_FLAG: u32 = 0x00100000;
    dm.Anonymous1.Anonymous2.dmDisplayOrientation = target_orient;
    dm.dmFields |= DM_DISPLAYORIENTATION_FLAG | DM_PELSWIDTH_FLAG | DM_PELSHEIGHT_FLAG;

    let res = unsafe {
        ChangeDisplaySettingsExW(
            dev_w.as_ptr(),
            &dm,
            std::ptr::null_mut(),
            CDS_UPDATEREGISTRY,
            std::ptr::null(),
        )
    };
    if res == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("Cambio orientamento {}° fallito ({})", degrees, res))
    }
}

pub fn query_hdr_map() -> HashMap<String, (bool, bool)> {
    let mut map = HashMap::new();
    unsafe {
        let mut num_paths: u32 = 0;
        let mut num_modes: u32 = 0;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes) != 0 {
            return map;
        }

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); num_paths as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); num_modes as usize];
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
            let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
            src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            src.header.adapterId = path.sourceInfo.adapterId;
            src.header.id = path.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                continue;
            }
            let gdi = wide_to_string(&src.viewGdiDeviceName);
            if gdi.is_empty() {
                continue;
            }

            let mut adv: DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO = std::mem::zeroed();
            adv.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
            adv.header.size = std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
            adv.header.adapterId = path.targetInfo.adapterId;
            adv.header.id = path.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut adv.header) == 0 {
                let val = adv.Anonymous.value;
                let supported = (val & 0x1) != 0;
                let enabled = (val & 0x2) != 0;
                map.insert(gdi, (supported, enabled));
            }
        }
    }
    map
}

pub fn set_hdr_enabled(gdi_device: &str, enabled: bool) -> Result<(), String> {
    if gdi_device.is_empty() {
        return Err("Dispositivo GDI non valido".to_string());
    }
    unsafe {
        let mut num_paths: u32 = 0;
        let mut num_modes: u32 = 0;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes) != 0 {
            return Err("GetDisplayConfigBufferSizes fallita".to_string());
        }

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); num_paths as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); num_modes as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        ) != 0
        {
            return Err("QueryDisplayConfig fallita".to_string());
        }
        paths.truncate(num_paths as usize);

        for path in paths {
            let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
            src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            src.header.adapterId = path.sourceInfo.adapterId;
            src.header.id = path.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                continue;
            }
            if wide_to_string(&src.viewGdiDeviceName) != gdi_device {
                continue;
            }

            let mut set_pkt: DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE = std::mem::zeroed();
            set_pkt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE;
            set_pkt.header.size =
                std::mem::size_of::<DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE>() as u32;
            set_pkt.header.adapterId = path.targetInfo.adapterId;
            set_pkt.header.id = path.targetInfo.id;
            set_pkt.Anonymous.value = if enabled { 1 } else { 0 };

            let rc = DisplayConfigSetDeviceInfo(&set_pkt.header);
            if rc == 0 {
                return Ok(());
            } else {
                return Err(format!("Impossibile cambiare HDR (errore {})", rc));
            }
        }
    }
    Err("Monitor non trovato nei path attivi".to_string())
}

pub fn build_gamma_ramp(cfg: &SoftwareLutSettings) -> [u16; 768] {
    let mut ramp = [0u16; 768];
    if !cfg.enabled {
        for i in 0..256 {
            let val = ((i as u32) * 257).min(65535) as u16;
            ramp[i] = val;
            ramp[256 + i] = val;
            ramp[512 + i] = val;
        }
        return ramp;
    }

    let gamma = (cfg.gamma_pct.clamp(50, 220) as f64) / 100.0;
    let inv_gamma = 1.0 / gamma;
    let contrast = (cfg.contrast_pct.clamp(50, 150) as f64) / 100.0;
    let warmth = (cfg.warmth.clamp(0, 100) as f64) / 100.0;
    let shadow_delta = ((cfg.shadow_control.clamp(0, 100) - 50) as f64) / 50.0;
    let game_color_delta = ((cfg.game_color.clamp(0, 20) - 10) as f64) / 10.0;

    // DCB subtle color balance multipliers
    let (dcb_r, dcb_g, dcb_b) = match cfg.dcb_mode {
        1 => (1.03, 1.03, 1.03), // Miglioramento completo
        2 => (1.04, 1.01, 0.97), // Interfaccia naturale (incarnato caldo)
        3 => (0.97, 1.05, 0.97), // Verde prato
        4 => (0.96, 1.00, 1.06), // Blu cielo
        5 => (1.02, 1.02, 1.02), // Rilevamento automatico
        6 => (1.06, 1.04, 1.06), // Dimostrazione
        _ => (1.0, 1.0, 1.0),
    };

    // Filtro luce blu morbido (riduce gradualmente B e leggermente G)
    let r_mult = ((cfg.red_pct.clamp(20, 150) as f64 / 100.0) * (1.0 - warmth * 0.02) * dcb_r).min(1.35);
    let g_mult = ((cfg.green_pct.clamp(20, 150) as f64 / 100.0) * (1.0 - warmth * 0.22) * dcb_g).min(1.35);
    let b_mult = ((cfg.blue_pct.clamp(20, 150) as f64 / 100.0) * (1.0 - warmth * 0.48) * dcb_b).min(1.35);

    for i in 0..256 {
        let norm = (i as f64) / 255.0;
        let mut adj = norm.powf(inv_gamma);

        // Shadow Control: non-linear lift/crush focused on shadows (norm < 0.6) without clipping highlights
        if shadow_delta.abs() > 0.001 {
            let shadow_weight = (1.0 - adj).powf(2.1);
            adj = (adj + shadow_delta * 0.22 * shadow_weight).clamp(0.0, 1.0);
        }

        // Game Color: S-curve punch around midpoint
        if game_color_delta.abs() > 0.001 {
            let s_curve = adj - 0.5;
            adj = (adj + game_color_delta * 0.14 * s_curve * (1.0 - 4.0 * s_curve * s_curve).max(0.0)).clamp(0.0, 1.0);
        }

        let contrast_adj = ((adj - 0.5) * contrast + 0.5).clamp(0.0, 1.0);

        let r = (contrast_adj * r_mult * 65535.0).round().clamp(0.0, 65535.0) as u16;
        let g = (contrast_adj * g_mult * 65535.0).round().clamp(0.0, 65535.0) as u16;
        let b = (contrast_adj * b_mult * 65535.0).round().clamp(0.0, 65535.0) as u16;

        ramp[i] = r;
        ramp[256 + i] = g;
        ramp[512 + i] = b;
    }
    ramp
}

pub fn apply_software_lut(gdi_device: &str, cfg: &SoftwareLutSettings) -> bool {
    if gdi_device.is_empty() {
        return false;
    }
    let ramp = build_gamma_ramp(cfg);
    let dev_w = utf16_z(gdi_device);
    unsafe {
        let hdc = CreateDCW(
            std::ptr::null(),
            dev_w.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        );
        if hdc.is_null() {
            return false;
        }
        let ok = SetDeviceGammaRamp(hdc, ramp.as_ptr() as *const _);
        let _ = DeleteDC(hdc);
        ok != 0
    }
}

pub fn reset_software_lut(gdi_device: &str) -> bool {
    apply_software_lut(gdi_device, &SoftwareLutSettings::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_ramp_when_disabled() {
        let ramp = build_gamma_ramp(&SoftwareLutSettings::default());
        assert_eq!(ramp[0], 0);
        assert_eq!(ramp[255], 65535);
        assert_eq!(ramp[256 + 128], 128 * 257);
        assert_eq!(ramp[512 + 255], 65535);
    }

    #[test]
    fn warmth_reduces_blue_more_than_red() {
        let cfg = SoftwareLutSettings {
            enabled: true,
            warmth: 80,
            ..Default::default()
        };
        let ramp = build_gamma_ramp(&cfg);
        assert!(ramp[255] > ramp[256 + 255]);
        assert!(ramp[256 + 255] > ramp[512 + 255]);
    }

    #[test]
    fn shadow_control_lifts_darks_without_clipping_white() {
        let base = build_gamma_ramp(&SoftwareLutSettings {
            enabled: true,
            ..Default::default()
        });
        let lifted = build_gamma_ramp(&SoftwareLutSettings {
            enabled: true,
            shadow_control: 90,
            ..Default::default()
        });
        assert!(lifted[32] > base[32]);
        assert_eq!(lifted[255], 65535);
    }
}
