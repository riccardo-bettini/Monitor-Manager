//! AOC G-Menu Protocol, VCP Capabilities & Multi-Profile Configuration
//! Supports standard VESA MCCS + AOC proprietary VCP registers (`0xDC`, `0xED`, `0xE0..=0xFF`)
//! with automatic Hardware-First + Software LUT / Win32 Overlay fallback for unsupported registers.

use crate::monitor_profiles::VcpValueOption;
use crate::win_display::SoftwareLutSettings;
use serde::{Deserialize, Serialize};

pub mod vcp {
    pub const FACTORY_RESET: u8 = 0x04;
    pub const BRIGHT_CONTRAST_RESET: u8 = 0x05;
    pub const COLOR_RESET: u8 = 0x08;
    pub const BRIGHTNESS: u8 = 0x10;
    pub const CONTRAST: u8 = 0x12;
    pub const COLOR_PRESET: u8 = 0x14;
    pub const RED_GAIN: u8 = 0x16;
    pub const GREEN_GAIN: u8 = 0x18;
    pub const BLUE_GAIN: u8 = 0x1A;
    pub const AUTO_SETUP_DSUB: u8 = 0x1E;
    pub const INPUT_SOURCE: u8 = 0x60;
    pub const VOLUME: u8 = 0x62;
    pub const GAMMA: u8 = 0x72;
    pub const IMAGE_RATIO: u8 = 0x86;
    pub const SHARPNESS: u8 = 0x87;
    pub const SATURATION: u8 = 0x8A;
    pub const AUDIO_MUTE: u8 = 0x8D;
    pub const ECO_MODE: u8 = 0xDC; // AOC Eco Mode (0x00=Standard, 0x0B..0x11)

    // AOC Vendor-Specific range (0xE0..=0xFF)
    pub const LOW_BLUE: u8 = 0xE1;
    pub const OVERDRIVE: u8 = 0xE2;
    pub const GAME_MODE: u8 = 0xE3;
    pub const DCB_MODE: u8 = 0xE4;
    pub const SHADOW_CONTROL: u8 = 0xE6;
    pub const DCR: u8 = 0xED; // Dynamic Contrast Ratio (0=Off, 1=On)
    pub const DIAL_POINT: u8 = 0xF8; // Selettore / Crosshair
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AocMonitorState {
    pub is_aoc: bool,
    pub serial_number: String,
    pub model_desc: String,

    // Modalità Gioco: 0=Off, 1=FPS, 2=RTS, 3=Corsa, 4=G1, 5=G2, 6=G3, 7=Personalizzato
    pub game_mode: i32,
    pub game_mode_hw: bool,

    pub shadow_control: i32, // 0..=100 (default 50)
    pub shadow_control_hw: bool,

    pub game_color: i32, // 0..=20 (default 10)
    pub game_color_hw: bool,

    // DCB Mod: 0=Off, 1=Miglior. Comp., 2=Interf. Naturale, 3=Verde Prato, 4=Blu Cielo, 5=Rilev. Aut., 6=Dimostraz.
    pub dcb_mode: i32,
    pub dcb_hw: bool,

    // Overdrive: 0=Off, 1=Debole, 2=Medio, 3=Forte, 4=Boost
    pub overdrive: i32,
    pub overdrive_hw: bool,

    // Gamma: 0=Gamma1(2.2), 1=Gamma2(2.0), 2=Gamma3(2.4)
    pub gamma_mode: i32,
    pub gamma_hw: bool,

    // Modo di Blu Basso: 0=Off, 1=Multimedia, 2=Internet, 3=Ufficio, 4=Lettura
    pub low_blue_mode: i32,
    pub low_blue_hw: bool,

    // Eco Mode: 0=Standard, 1=Testo, 2=Internet, 3=Gioco, 4=Film, 5=Sport, 6=Lettura, 7=Uniformità
    pub eco_mode: i32,
    pub eco_hw: bool,
    pub eco_vcp_values: Vec<u32>,

    // Display Tab
    pub dcr_enabled: bool,
    pub dcr_hw: bool,

    // Cont.Fotog. (FPS Counter): 0=Off, 1=TopLeft, 2=TopRight, 3=BottomLeft, 4=BottomRight
    pub frame_counter_pos: i32,
    pub frame_counter_hw: bool,

    // Selettore (Crosshair)
    pub dial_point_enabled: bool,
    pub dial_point_hw: bool,

    pub audio_mute: bool,
    pub audio_mute_hw: bool,

    pub image_ratio: u32,
    pub image_ratio_hw: bool,
    pub image_ratio_options: Vec<VcpValueOption>,

    pub input_options: Vec<VcpValueOption>,
    pub color_temp_options: Vec<VcpValueOption>,

    pub dsub_available: bool,
    pub usb_charge_supported: bool,
    pub light_fx_supported: bool,
}

impl Default for AocMonitorState {
    fn default() -> Self {
        Self {
            is_aoc: false,
            serial_number: String::new(),
            model_desc: String::new(),
            game_mode: 7, // Personalizzato
            game_mode_hw: false,
            shadow_control: 50,
            shadow_control_hw: false,
            game_color: 10,
            game_color_hw: false,
            dcb_mode: 0,
            dcb_hw: false,
            overdrive: 2, // Medio
            overdrive_hw: false,
            gamma_mode: 0, // Gamma1(2.2)
            gamma_hw: false,
            low_blue_mode: 0,
            low_blue_hw: false,
            eco_mode: 0,
            eco_hw: false,
            eco_vcp_values: vec![0x00, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10],
            dcr_enabled: false,
            dcr_hw: false,
            frame_counter_pos: 0,
            frame_counter_hw: false,
            dial_point_enabled: false,
            dial_point_hw: false,
            audio_mute: false,
            audio_mute_hw: false,
            image_ratio: 2,
            image_ratio_hw: false,
            image_ratio_options: vec![
                VcpValueOption { value: 2, label: "WIDESCREEN".into() },
                VcpValueOption { value: 1, label: "4:3".into() },
                VcpValueOption { value: 5, label: "1:1".into() },
            ],
            input_options: vec![
                VcpValueOption { value: 1, label: "VGA_ANALOG".into() },
                VcpValueOption { value: 15, label: "DP1".into() },
                VcpValueOption { value: 17, label: "HDMI1".into() },
                VcpValueOption { value: 18, label: "HDMI2".into() },
            ],
            color_temp_options: vec![
                VcpValueOption { value: 1, label: "SRGB".into() },
                VcpValueOption { value: 5, label: "CALDO(6500K)".into() },
                VcpValueOption { value: 6, label: "NORMALE(7300K)".into() },
                VcpValueOption { value: 8, label: "FREDDO(9300K)".into() },
                VcpValueOption { value: 11, label: "RGB UTENTE".into() },
            ],
            dsub_available: false,
            usb_charge_supported: false,
            light_fx_supported: false,
        }
    }
}

impl AocMonitorState {
    pub fn eco_ui_to_vcp(&self, ui_idx: i32) -> u32 {
        match ui_idx {
            0 => 0x00,
            1 => 0x0B,
            2 => 0x0C,
            3 => 0x0D,
            4 => 0x0E,
            5 => 0x0F,
            6 => 0x10,
            7 => 0x11,
            _ => 0x00,
        }
    }

    pub fn eco_vcp_to_ui(vcp_val: u32) -> i32 {
        match vcp_val {
            0x00 => 0,
            0x0B => 1,
            0x0C => 2,
            0x0D => 3,
            0x0E => 4,
            0x0F => 5,
            0x10 => 6,
            0x11 => 7,
            _ => 0,
        }
    }
}

pub fn eco_label(vcp_val: u32) -> &'static str {
    match vcp_val {
        0x00 => "Standard",
        0x0B => "Testo",
        0x0C => "Internet",
        0x0D => "Gioco",
        0x0E => "Film",
        0x0F => "Sport",
        0x10 => "Lettura",
        0x11 => "Uniformità",
        _ => "Standard",
    }
}

pub fn input_label(vcp_val: u32) -> String {
    match vcp_val {
        0x01 => "VGA_ANALOG".into(),
        0x03 => "DVI1".into(),
        0x04 => "DVI2".into(),
        0x0F => "DP1".into(),
        0x10 => "DP2".into(),
        0x11 => "HDMI1".into(),
        0x12 => "HDMI2".into(),
        0x1B => "USB-C".into(),
        other => format!("INPUT 0x{:02X}", other),
    }
}

pub fn color_temp_label(vcp_val: u32) -> String {
    match vcp_val {
        1 => "SRGB".into(),
        4 => "5000K".into(),
        5 => "CALDO(6500K)".into(),
        6 => "NORMALE(7300K)".into(),
        8 => "FREDDO(9300K)".into(),
        11 => "RGB UTENTE".into(),
        other => format!("PRESET {}", other),
    }
}

pub fn image_ratio_label(vcp_val: u32) -> String {
    match vcp_val {
        1 => "4:3".into(),
        2 => "WIDESCREEN".into(),
        3 => "ASPECT".into(),
        5 => "1:1".into(),
        0x0B => "16:9 (17\")".into(),
        0x0C => "19\" (4:3)".into(),
        0x0E => "19\"W (16:10)".into(),
        0x0F => "21.5\"W".into(),
        0x11 => "23\"W".into(),
        other => format!("MODO 0x{:02X}", other),
    }
}

pub fn resolve_aoc_state(
    brand: &str,
    model_name: &str,
    serial_number: &str,
    caps_raw: &str,
    vcp_readings: &[(u8, u32, u32, Vec<u16>)],
) -> AocMonitorState {
    let mut state = AocMonitorState::default();
    let b_up = brand.to_uppercase();
    let m_up = model_name.to_uppercase();
    let caps_low = caps_raw.to_lowercase();

    state.is_aoc = b_up.contains("AOC")
        || m_up.contains("AOC")
        || m_up.contains("24G")
        || m_up.contains("27G")
        || m_up.contains("Q27G")
        || m_up.contains("AG2")
        || m_up.contains("AG3")
        || caps_low.contains("model(c24g")
        || caps_low.contains("model(24g")
        || caps_low.contains("model(27g")
        || caps_low.contains("model(ag");

    state.serial_number = serial_number.trim().to_string();

    if let Some(idx) = caps_low.find("model(") {
        let rest = &caps_raw[idx + 6..];
        if let Some(end) = rest.find(')') {
            state.model_desc = rest[..end].trim().to_string();
        }
    }
    if state.model_desc.is_empty() {
        state.model_desc = model_name.trim().to_string();
    }

    let find = |code: u8| -> Option<&(u8, u32, u32, Vec<u16>)> {
        vcp_readings.iter().find(|r| r.0 == code)
    };

    // Eco Mode (0xDC)
    if let Some((_, cur, _, enums)) = find(vcp::ECO_MODE) {
        state.eco_hw = true;
        state.eco_mode = AocMonitorState::eco_vcp_to_ui(*cur);
        if !enums.is_empty() {
            state.eco_vcp_values = enums.iter().map(|&v| v as u32).collect();
        }
    }

    // DCR (0xED)
    if let Some((_, cur, max, _)) = find(vcp::DCR) {
        if *max >= 1 {
            state.dcr_hw = true;
            state.dcr_enabled = *cur > 0;
        }
    }

    // Gamma (0x72)
    if let Some((_, cur, max, _)) = find(vcp::GAMMA) {
        if *max >= 1 {
            state.gamma_hw = true;
            state.gamma_mode = (*cur as i32).clamp(0, 2);
        }
    }

    // LowBlue (0xE1)
    if let Some((_, cur, max, _)) = find(vcp::LOW_BLUE) {
        if *max >= 1 {
            state.low_blue_hw = true;
            state.low_blue_mode = (*cur as i32).clamp(0, 4);
        }
    }

    // Overdrive (0xE2)
    if let Some((_, cur, max, _)) = find(vcp::OVERDRIVE) {
        if *max >= 1 {
            state.overdrive_hw = true;
            state.overdrive = (*cur as i32).clamp(0, 4);
        }
    }

    // Game Mode (0xE3)
    if let Some((_, cur, max, _)) = find(vcp::GAME_MODE) {
        if *max >= 1 {
            state.game_mode_hw = true;
            state.game_mode = (*cur as i32).clamp(0, 7);
        }
    }

    // DCB Mode (0xE4)
    if let Some((_, cur, max, _)) = find(vcp::DCB_MODE) {
        if *max >= 1 {
            state.dcb_hw = true;
            state.dcb_mode = (*cur as i32).clamp(0, 6);
        }
    }

    // Shadow Control (0xE6) - solo se max > 0 (su C24G1 max=0 -> usa fallback SW LUT)
    if let Some((_, cur, max, _)) = find(vcp::SHADOW_CONTROL) {
        if *max > 0 {
            state.shadow_control_hw = true;
            state.shadow_control = (*cur as i32).clamp(0, 100);
        }
    }

    // Audio Mute (0x8D)
    if let Some((_, cur, _, _)) = find(vcp::AUDIO_MUTE) {
        state.audio_mute_hw = true;
        state.audio_mute = *cur == 1;
    }

    // Image Ratio (0x86)
    if let Some((_, cur, _, enums)) = find(vcp::IMAGE_RATIO) {
        state.image_ratio_hw = true;
        state.image_ratio = *cur;
        if !enums.is_empty() {
            state.image_ratio_options = enums
                .iter()
                .map(|&v| VcpValueOption {
                    value: v as u32,
                    label: image_ratio_label(v as u32),
                })
                .collect();
        }
    }

    // Input Source (0x60)
    if let Some((_, cur, _, enums)) = find(vcp::INPUT_SOURCE) {
        state.dsub_available = *cur == 1;
        if !enums.is_empty() {
            state.input_options = enums
                .iter()
                .map(|&v| VcpValueOption {
                    value: v as u32,
                    label: input_label(v as u32),
                })
                .collect();
        }
    }

    // Color Preset (0x14)
    if let Some((_, _, _, enums)) = find(vcp::COLOR_PRESET) {
        if !enums.is_empty() {
            state.color_temp_options = enums
                .iter()
                .map(|&v| VcpValueOption {
                    value: v as u32,
                    label: color_temp_label(v as u32),
                })
                .collect();
        }
    }

    // Light FX & USB Charge detection (AGON series only)
    state.light_fx_supported = m_up.contains("AG") || m_up.contains("AGON");
    state.usb_charge_supported = caps_low.contains("usb");

    state
}

/// Combina tutti i fallback Software LUT attivi per AOC (LowBlue + Shadow Control + Game Color + Gamma + DCB)
pub fn build_lut_from_aoc_state(st: &AocMonitorState) -> SoftwareLutSettings {
    let warmth = if st.low_blue_hw {
        0
    } else {
        match st.low_blue_mode {
            1 => 22, // Multimedia
            2 => 36, // Internet
            3 => 52, // Ufficio
            4 => 72, // Lettura
            _ => 0,
        }
    };

    let gamma_pct = if st.gamma_hw {
        100
    } else {
        match st.gamma_mode {
            1 => 91,  // Gamma2 (2.0)
            2 => 109, // Gamma3 (2.4)
            _ => 100, // Gamma1 (2.2)
        }
    };

    let shadow = if st.shadow_control_hw {
        50
    } else {
        st.shadow_control.clamp(0, 100)
    };

    let game_color = if st.game_color_hw {
        10
    } else {
        st.game_color.clamp(0, 20)
    };

    let dcb = if st.dcb_hw { 0 } else { st.dcb_mode.clamp(0, 6) };

    let enabled = warmth > 0 || gamma_pct != 100 || shadow != 50 || game_color != 10 || dcb != 0;

    SoftwareLutSettings {
        enabled,
        gamma_pct,
        warmth,
        contrast_pct: 100,
        red_pct: 100,
        green_pct: 100,
        blue_pct: 100,
        shadow_control: shadow,
        game_color,
        dcb_mode: dcb,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AocAdvancedSlot {
    pub shadow_control: i32,
    pub game_color: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub color_temp: i32,
    pub red_gain: i32,
    pub green_gain: i32,
    pub blue_gain: i32,
    pub dcb_mode: i32,
    pub overdrive: i32,
    pub gamma_mode: i32,
}

impl Default for AocAdvancedSlot {
    fn default() -> Self {
        Self {
            shadow_control: 50,
            game_color: 10,
            brightness: 100,
            contrast: 50,
            color_temp: 11,
            red_gain: 50,
            green_gain: 50,
            blue_gain: 50,
            dcb_mode: 0,
            overdrive: 2,
            gamma_mode: 0,
        }
    }
}

impl AocAdvancedSlot {
    pub fn preset_for_mode(mode: i32) -> Self {
        match mode {
            1 => Self {
                // FPS
                shadow_control: 65,
                game_color: 12,
                brightness: 95,
                contrast: 55,
                overdrive: 3,
                ..Default::default()
            },
            2 => Self {
                // RTS
                shadow_control: 55,
                game_color: 13,
                brightness: 90,
                contrast: 55,
                overdrive: 2,
                ..Default::default()
            },
            3 => Self {
                // Corsa
                shadow_control: 50,
                game_color: 14,
                brightness: 100,
                contrast: 60,
                overdrive: 4,
                ..Default::default()
            },
            _ => Self::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AocProfile {
    pub id: String,
    pub name: String,
    pub game_mode: i32,
    pub eco_mode: i32,
    pub low_blue_mode: i32,
    pub dcr_enabled: bool,
    pub frame_counter_pos: i32,
    pub dial_point_enabled: bool,
    pub g1: AocAdvancedSlot,
    pub g2: AocAdvancedSlot,
    pub g3: AocAdvancedSlot,
    pub custom: AocAdvancedSlot,
}

impl Default for AocProfile {
    fn default() -> Self {
        Self {
            id: "profile1".into(),
            name: "PROFILE1".into(),
            game_mode: 7,
            eco_mode: 0,
            low_blue_mode: 0,
            dcr_enabled: false,
            frame_counter_pos: 0,
            dial_point_enabled: false,
            g1: AocAdvancedSlot::preset_for_mode(1),
            g2: AocAdvancedSlot::preset_for_mode(2),
            g3: AocAdvancedSlot::preset_for_mode(3),
            custom: AocAdvancedSlot::default(),
        }
    }
}

impl AocProfile {
    pub fn slot_mut(&mut self, slot_idx: i32) -> &mut AocAdvancedSlot {
        match slot_idx {
            4 => &mut self.g1,
            5 => &mut self.g2,
            6 => &mut self.g3,
            _ => &mut self.custom,
        }
    }

    pub fn slot(&self, slot_idx: i32) -> &AocAdvancedSlot {
        match slot_idx {
            4 => &self.g1,
            5 => &self.g2,
            6 => &self.g3,
            _ => &self.custom,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AocConfig {
    pub active_profile_id: String,
    pub profiles: Vec<AocProfile>,
    pub extended_brightness_enabled: bool,
    pub day_brightness: i32,
    pub evening_brightness: i32,
    pub night_brightness: i32,
    pub evening_hour: u32,
    pub night_hour: u32,
    pub sync_osd_message: bool,
}

impl Default for AocConfig {
    fn default() -> Self {
        Self {
            active_profile_id: "profile1".into(),
            profiles: vec![AocProfile::default()],
            extended_brightness_enabled: false,
            day_brightness: 90,
            evening_brightness: 65,
            night_brightness: 40,
            evening_hour: 19,
            night_hour: 22,
            sync_osd_message: false,
        }
    }
}

impl AocConfig {
    pub fn config_path() -> std::path::PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        base.join("MonitorManager").join("aoc_config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<Self>(&raw) {
                if cfg.profiles.is_empty() {
                    cfg.profiles.push(AocProfile::default());
                }
                if !cfg.profiles.iter().any(|p| p.id == cfg.active_profile_id) {
                    cfg.active_profile_id = cfg.profiles[0].id.clone();
                }
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn active_profile(&self) -> &AocProfile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile_id)
            .unwrap_or(&self.profiles[0])
    }

    pub fn active_profile_mut(&mut self) -> &mut AocProfile {
        let idx = self
            .profiles
            .iter()
            .position(|p| p.id == self.active_profile_id)
            .unwrap_or(0);
        &mut self.profiles[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_real_aoc_c24g1_capabilities() {
        let caps = "(prot(monitor)type(LCD)model(C24G1)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 0C 10 12 14(01 05 06 08 0B) 16 18 1A 52 60(01 0F 11 12) 86(01 02 05 0B 0C 0E 0F 11) AC AE B2 B6 C6 C8 CA CC(01 02 03 04 05) D6(01 04 05) DC(00 0B 0C 0D 0E 0F 10) DF ED FF)mswhql(1)asset_eep(40)mccs_ver(2.2))";
        let readings = vec![
            (vcp::BRIGHTNESS, 100, 100, vec![]),
            (vcp::CONTRAST, 50, 100, vec![]),
            (vcp::COLOR_PRESET, 11, 11, vec![1, 5, 6, 8, 11]),
            (vcp::INPUT_SOURCE, 15, 3, vec![1, 15, 17, 18]),
            (vcp::IMAGE_RATIO, 2, 17, vec![1, 2, 5, 0x0B]),
            (vcp::ECO_MODE, 0, 16, vec![0x00, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10]),
            (vcp::SHADOW_CONTROL, 0, 0, vec![]), // max=0 -> SW fallback
            (vcp::DCR, 0, 1, vec![]),
            (vcp::DIAL_POINT, 0, 1, vec![]),
        ];

        let st = resolve_aoc_state("AOC", "AOC 24G1WG4", "240324", caps, &readings);
        assert!(st.is_aoc);
        assert_eq!(st.serial_number, "240324");
        assert_eq!(st.model_desc, "C24G1");
        assert!(st.eco_hw);
        assert!(st.dcr_hw);
        assert!(!st.shadow_control_hw);
        assert!(!st.dsub_available);
        assert_eq!(st.input_options.len(), 4);
        assert_eq!(st.input_options[0].label, "VGA_ANALOG");
        assert_eq!(st.color_temp_options.len(), 5);
    }

    #[test]
    fn builds_coherent_sw_lut_from_aoc_state() {
        let mut st = AocMonitorState::default();
        st.low_blue_mode = 3; // Ufficio -> 52%
        st.gamma_mode = 2; // 2.4 -> 109%
        st.shadow_control = 75;
        let lut = build_lut_from_aoc_state(&st);
        assert!(lut.enabled);
        assert_eq!(lut.warmth, 52);
        assert_eq!(lut.gamma_pct, 109);
        assert_eq!(lut.shadow_control, 75);
    }
}
