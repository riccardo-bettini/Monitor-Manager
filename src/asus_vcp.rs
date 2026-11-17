//! ASUS DisplayWidget Center (GameVisual, GamePlus, Image & Color) protocol & VCP profiles.
//! Maps standard MCCS codes alongside ASUS TUF/ROG proprietary VCP extensions (`0xDC`, `0xE0..=0xFF`)
//! discovered via monitor capability strings and live DDC/CI probing.

use serde::{Deserialize, Serialize};

/// Standardized + ASUS proprietary VCP opcodes used by ASUS TUF/ROG displays (verified on VG27WQ3B).
pub mod vcp {
    pub const FACTORY_RESET: u8 = 0x04;
    pub const COLOR_RESET: u8 = 0x08;
    pub const BRIGHTNESS: u8 = 0x10;
    pub const CONTRAST: u8 = 0x12;
    pub const COLOR_PRESET: u8 = 0x14;
    pub const RED_GAIN: u8 = 0x16;
    pub const GREEN_GAIN: u8 = 0x18;
    pub const BLUE_GAIN: u8 = 0x1A;
    pub const INPUT_SOURCE: u8 = 0x60;
    pub const VOLUME: u8 = 0x62;
    pub const GAMMA: u8 = 0x72;
    pub const ASPECT_CONTROL: u8 = 0x86; // MCCS Display Scaling (01=1:1, 02=Fill/Max, 03=Aspect, ...)
    pub const VIVID_PIXEL: u8 = 0x87; // MCCS Sharpness / ASUS VividPixel (step 10: 0..100)
    pub const SATURATION: u8 = 0x8A;
    pub const AUDIO_MUTE: u8 = 0x8D;
    pub const POWER_MODE: u8 = 0xD6;
    pub const GAME_VISUAL: u8 = 0xDC; // ASUS GameVisual Display Mode

    // ASUS Vendor Specific range (0xE0..=0xFF)
    pub const VARIABLE_OD: u8 = 0xE0; // Variable Overdrive / TraceFree (0..5 on VG27WQ3B)
    pub const FRAME_RATE_BOOST: u8 = 0xE1; // Dual-mode Frame Rate Boost or auxiliary toggle (0/1)
    pub const SHADOW_BOOST_ALT: u8 = 0xE2; // Alternative Shadow Boost / TraceFree register
    pub const GAMEPLUS_CROSSHAIR: u8 = 0xE3; // 00=Off, 07..0C=Crosshair styles
    pub const GAMEPLUS_TIMER: u8 = 0xE4; // 00=Off, 01..05=30/40/50/60/90 min
    pub const SHADOW_BOOST: u8 = 0xE5; // 00=Off, 01..04=Level 1..4 / Shadow Boost
    pub const BLUE_LIGHT_FILTER: u8 = 0xE6; // Blue Light Filter level (0..4 on VG27WQ3B)
    pub const DYNAMIC_SHADOW_BOOST: u8 = 0xE7; // Dynamic Shadow Boost toggle (0/1)
    pub const GAMEPLUS_FPS_COUNTER: u8 = 0xE8; // GamePlus FPS / Sub-mode action
    pub const GAMEPLUS_COORDS: u8 = 0xE9; // Packed GamePlus coordinate / timing telemetry
    pub const COLOR_SPACE: u8 = 0xEA; // Color gamut: 0=Wide Gamut, 1=sRGB, 2=DCI-P3
    pub const GAMEPLUS_ALIGNMENT: u8 = 0xEB; // Display Alignment / Stopwatch
    pub const ELMB: u8 = 0xEE; // Extreme Low Motion Blur (0..2 on VG27WQ3B)
    pub const VRR_ENABLE: u8 = 0xF0; // Adaptive-Sync / FreeSync / VRR toggle (0/1)
    pub const OLED_ANTI_FLICKER: u8 = 0xF2; // OLED Anti-Flicker (0/1/2 on OLED models)
    pub const GAMEPLUS_DPAD_NUDGE: u8 = 0xFA; // Write-only 4-way D-Pad: 1=Up, 2=Down, 3=Left, 4=Right
    pub const GAMEPLUS_EXTRA: u8 = 0xFB; // Extended GamePlus sub-styles
}

/// All 10 GameVisual cards shown in ASUS DisplayWidget Center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GameVisualMode {
    AiVisual = 0,
    RtsRpg = 1,
    Fps = 2,
    Cinema = 3,
    Scenery = 4,
    Racing = 5,
    Srgb = 6,
    Moba = 7,
    User = 8,
    Hdr = 9,
}

impl GameVisualMode {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => Self::AiVisual,
            1 => Self::RtsRpg,
            2 => Self::Fps,
            3 => Self::Cinema,
            4 => Self::Scenery,
            5 => Self::Racing,
            6 => Self::Srgb,
            7 => Self::Moba,
            8 => Self::User,
            9 => Self::Hdr,
            _ => Self::Racing,
        }
    }

    pub fn from_vcp_value(val: u32) -> Self {
        match val {
            1 => Self::RtsRpg,
            2 => Self::Fps,
            3 => Self::Cinema,
            4 => Self::Scenery,
            5 => Self::Racing,
            6 => Self::Srgb,
            7 => Self::Moba,
            8 => Self::User,
            9 => Self::Hdr,
            _ => Self::Racing,
        }
    }

    /// Returns the hardware `0xDC` VCP payload for hardware modes (`None` for software `AiVisual`).
    pub fn hardware_vcp_value(self) -> Option<u32> {
        match self {
            Self::AiVisual => None,
            other => Some(other as u32),
        }
    }

    pub fn label_it(self) -> &'static str {
        match self {
            Self::AiVisual => "AI Visual",
            Self::RtsRpg => "RTS/RPG",
            Self::Fps => "FPS",
            Self::Cinema => "CINEMA",
            Self::Scenery => "Scenario",
            Self::Racing => "Gara",
            Self::Srgb => "sRGB",
            Self::Moba => "MOBA",
            Self::User => "Utente",
            Self::Hdr => "HDR",
        }
    }

    pub fn title_it(self) -> &'static str {
        match self {
            Self::AiVisual => "Modalità AI Visual",
            Self::RtsRpg => "Modalità RTS/RPG",
            Self::Fps => "Modalità FPS",
            Self::Cinema => "Modalità Cinema",
            Self::Scenery => "Modalità Scenario",
            Self::Racing => "Modalità gara",
            Self::Srgb => "Modalità sRGB",
            Self::Moba => "Modalità MOBA",
            Self::User => "Modalità Utente",
            Self::Hdr => "Modalità HDR",
        }
    }

    pub fn title_en(self) -> &'static str {
        match self {
            Self::AiVisual => "AI Visual Mode",
            Self::RtsRpg => "RTS/RPG Mode",
            Self::Fps => "FPS Mode",
            Self::Cinema => "Cinema Mode",
            Self::Scenery => "Scenery Mode",
            Self::Racing => "Racing Mode",
            Self::Srgb => "sRGB Mode",
            Self::Moba => "MOBA Mode",
            Self::User => "User Mode",
            Self::Hdr => "HDR Mode",
        }
    }
}

/// Resolved capability detection & current live register state for a connected ASUS / gaming monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsusMonitorState {
    pub is_asus: bool,
    pub is_oled_panel: bool,
    pub dual_mode_supported: bool,
    pub gamevisual_supported: bool,
    pub gamevisual_mode: i32, // 0..=9 matching GameVisualMode

    // Immagine (Left column in DisplayWidget Center)
    pub variable_od: u32,
    pub variable_od_max: u32,
    pub variable_od_vcp: u8,
    pub variable_od_supported: bool,

    pub blue_light_filter: u32,
    pub blue_light_max: u32,
    pub blue_light_vcp: u8,
    pub blue_light_supported: bool,

    pub vivid_pixel: u32,
    pub vivid_pixel_max: u32,
    pub vivid_pixel_supported: bool,

    pub elmb: u32,
    pub elmb_max: u32,
    pub elmb_supported: bool,

    pub oled_anti_flicker: u32,
    pub oled_anti_flicker_max: u32,

    pub shadow_boost: u32,
    pub shadow_boost_max: u32,
    pub shadow_boost_vcp: u8,
    pub shadow_boost_supported: bool,

    pub dynamic_shadow_boost: bool,
    pub dynamic_shadow_boost_vcp: u8,
    pub dynamic_shadow_boost_supported: bool,

    pub frame_rate_boost: bool,
    pub vrr_enabled: bool,
    pub vrr_vcp: u8,
    pub vrr_supported: bool,

    pub aspect_control: u32, // 1=1:1, 2=Fill (1:1-Riempi), 3=Aspect, 5=Full
    pub aspect_control_supported: bool,

    // Colore (Right column in DisplayWidget Center)
    pub color_space: u32, // 0=Wide Gamut, 1=sRGB, 2=DCI-P3
    pub color_space_supported: bool,

    // GamePlus hardware status
    pub crosshair_style: u32, // 0=Off, 1..=6 mapped to VCP or overlay
    pub crosshair_vcp_values: Vec<u32>, // e.g. [0x00, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C]
    pub timer_preset: u32,    // 0=Off, 1=30:00, 2=40:00, 3=50:00, 4=60:00, 5=90:00
    pub fps_counter_mode: u32,
    pub alignment_enabled: bool,

    pub caps_raw: String,
    pub mpu_ver: String,
}

impl Default for AsusMonitorState {
    fn default() -> Self {
        Self {
            is_asus: false,
            is_oled_panel: false,
            dual_mode_supported: false,
            gamevisual_supported: false,
            gamevisual_mode: GameVisualMode::Racing as i32,
            variable_od: 3,
            variable_od_max: 5,
            variable_od_vcp: vcp::VARIABLE_OD,
            variable_od_supported: false,
            blue_light_filter: 0,
            blue_light_max: 4,
            blue_light_vcp: vcp::BLUE_LIGHT_FILTER,
            blue_light_supported: false,
            vivid_pixel: 50,
            vivid_pixel_max: 100,
            vivid_pixel_supported: false,
            elmb: 0,
            elmb_max: 2,
            elmb_supported: false,
            oled_anti_flicker: 0,
            oled_anti_flicker_max: 2,
            shadow_boost: 0,
            shadow_boost_max: 4,
            shadow_boost_vcp: vcp::SHADOW_BOOST,
            shadow_boost_supported: false,
            dynamic_shadow_boost: false,
            dynamic_shadow_boost_vcp: vcp::DYNAMIC_SHADOW_BOOST,
            dynamic_shadow_boost_supported: false,
            frame_rate_boost: false,
            vrr_enabled: true,
            vrr_vcp: vcp::VRR_ENABLE,
            vrr_supported: false,
            aspect_control: 2,
            aspect_control_supported: false,
            color_space: 0,
            color_space_supported: false,
            crosshair_style: 0,
            crosshair_vcp_values: vec![0x00, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C],
            timer_preset: 0,
            fps_counter_mode: 0,
            alignment_enabled: false,
            caps_raw: String::new(),
            mpu_ver: String::new(),
        }
    }
}

impl AsusMonitorState {
    /// Converts UI crosshair index (`0..=6`) to the monitor's VCP value (e.g. `0x00`, `0x07..=0x0C`).
    pub fn crosshair_ui_to_vcp(&self, ui_idx: u32) -> u32 {
        self.crosshair_vcp_values
            .get(ui_idx as usize)
            .copied()
            .unwrap_or(ui_idx)
    }

    /// Converts hardware `0xE3` VCP value to UI crosshair index (`0..=6`).
    pub fn crosshair_vcp_to_ui(&self, vcp_val: u32) -> u32 {
        self.crosshair_vcp_values
            .iter()
            .position(|&v| v == vcp_val)
            .map(|p| p as u32)
            .unwrap_or(0)
    }
}

/// Complete portable profile snapshot exported/imported via JSON (`Importa` / `Esporta` + `Confronta`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplayProfileSnapshot {
    pub profile_name: String,
    pub model: String,
    pub gamevisual_mode: i32,
    pub brightness: u32,
    pub contrast: u32,
    pub variable_od: u32,
    pub blue_light_filter: u32,
    pub vivid_pixel: u32,
    pub elmb: u32,
    pub shadow_boost: u32,
    pub dynamic_shadow_boost: bool,
    pub vrr_enabled: bool,
    pub aspect_control: u32,
    pub color_space: u32,
    pub saturation: u32,
    pub color_temp: u32,
    pub red_gain: u32,
    pub green_gain: u32,
    pub blue_gain: u32,
    pub volume: u32,
}

/// Detects panel type, capability metadata and resolves exact VCP bindings from raw MCCS caps + probed VCPs.
pub fn resolve_asus_state(
    brand: &str,
    model_name: &str,
    caps_raw: &str,
    vcp_readings: &[(u8, u32, u32, Vec<u16>)], // (code, cur, max, declared_enums)
) -> AsusMonitorState {
    let mut state = AsusMonitorState::default();
    let b_up = brand.to_uppercase();
    let m_up = model_name.to_uppercase();
    let caps_low = caps_raw.to_lowercase();

    state.is_asus = b_up.contains("ASUS")
        || b_up.contains("AUS")
        || m_up.contains("VG")
        || m_up.contains("PG")
        || m_up.contains("XG")
        || caps_low.contains("model(vg")
        || caps_low.contains("model(pg")
        || caps_low.contains("model(xg");

    state.is_oled_panel = m_up.contains("OLED")
        || caps_low.contains("type(oled)")
        || caps_low.contains("pg27aqdm")
        || caps_low.contains("pg32ucdm")
        || caps_low.contains("pg32ucdp")
        || caps_low.contains("xg27aqdmg");

    state.dual_mode_supported = m_up.contains("UCDP") || m_up.contains("UCG") || m_up.contains("DUAL");
    state.caps_raw = caps_raw.to_string();

    if let Some(idx) = caps_low.find("mpu_ver(") {
        let rest = &caps_raw[idx + 8..];
        if let Some(end) = rest.find(')') {
            state.mpu_ver = rest[..end].trim().to_string();
        }
    }

    let find = |code: u8| -> Option<&(u8, u32, u32, Vec<u16>)> {
        vcp_readings.iter().find(|r| r.0 == code)
    };

    if let Some((_, cur, _, _)) = find(vcp::GAME_VISUAL) {
        state.gamevisual_supported = true;
        state.gamevisual_mode = GameVisualMode::from_vcp_value(*cur) as i32;
    }

    if let Some((_, cur, max, _)) = find(vcp::VARIABLE_OD) {
        state.variable_od_supported = true;
        state.variable_od_vcp = vcp::VARIABLE_OD;
        state.variable_od = *cur;
        state.variable_od_max = (*max).max(1);
    }

    if let Some((_, cur, max, _)) = find(vcp::BLUE_LIGHT_FILTER) {
        state.blue_light_supported = true;
        state.blue_light_vcp = vcp::BLUE_LIGHT_FILTER;
        state.blue_light_filter = *cur;
        state.blue_light_max = (*max).max(1);
    }

    if let Some((_, cur, max, enums)) = find(vcp::VIVID_PIXEL) {
        state.vivid_pixel_supported = true;
        state.vivid_pixel = *cur;
        state.vivid_pixel_max = enums
            .iter()
            .copied()
            .max()
            .map(|v| v as u32)
            .unwrap_or(*max)
            .max(100);
    }

    if let Some((_, cur, max, _)) = find(vcp::ELMB) {
        state.elmb_supported = true;
        state.elmb = *cur;
        state.elmb_max = (*max).max(1);
    }

    if let Some((_, cur, max, _)) = find(vcp::OLED_ANTI_FLICKER) {
        if state.is_oled_panel {
            state.oled_anti_flicker = *cur;
            state.oled_anti_flicker_max = (*max).max(1);
        }
    }

    // Prefer CAPS-declared 0xE5 Shadow Boost (0..4), fallback to 0xE2 (0..3)
    if let Some((_, cur, max, enums)) = find(vcp::SHADOW_BOOST) {
        state.shadow_boost_supported = true;
        state.shadow_boost_vcp = vcp::SHADOW_BOOST;
        state.shadow_boost = *cur;
        state.shadow_boost_max = enums
            .iter()
            .copied()
            .max()
            .map(|v| v as u32)
            .unwrap_or(*max)
            .max(1);
    } else if let Some((_, cur, max, _)) = find(vcp::SHADOW_BOOST_ALT) {
        state.shadow_boost_supported = true;
        state.shadow_boost_vcp = vcp::SHADOW_BOOST_ALT;
        state.shadow_boost = *cur;
        state.shadow_boost_max = (*max).max(1);
    }

    if let Some((_, cur, _, _)) = find(vcp::DYNAMIC_SHADOW_BOOST) {
        state.dynamic_shadow_boost_supported = true;
        state.dynamic_shadow_boost_vcp = vcp::DYNAMIC_SHADOW_BOOST;
        state.dynamic_shadow_boost = *cur > 0;
    }

    if let Some((_, cur, _, _)) = find(vcp::FRAME_RATE_BOOST) {
        state.frame_rate_boost = *cur > 0 && state.dual_mode_supported;
    }

    if let Some((_, cur, _, _)) = find(vcp::VRR_ENABLE) {
        state.vrr_supported = true;
        state.vrr_vcp = vcp::VRR_ENABLE;
        state.vrr_enabled = *cur > 0;
    }

    if let Some((_, cur, _, _)) = find(vcp::ASPECT_CONTROL) {
        state.aspect_control_supported = true;
        state.aspect_control = *cur;
    }

    if let Some((_, cur, _, _)) = find(vcp::COLOR_SPACE) {
        state.color_space_supported = true;
        state.color_space = *cur;
    }

    if let Some((_, cur, _, enums)) = find(vcp::GAMEPLUS_CROSSHAIR) {
        if !enums.is_empty() {
            state.crosshair_vcp_values = enums.iter().map(|&v| v as u32).collect();
        }
        state.crosshair_style = state.crosshair_vcp_to_ui(*cur);
    }

    if let Some((_, cur, _, _)) = find(vcp::GAMEPLUS_TIMER) {
        state.timer_preset = *cur;
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_vg27wq3b_verified_register_values() {
        let caps = "(prot(monitor)type(lcd)model(VG27WQ3B)cmds(01 02 03 07 0C E3 F3)vcp(10 12 14(05 06 08 0B) 16 18 1A 60(0F 11 12) 62 87(00 0A 14 1E 28 32 3C 46 50 5A 64) 8A DC(01 02 03 04 05 06 07 08 09) E0 E1(00 01) E3(00 07 08 09 0A 0B 0C) E4(00 01 02 03 04 05) E5(00 01 02 03 04) E6 E7(00 01) EA(00 01 02) EE F0(00 01))mccs_ver(2.2)mpu_ver(01))";
        let readings = vec![
            (vcp::GAME_VISUAL, 5, 9, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]),
            (vcp::VARIABLE_OD, 3, 5, vec![]),
            (vcp::BLUE_LIGHT_FILTER, 3, 4, vec![]),
            (
                vcp::VIVID_PIXEL,
                50,
                100,
                vec![0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ),
            (vcp::ELMB, 1, 2, vec![]),
            (vcp::SHADOW_BOOST, 0, 4, vec![0, 1, 2, 3, 4]),
            (vcp::DYNAMIC_SHADOW_BOOST, 0, 1, vec![0, 1]),
            (vcp::VRR_ENABLE, 1, 1, vec![0, 1]),
            (vcp::ASPECT_CONTROL, 2, 15, vec![]),
            (vcp::COLOR_SPACE, 0, 2, vec![0, 1, 2]),
            (
                vcp::GAMEPLUS_CROSSHAIR,
                0,
                6,
                vec![0x00, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C],
            ),
        ];

        let st = resolve_asus_state("ASUS", "VG27WQ3B", caps, &readings);
        assert!(st.is_asus);
        assert!(!st.is_oled_panel);
        assert!(!st.dual_mode_supported);
        assert_eq!(st.gamevisual_mode, GameVisualMode::Racing as i32);
        assert_eq!(st.variable_od, 3);
        assert_eq!(st.variable_od_max, 5);
        assert_eq!(st.blue_light_filter, 3);
        assert_eq!(st.blue_light_max, 4);
        assert_eq!(st.vivid_pixel, 50);
        assert_eq!(st.elmb, 1);
        assert!(st.vrr_enabled);
        assert!(!st.dynamic_shadow_boost);
        assert_eq!(st.crosshair_ui_to_vcp(1), 0x07);
        assert_eq!(st.crosshair_vcp_to_ui(0x0B), 5);
        assert_eq!(st.mpu_ver, "01");
    }
}

/// Configurazione persistente DisplayWidget Center salvata in %APPDATA%\MonitorManager\dwc_config.json
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DwcConfig {
    pub power_saving_mode: bool,
    pub ai_visual_enabled: bool,
    pub ai_night_schedule: bool,
    pub ai_night_start_hour: u32,
    pub ai_night_end_hour: u32,
    pub gameplus_delivery_mode: String,
    pub gameplus_offset_x: i32,
    pub gameplus_offset_y: i32,
    pub hotkeys_enabled: bool,
    pub hotkeys: Vec<crate::automation::HotkeyBinding>,
    pub app_tweaker_enabled: bool,
    pub app_rules: Vec<crate::automation::AppTweakerRule>,
    pub update_policy: String,
}

impl Default for DwcConfig {
    fn default() -> Self {
        Self {
            power_saving_mode: false,
            ai_visual_enabled: false,
            ai_night_schedule: false,
            ai_night_start_hour: 21,
            ai_night_end_hour: 7,
            gameplus_delivery_mode: "auto".to_string(),
            gameplus_offset_x: 0,
            gameplus_offset_y: 0,
            hotkeys_enabled: true,
            hotkeys: crate::automation::default_hotkeys(),
            app_tweaker_enabled: false,
            app_rules: Vec::new(),
            update_policy: "manual".to_string(),
        }
    }
}

impl DwcConfig {
    pub fn config_path() -> std::path::PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        base.join("MonitorManager").join("dwc_config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<Self>(&raw) {
                if cfg.hotkeys.is_empty() {
                    cfg.hotkeys = crate::automation::default_hotkeys();
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
}
