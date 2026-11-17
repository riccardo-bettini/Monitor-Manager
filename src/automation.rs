//! Automation engines for Monitor Manager (ASUS DisplayWidget Center parity):
//! - Global Keyboard Shortcuts via Win32 `RegisterHotKey`
//! - App Tweaker (per-executable automatic GameVisual & picture switcher)
//! - AI Visual (lightweight GDI 16x9 desktop scene analyzer + App context classifier)
//! - Assistente IA natural-language instant command parser

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_NOREPEAT: u32 = 0x4000;
const WM_HOTKEY: u32 = 0x0312;
const PM_REMOVE: u32 = 0x0001;
const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const SRCCOPY: u32 = 0x00CC_0020;
const HALFTONE: i32 = 4;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct SystemTime {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmi_header: BitmapInfoHeader,
    bmi_colors: [u32; 1],
}

#[link(name = "user32")]
extern "system" {
    fn RegisterHotKey(hwnd: isize, id: i32, fs_modifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
    fn PeekMessageW(lp_msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn GetSystemMetrics(n_index: i32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn CreateCompatibleBitmap(hdc: isize, cx: i32, cy: i32) -> isize;
    fn SelectObject(hdc: isize, h: isize) -> isize;
    fn DeleteObject(ho: isize) -> i32;
    fn DeleteDC(hdc: isize) -> i32;
    fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
    fn StretchBlt(
        hdc_dest: isize,
        x_dest: i32,
        y_dest: i32,
        w_dest: i32,
        h_dest: i32,
        hdc_src: isize,
        x_src: i32,
        y_src: i32,
        w_src: i32,
        h_src: i32,
        rop: u32,
    ) -> i32;
    fn GetDIBits(
        hdc: isize,
        hbm: isize,
        start: u32,
        c_lines: u32,
        lpv_bits: *mut c_void,
        lpbmi: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(lp_system_time: *mut SystemTime);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub id: i32,
    pub action: String, // e.g. "brightness_up", "brightness_down", "cycle_gamevisual", "toggle_hdr", "toggle_crosshair", "cycle_input", "night_care"
    pub modifiers: u32, // bitmask: MOD_ALT(1) | MOD_CONTROL(2) | MOD_SHIFT(4)
    pub vk: u32,        // Win32 Virtual-Key code
    pub enabled: bool,
}

impl HotkeyBinding {
    pub fn shortcut_label(&self) -> String {
        let mut parts = Vec::new();
        if (self.modifiers & MOD_CONTROL) != 0 {
            parts.push("Ctrl".to_string());
        }
        if (self.modifiers & MOD_ALT) != 0 {
            parts.push("Alt".to_string());
        }
        if (self.modifiers & MOD_SHIFT) != 0 {
            parts.push("Shift".to_string());
        }
        parts.push(vk_to_label(self.vk));
        parts.join(" + ")
    }
}

pub fn default_hotkeys() -> Vec<HotkeyBinding> {
    vec![
        HotkeyBinding {
            id: 1,
            action: "brightness_up".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: 0x26, // VK_UP
            enabled: true,
        },
        HotkeyBinding {
            id: 2,
            action: "brightness_down".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: 0x28, // VK_DOWN
            enabled: true,
        },
        HotkeyBinding {
            id: 3,
            action: "cycle_gamevisual".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'G' as u32,
            enabled: true,
        },
        HotkeyBinding {
            id: 4,
            action: "toggle_crosshair".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'X' as u32,
            enabled: true,
        },
        HotkeyBinding {
            id: 5,
            action: "toggle_hdr".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'H' as u32,
            enabled: true,
        },
        HotkeyBinding {
            id: 6,
            action: "cycle_input".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'I' as u32,
            enabled: true,
        },
        HotkeyBinding {
            id: 7,
            action: "night_care".into(),
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'N' as u32,
            enabled: true,
        },
    ]
}

pub fn vk_to_label(vk: u32) -> String {
    match vk {
        0x26 => "Up".into(),
        0x28 => "Down".into(),
        0x25 => "Left".into(),
        0x27 => "Right".into(),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x1B => "Esc".into(),
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        _ => format!("0x{vk:02X}"),
    }
}

pub fn parse_shortcut_text(s: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut vk = 0u32;
    for tok in s.split('+').map(|p| p.trim().to_uppercase()) {
        match tok.as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "ALT" => mods |= MOD_ALT,
            "SHIFT" => mods |= MOD_SHIFT,
            "UP" => vk = 0x26,
            "DOWN" => vk = 0x28,
            "LEFT" => vk = 0x25,
            "RIGHT" => vk = 0x27,
            "SPACE" => vk = 0x20,
            f if f.starts_with('F') => {
                if let Ok(n) = f[1..].parse::<u32>() {
                    if (1..=12).contains(&n) {
                        vk = 0x70 + n - 1;
                    }
                }
            }
            c if c.len() == 1 => {
                let ch = c.as_bytes()[0];
                if ch.is_ascii_alphanumeric() {
                    vk = ch as u32;
                }
            }
            _ => {}
        }
    }
    if mods != 0 && vk != 0 {
        Some((mods, vk))
    } else {
        None
    }
}

/// Manages real Windows global hotkeys on a dedicated message thread.
#[derive(Clone)]
pub struct GlobalHotkeyService {
    desired: Arc<Mutex<(bool, Vec<HotkeyBinding>, u64)>>,
}

impl GlobalHotkeyService {
    pub fn spawn<F>(on_trigger: F) -> Self
    where
        F: Fn(String) + Send + 'static,
    {
        let desired = Arc::new(Mutex::new((true, default_hotkeys(), 1u64)));
        let worker_desired = Arc::clone(&desired);
        std::thread::Builder::new()
            .name("global-hotkeys".into())
            .spawn(move || unsafe {
                let mut registered_ids: Vec<i32> = Vec::new();
                let mut active_bindings: Vec<HotkeyBinding> = Vec::new();
                let mut applied_gen = 0u64;

                loop {
                    let next_cfg = {
                        let g = worker_desired.lock().unwrap();
                        if g.2 != applied_gen {
                            Some((g.0, g.1.clone(), g.2))
                        } else {
                            None
                        }
                    };

                    if let Some((enabled, list, gen)) = next_cfg {
                        for id in registered_ids.drain(..) {
                            UnregisterHotKey(0, id);
                        }
                        active_bindings.clear();
                        if enabled {
                            for b in list {
                                if b.enabled && b.vk != 0 {
                                    if RegisterHotKey(0, b.id, b.modifiers | MOD_NOREPEAT, b.vk) != 0
                                    {
                                        registered_ids.push(b.id);
                                        active_bindings.push(b);
                                    }
                                }
                            }
                        }
                        applied_gen = gen;
                    }

                    let mut msg: Msg = std::mem::zeroed();
                    while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_HOTKEY {
                            let id = msg.w_param as i32;
                            if let Some(b) = active_bindings.iter().find(|item| item.id == id) {
                                on_trigger(b.action.clone());
                            }
                        }
                    }

                    std::thread::sleep(Duration::from_millis(50));
                }
            })
            .ok();

        Self { desired }
    }

    pub fn update_bindings(&self, enabled: bool, bindings: Vec<HotkeyBinding>) {
        if let Ok(mut g) = self.desired.lock() {
            g.0 = enabled;
            g.1 = bindings;
            g.2 = g.2.wrapping_add(1);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppTweakerRule {
    pub exe_name: String,
    pub display_name: String,
    pub gamevisual_mode: i32, // 0..=9, or -1 to leave unchanged
    pub brightness: i32,      // 0..=100, or -1
    pub blue_light: i32,      // 0..=4, or -1
    pub variable_od: i32,     // 0..=5, or -1
    pub shadow_boost: i32,    // 0..=4, or -1
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct AiSceneMetrics {
    pub avg_luma: u8,         // 0..255
    pub dark_pixel_pct: u8,   // 0..100
    pub avg_saturation: u8,   // 0..100
    pub detected_scene: String,
    pub recommended_mode: i32, // GameVisualMode 1..=8
}

/// Captures a 16x9 thumbnail of the desktop using Win32 GDI StretchBlt and computes average luminance,
/// dark pixel percentage, and saturation without heavy CPU or external dependencies.
pub fn sample_screen_metrics() -> (u8, u8, u8) {
    unsafe {
        let sw = GetSystemMetrics(0).max(1);
        let sh = GetSystemMetrics(1).max(1);
        let screen_dc = GetDC(0);
        if screen_dc == 0 {
            return (128, 25, 45);
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, 16, 9);
        if mem_dc == 0 || bmp == 0 {
            if bmp != 0 {
                DeleteObject(bmp);
            }
            if mem_dc != 0 {
                DeleteDC(mem_dc);
            }
            ReleaseDC(0, screen_dc);
            return (128, 25, 45);
        }

        let old = SelectObject(mem_dc, bmp);
        SetStretchBltMode(mem_dc, HALFTONE);
        StretchBlt(mem_dc, 0, 0, 16, 9, screen_dc, 0, 0, sw, sh, SRCCOPY);

        let mut bmi: BitmapInfo = std::mem::zeroed();
        bmi.bmi_header.bi_size = std::mem::size_of::<BitmapInfoHeader>() as u32;
        bmi.bmi_header.bi_width = 16;
        bmi.bmi_header.bi_height = -9;
        bmi.bmi_header.bi_planes = 1;
        bmi.bmi_header.bi_bit_count = 32;
        bmi.bmi_header.bi_compression = BI_RGB;

        let mut pixels = [0u8; 16 * 9 * 4];
        let got = GetDIBits(
            mem_dc,
            bmp,
            0,
            9,
            pixels.as_mut_ptr().cast::<c_void>(),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old);
        DeleteObject(bmp);
        DeleteDC(mem_dc);
        ReleaseDC(0, screen_dc);

        if got <= 0 {
            return (128, 25, 45);
        }

        let mut luma_sum: u64 = 0;
        let mut dark_count: u64 = 0;
        let mut sat_sum: u64 = 0;
        let count = 16 * 9;
        for chunk in pixels.chunks_exact(4) {
            let b = chunk[0] as u64;
            let g = chunk[1] as u64;
            let r = chunk[2] as u64;
            let luma = (r * 299 + g * 587 + b * 114) / 1000;
            luma_sum += luma;
            if luma < 48 {
                dark_count += 1;
            }
            let max_c = r.max(g).max(b);
            let min_c = r.min(g).min(b);
            let sat = if max_c > 0 {
                ((max_c - min_c) * 100) / max_c
            } else {
                0
            };
            sat_sum += sat;
        }
        (
            (luma_sum / count) as u8,
            ((dark_count * 100) / count) as u8,
            (sat_sum / count) as u8,
        )
    }
}

/// Pure scene classifier combining foreground app context + screen luminance & saturation metrics.
pub fn classify_ai_scene(
    exe_lower: &str,
    title_lower: &str,
    avg_luma: u8,
    dark_pct: u8,
    avg_sat: u8,
) -> AiSceneMetrics {
    let combined = format!("{exe_lower} {title_lower}");
    let (scene, mode) = if combined.contains("vlc")
        || combined.contains("mpv")
        || combined.contains("mpc-")
        || combined.contains("netflix")
        || combined.contains("prime video")
        || combined.contains("disney")
        || combined.contains("stremio")
        || combined.contains("youtube")
        || combined.contains("plex")
    {
        ("Cinema / Video", 3) // CINEMA
    } else if combined.contains("valorant")
        || combined.contains("cs2")
        || combined.contains("csgo")
        || combined.contains("apex")
        || combined.contains("cod")
        || combined.contains("overwatch")
        || combined.contains("fortnite")
        || combined.contains("battlefield")
        || combined.contains("escape from tarkov")
        || combined.contains("pubg")
        || (dark_pct >= 62 && avg_luma < 85 && !combined.contains("explorer"))
    {
        ("FPS / Dark Tactical", 2) // FPS
    } else if combined.contains("league of legends")
        || combined.contains("dota")
        || combined.contains("smite")
    {
        ("MOBA Arena", 7) // MOBA
    } else if combined.contains("starcr")
        || combined.contains("age of")
        || combined.contains("baldurs")
        || combined.contains("diablo")
        || combined.contains("civilization")
        || combined.contains("total war")
    {
        ("RTS / RPG", 1) // RTS/RPG
    } else if combined.contains("forza")
        || combined.contains("f1_")
        || combined.contains("assetto")
        || combined.contains("granturismo")
        || combined.contains("trackmania")
        || combined.contains("rocketleague")
    {
        ("Racing Motion", 5) // Racing
    } else if combined.contains("photoshop")
        || combined.contains("lightroom")
        || combined.contains("figma")
        || combined.contains("blender")
        || combined.contains("davinci")
        || combined.contains("premiere")
        || combined.contains("code.exe")
        || combined.contains("devenv")
    {
        ("Color Accuracy / sRGB", 6) // sRGB
    } else if avg_sat > 55 && avg_luma > 135 {
        ("Vivid Scenery", 4) // Scenery
    } else {
        ("Adaptive Desktop / Racing", 5) // Racing (default crisp balanced mode on ASUS TUF)
    };

    AiSceneMetrics {
        avg_luma,
        dark_pixel_pct: dark_pct,
        avg_saturation: avg_sat,
        detected_scene: scene.to_string(),
        recommended_mode: mode,
    }
}

pub fn current_local_hour() -> u8 {
    unsafe {
        let mut st: SystemTime = std::mem::zeroed();
        GetLocalTime(&mut st);
        st.w_hour as u8
    }
}

pub fn is_night_care_hour(hour: u8, start_hour: u8, end_hour: u8) -> bool {
    if start_hour == end_hour {
        return false;
    }
    if start_hour < end_hour {
        hour >= start_hour && hour < end_hour
    } else {
        hour >= start_hour || hour < end_hour
    }
}

/// Natural-language intent emitted by Assistente IA prompt bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAction {
    SetBrightness(u32),
    SetContrast(u32),
    SetBlueLight(u32),
    SetGameVisual(i32),
    SetCrosshair(u32),
    SetHdr(bool),
    SetTaskbarAutohide(bool),
    SetDarkTheme(bool),
    SetPowerSaving(bool),
    SetInputSource(u32),
    SetVolume(u32),
    ResetMonitor,
}

/// Parses short bilingual IT/EN natural-language commands into instant monitor actions.
pub fn parse_ai_command(input: &str) -> Vec<AiAction> {
    let q = input.trim().to_lowercase();
    let mut actions = Vec::new();
    if q.is_empty() {
        return actions;
    }

    let first_num = || -> Option<u32> {
        q.split(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse::<u32>().ok())
    };

    if q.contains("luminos") || q.contains("bright") {
        if let Some(n) = first_num() {
            actions.push(AiAction::SetBrightness(n.min(100)));
        }
    }
    if q.contains("contrasto") || q.contains("contrast") {
        if let Some(n) = first_num() {
            actions.push(AiAction::SetContrast(n.min(100)));
        }
    }
    if q.contains("luce blu") || q.contains("blue light") || q.contains("notte") || q.contains("night") {
        if let Some(n) = first_num() {
            actions.push(AiAction::SetBlueLight(n.min(4)));
        } else if q.contains("notte") || q.contains("night") || q.contains("riposa") {
            actions.push(AiAction::SetBlueLight(3));
            actions.push(AiAction::SetBrightness(45));
        }
    }
    if q.contains("volume") || q.contains("audio") {
        if let Some(n) = first_num() {
            actions.push(AiAction::SetVolume(n.min(100)));
        }
    }
    if q.contains("hdr") {
        if q.contains("disattiva") || q.contains("off") || q.contains("spegni") {
            actions.push(AiAction::SetHdr(false));
        } else {
            actions.push(AiAction::SetHdr(true));
        }
    }
    if q.contains("barra") || q.contains("taskbar") {
        let on = !(q.contains("off") || q.contains("mostra") || q.contains("disattiva"));
        actions.push(AiAction::SetTaskbarAutohide(on));
    }
    if q.contains("tema") || q.contains("theme") {
        let dark = !q.contains("chiaro") && !q.contains("light") && !q.contains("leggero");
        actions.push(AiAction::SetDarkTheme(dark));
    }
    if q.contains("risparmio") || q.contains("eco") || q.contains("power saving") {
        let on = !(q.contains("off") || q.contains("standard") || q.contains("disattiva"));
        actions.push(AiAction::SetPowerSaving(on));
    }
    if q.contains("mirino") || q.contains("crosshair") {
        if q.contains("off") || q.contains("disattiva") || q.contains("togli") {
            actions.push(AiAction::SetCrosshair(0));
        } else if q.contains("verde") || q.contains("green") {
            actions.push(AiAction::SetCrosshair(4));
        } else {
            actions.push(AiAction::SetCrosshair(3));
        }
    }
    if q.contains("displayport") || q.contains("dp") {
        actions.push(AiAction::SetInputSource(0x0F));
    } else if q.contains("hdmi 2") || q.contains("hdmi2") {
        actions.push(AiAction::SetInputSource(0x12));
    } else if q.contains("hdmi") {
        actions.push(AiAction::SetInputSource(0x11));
    }

    if q.contains("rts") || q.contains("rpg") {
        actions.push(AiAction::SetGameVisual(1));
    } else if q.contains("fps") {
        actions.push(AiAction::SetGameVisual(2));
    } else if q.contains("cinema") || q.contains("film") {
        actions.push(AiAction::SetGameVisual(3));
    } else if q.contains("scenario") || q.contains("scenery") {
        actions.push(AiAction::SetGameVisual(4));
    } else if q.contains("gara") || q.contains("racing") {
        actions.push(AiAction::SetGameVisual(5));
    } else if q.contains("srgb") {
        actions.push(AiAction::SetGameVisual(6));
    } else if q.contains("moba") {
        actions.push(AiAction::SetGameVisual(7));
    } else if q.contains("utente") || q.contains("user") {
        actions.push(AiAction::SetGameVisual(8));
    }

    if q.contains("ripristina") || q.contains("reset") {
        actions.push(AiAction::ResetMonitor);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bilingual_ai_commands() {
        assert_eq!(
            parse_ai_command("imposta luminosità 75"),
            vec![AiAction::SetBrightness(75)]
        );
        assert_eq!(
            parse_ai_command("attiva hdr e modalità cinema"),
            vec![AiAction::SetHdr(true), AiAction::SetGameVisual(3)]
        );
        assert_eq!(
            parse_ai_command("mirino verde"),
            vec![AiAction::SetCrosshair(4)]
        );
        assert_eq!(
            parse_ai_command("switch input hdmi 2"),
            vec![AiAction::SetInputSource(0x12)]
        );
    }

    #[test]
    fn classifies_scenes_and_night_care_hours() {
        let vlc = classify_ai_scene("vlc.exe", "Movie.mkv", 60, 50, 40);
        assert_eq!(vlc.recommended_mode, 3);
        let val = classify_ai_scene("valorant.exe", "VALORANT", 45, 70, 35);
        assert_eq!(val.recommended_mode, 2);
        let figma = classify_ai_scene("figma.exe", "Design", 210, 5, 30);
        assert_eq!(figma.recommended_mode, 6);

        assert!(is_night_care_hour(23, 21, 7));
        assert!(is_night_care_hour(3, 21, 7));
        assert!(!is_night_care_hour(14, 21, 7));
    }

    #[test]
    fn parses_and_formats_hotkey_shortcuts() {
        let (mods, vk) = parse_shortcut_text("Ctrl + Alt + G").unwrap();
        let b = HotkeyBinding {
            id: 1,
            action: "cycle_gamevisual".into(),
            modifiers: mods,
            vk,
            enabled: true,
        };
        assert_eq!(b.shortcut_label(), "Ctrl + Alt + G");
    }
}
