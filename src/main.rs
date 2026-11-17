#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod asus_vcp;
mod automation;
mod ddc;
mod gameplus_overlay;
mod monitor_profiles;
mod settings;
mod updater;
mod win_display;
mod win_system;
mod wmi_brightness;

slint::include_modules!();

use asus_vcp::{vcp as avcp, DisplayProfileSnapshot, DwcConfig, GameVisualMode};
use automation::{AiAction, AppTweakerRule, GlobalHotkeyService};
use ddc::{DdcEngine, MonitorInfo};
use gameplus_overlay::{GamePlusOverlayEngine, OverlayConfig};
use raw_window_handle::HasWindowHandle;
use settings::AppSettings;
use win_display::SoftwareLutSettings;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon as TrayIconData, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use updater::APP_VERSION;

thread_local! {
    static MAIN_TRAY_ICON: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// Rilascia immediatamente le pagine di memoria non necessarie al sistema operativo Windows
/// portando il consumo RAM in System Tray a ~2-6 MB durante il gaming.
fn trim_process_working_set() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(
            hProcess: isize,
            dwMinimumWorkingSetSize: usize,
            dwMaximumWorkingSetSize: usize,
        ) -> i32;
    }
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn get_slint_hwnd(ui: &AppWindow) -> Option<windows_sys::Win32::Foundation::HWND> {
    if let Ok(handle) = ui.window().window_handle().window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
            return Some(win32.hwnd.get() as windows_sys::Win32::Foundation::HWND);
        }
    }
    get_window_hwnd_fallback()
}

fn get_window_hwnd_fallback() -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, GW_OWNER,
    };

    let my_pid = std::process::id();

    unsafe extern "system" fn find_hwnd_cb(hwnd: HWND, lparam: isize) -> i32 {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let my_pid = *(lparam as *const u32);
        if pid == my_pid && GetWindow(hwnd, GW_OWNER).is_null() {
            let mut title_buf = [0u16; 64];
            let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
            if len > 0 {
                let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                if title == "Monitor Manager" {
                    let target_ptr = (lparam as *mut usize).add(1) as *mut HWND;
                    *target_ptr = hwnd;
                    return 0;
                }
            }
        }
        1
    }

    let mut state: [usize; 2] = [my_pid as usize, 0];
    unsafe {
        EnumWindows(Some(find_hwnd_cb), state.as_mut_ptr() as isize);
        let found = state[1] as HWND;
        if !found.is_null() {
            Some(found)
        } else {
            None
        }
    }
}

fn is_windows_11_or_greater() -> bool {
    #[repr(C)]
    struct RtlOsVersionInfoW {
        dw_os_version_info_size: u32,
        dw_major_version: u32,
        dw_minor_version: u32,
        dw_build_number: u32,
        dw_platform_id: u32,
        sz_csd_version: [u16; 128],
    }

    type RtlGetVersionFn = unsafe extern "system" fn(*mut RtlOsVersionInfoW) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleA(lpModuleName: *const i8) -> isize;
        fn GetProcAddress(hModule: isize, lpProcName: *const i8) -> *const std::ffi::c_void;
    }

    unsafe {
        if let (Ok(dll), Ok(sym)) = (
            std::ffi::CString::new("ntdll.dll"),
            std::ffi::CString::new("RtlGetVersion"),
        ) {
            let h_nt = GetModuleHandleA(dll.as_ptr());
            if h_nt != 0 {
                let proc = GetProcAddress(h_nt, sym.as_ptr());
                if !proc.is_null() {
                    let rtl_get_version: RtlGetVersionFn = std::mem::transmute(proc);
                    let mut info: RtlOsVersionInfoW = std::mem::zeroed();
                    info.dw_os_version_info_size =
                        std::mem::size_of::<RtlOsVersionInfoW>() as u32;
                    if rtl_get_version(&mut info) == 0 {
                        return info.dw_build_number >= 22000;
                    }
                }
            }
        }
    }
    false
}

/// Applica la vera trasparenza Acrylic di Windows (DWM) combinata con angoli arrotondati e drop-shadow.
/// Su Windows 11 usa DWMWA_SYSTEMBACKDROP_TYPE + DwmExtendFrameIntoClientArea (disabilitando l'override winit).
/// Su Windows 10 ricorre a SetWindowCompositionAttribute.
fn set_window_acrylic(hwnd_opt: Option<windows_sys::Win32::Foundation::HWND>, enabled: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWM_BB_ENABLE, DWM_BLURBEHIND,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;

    let Some(hwnd) = hwnd_opt.or_else(get_window_hwnd_fallback) else {
        return;
    };

    unsafe {
        // 1. Disattiva il DwmEnableBlurBehindWindow forzato da winit sulle finestre trasparenti
        let bb = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE,
            fEnable: 0,
            hRgnBlur: std::ptr::null_mut(),
            fTransitionOnMaximized: 0,
        };
        let _ = DwmEnableBlurBehindWindow(hwnd, &bb);

        // 2. Assicura gli angoli arrotondati Windows 11 (DWMWCP_ROUND = 2)
        let corner_pref: u32 = 2;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner_pref as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // 3. Abilita Dark Mode DWM (DWMWA_USE_IMMERSIVE_DARK_MODE = 20) per Acrylic scuro autentico
        let dark_mode: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            20,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );

        if is_windows_11_or_greater() {
            // Estende il frame DWM a tutta la client area per la sfocatura nativa Acrylic
            let margins = MARGINS {
                cxLeftWidth: if enabled { -1 } else { 0 },
                cxRightWidth: if enabled { -1 } else { 0 },
                cyTopHeight: if enabled { -1 } else { 0 },
                cyBottomHeight: if enabled { -1 } else { 0 },
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

            // DWMSBT_TRANSIENTWINDOW = 3 (Acrylic sfocato), DWMSBT_NONE = 1 (opaco)
            let backdrop_type: u32 = if enabled { 3 } else { 1 };
            let _ = DwmSetWindowAttribute(
                hwnd,
                38, // DWMWA_SYSTEMBACKDROP_TYPE
                &backdrop_type as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            };
            let _ = SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            return;
        }

        // 4. Fallback Windows 10 tramite SetWindowCompositionAttribute (user32.dll)
        #[repr(C)]
        struct AccentPolicy {
            accent_state: u32,
            accent_flags: u32,
            gradient_color: u32,
            animation_id: u32,
        }
        #[repr(C)]
        struct WindowCompositionAttributeData {
            attribute: u32,
            data: *mut std::ffi::c_void,
            size_of_data: usize,
        }

        type SetWindowCompositionAttributeFn = unsafe extern "system" fn(
            hwnd: windows_sys::Win32::Foundation::HWND,
            data: *mut WindowCompositionAttributeData,
        ) -> i32;

        #[link(name = "kernel32")]
        extern "system" {
            fn GetModuleHandleA(lpModuleName: *const i8) -> isize;
            fn GetProcAddress(hModule: isize, lpProcName: *const i8) -> *const std::ffi::c_void;
        }

        if let (Ok(lib), Ok(fn_name)) = (
            std::ffi::CString::new("user32.dll"),
            std::ffi::CString::new("SetWindowCompositionAttribute"),
        ) {
            let h_user32 = GetModuleHandleA(lib.as_ptr());
            if h_user32 != 0 {
                let proc = GetProcAddress(h_user32, fn_name.as_ptr());
                if !proc.is_null() {
                    let set_attr: SetWindowCompositionAttributeFn = std::mem::transmute(proc);
                    let mut policy = AccentPolicy {
                        accent_state: if enabled { 4 /* ACCENT_ENABLE_ACRYLICBLURBEHIND */ } else { 0 },
                        accent_flags: if enabled { 2 } else { 0 },
                        gradient_color: if enabled { 0x99181c24 } else { 0 },
                        animation_id: 0,
                    };
                    let mut data = WindowCompositionAttributeData {
                        attribute: 19, // WCA_ACCENT_POLICY
                        data: &mut policy as *mut _ as *mut std::ffi::c_void,
                        size_of_data: std::mem::size_of::<AccentPolicy>(),
                    };
                    let _ = set_attr(hwnd, &mut data);
                }
            }
        }
    }
}

fn hit_test_resize_border(
    x: i32,
    y: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    edge: i32,
    corner: i32,
) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
    };

    if x < left || x >= right || y < top || y >= bottom {
        return None;
    }

    let on_left_edge = x < left + edge;
    let on_right_edge = x >= right - edge;
    let on_top_edge = y < top + edge;
    let on_bottom_edge = y >= bottom - edge;

    if (x < left + corner && on_top_edge) || (y < top + corner && on_left_edge) {
        return Some(HTTOPLEFT);
    }
    if (x >= right - corner && on_top_edge) || (y < top + corner && on_right_edge) {
        return Some(HTTOPRIGHT);
    }
    if (x < left + corner && on_bottom_edge) || (y >= bottom - corner && on_left_edge) {
        return Some(HTBOTTOMLEFT);
    }
    if (x >= right - corner && on_bottom_edge) || (y >= bottom - corner && on_right_edge) {
        return Some(HTBOTTOMRIGHT);
    }

    if on_left_edge {
        return Some(HTLEFT);
    }
    if on_right_edge {
        return Some(HTRIGHT);
    }
    if on_top_edge {
        return Some(HTTOP);
    }
    if on_bottom_edge {
        return Some(HTBOTTOM);
    }
    None
}

fn install_borderless_resize_subclass(hwnd: windows_sys::Win32::Foundation::HWND) {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GetWindowRect, IsZoomed, SetWindowLongPtrW, GWLP_WNDPROC,
        WM_NCHITTEST, WNDPROC,
    };

    static PREV_WNDPROC: AtomicIsize = AtomicIsize::new(0);
    static SUBCLASSED_HWND: AtomicIsize = AtomicIsize::new(0);

    unsafe extern "system" fn resize_wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let prev = PREV_WNDPROC.load(Ordering::Acquire);
        let prev_proc: WNDPROC = if prev != 0 {
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(
                    windows_sys::Win32::Foundation::HWND,
                    u32,
                    WPARAM,
                    LPARAM,
                ) -> LRESULT,
            >(prev))
        } else {
            None
        };

        if msg == WM_NCHITTEST && IsZoomed(hwnd) == 0 {
            let ret = if let Some(f) = prev_proc {
                CallWindowProcW(Some(f), hwnd, msg, wparam, lparam)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            };

            // Intercetta solo se Winit/DefWindowProc risponde HTCLIENT o HTNOWHERE
            if ret == 1 || ret == 0 {
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if GetWindowRect(hwnd, &mut rect) != 0 {
                    let x = (lparam as u32 & 0xFFFF) as i16 as i32;
                    let y = ((lparam as u32 >> 16) & 0xFFFF) as i16 as i32;
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    let edge = ((6 * dpi + 48) / 96).max(5);
                    let corner = ((12 * dpi + 48) / 96).max(10);
                    if let Some(ht) = hit_test_resize_border(
                        x,
                        y,
                        rect.left,
                        rect.top,
                        rect.right,
                        rect.bottom,
                        edge,
                        corner,
                    ) {
                        return ht as LRESULT;
                    }
                }
            }
            return ret;
        }

        if let Some(f) = prev_proc {
            CallWindowProcW(Some(f), hwnd, msg, wparam, lparam)
        } else {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    let hwnd_val = hwnd as isize;
    if hwnd_val == 0 || SUBCLASSED_HWND.load(Ordering::Acquire) == hwnd_val {
        return;
    }

    unsafe {
        let wnd_proc_ptr = resize_wnd_proc as *const () as isize;
        let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc_ptr);
        if prev != 0 && prev != wnd_proc_ptr {
            PREV_WNDPROC.store(prev, Ordering::Release);
            SUBCLASSED_HWND.store(hwnd_val, Ordering::Release);
        }
    }
}

fn show_and_focus_window(ui: &AppWindow, acrylic: bool) {
    let _ = ui.show();
    if let Some(hwnd) = get_slint_hwnd(ui) {
        install_borderless_resize_subclass(hwnd);
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
        };
        unsafe {
            if IsIconic(hwnd) != 0 {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        set_window_acrylic(Some(hwnd), acrylic);
    }
}

fn utf16_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn generate_app_icon_rgba() -> (Vec<u8>, u32, u32) {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let dx = (x as f32 - 15.5).abs();
            let dy = (y as f32 - 15.5).abs();
            if dx <= 14.5
                && dy <= 14.5
                && (dx < 11.5
                    || dy < 11.5
                    || ((dx - 11.5).powi(2) + (dy - 11.5).powi(2)).sqrt() <= 3.2)
            {
                let t = (x + y) as f32 / 64.0;
                // Sfondo Dark Matte Titanium con accento Coral Neon
                let r = (36.0 * (1.0 - t) + 24.0 * t) as u8;
                let g = (41.0 * (1.0 - t) + 27.0 * t) as u8;
                let b = (52.0 * (1.0 - t) + 35.0 * t) as u8;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;

                let in_screen_border = (x >= 6 && x <= 25 && (y == 7 || y == 19))
                    || (y >= 7 && y <= 19 && (x == 6 || x == 25));
                let in_stand_neck = (x == 15 || x == 16) && (y >= 20 && y <= 23);
                let in_stand_base = y == 24 && (x >= 11 && x <= 20);

                if in_screen_border || in_stand_neck || in_stand_base {
                    rgba[idx] = 240;
                    rgba[idx + 1] = 90;
                    rgba[idx + 2] = 70;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    (rgba, w, h)
}

fn perform_uninstall() {
    use std::fs;
    use std::path::PathBuf;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        KEY_ALL_ACCESS,
    };

    if let Ok(userprofile) = env::var("USERPROFILE") {
        let desktop_lnk = PathBuf::from(&userprofile)
            .join("Desktop")
            .join("Monitor Manager.lnk");
        let _ = fs::remove_file(desktop_lnk);
    }
    if let Ok(appdata) = env::var("APPDATA") {
        let start_lnk = PathBuf::from(&appdata)
            .join("Microsoft\\Windows\\Start Menu\\Programs")
            .join("Monitor Manager.lnk");
        let _ = fs::remove_file(start_lnk);
    }

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let run_key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        if RegOpenKeyExW(HKEY_CURRENT_USER, run_key.as_ptr(), 0, KEY_ALL_ACCESS, &mut hkey) == 0 {
            let val: Vec<u16> = "MonitorManager\0".encode_utf16().collect();
            let _ = RegDeleteValueW(hkey, val.as_ptr());
            let _ = RegCloseKey(hkey);
        }

        let uninst_parent: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\0"
            .encode_utf16()
            .collect();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            uninst_parent.as_ptr(),
            0,
            KEY_ALL_ACCESS,
            &mut hkey,
        ) == 0
        {
            let child: Vec<u16> = "MonitorManager\0".encode_utf16().collect();
            let _ = RegDeleteTreeW(hkey, child.as_ptr());
            let _ = RegCloseKey(hkey);
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cmd_arg = format!(
                "ping 127.0.0.1 -n 2 > nul & rmdir /s /q \"{}\"",
                dir.display()
            );
            let _ = std::process::Command::new("cmd")
                .args(["/C", &cmd_arg])
                .spawn();
        }
    }
}

fn to_ui_monitor(m: &MonitorInfo, vcps_probing: bool) -> UiMonitor {
    let available_hz_vec: Vec<i32> = m
        .os_info
        .available_refresh_rates
        .iter()
        .map(|&h| h as i32)
        .collect();
    let discovered_vec: Vec<UiDiscoveredVcp> = m
        .discovered_vcps
        .iter()
        .map(|dv| {
            let opts: Vec<UiVcpOption> = dv
                .options
                .iter()
                .map(|o| UiVcpOption {
                    value: o.value as i32,
                    label: SharedString::from(o.label.as_str()),
                })
                .collect();
            UiDiscoveredVcp {
                code: dv.code as i32,
                hex_code: SharedString::from(format!("0x{:02X}", dv.code)),
                name_it: SharedString::from(dv.name_it.as_str()),
                name_en: SharedString::from(dv.name_en.as_str()),
                category: SharedString::from(dv.category.as_str()),
                is_continuous: dv.is_continuous,
                current: dv.current as i32,
                max: dv.max.max(1) as i32,
                is_read_only: dv.is_read_only,
                is_critical: dv.is_critical,
                options: ModelRc::from(Rc::new(VecModel::from(opts))),
            }
        })
        .collect();

    UiMonitor {
        id: m.id as i32,
        name: SharedString::from(m.name.as_str()),
        brand: SharedString::from(m.brand.as_str()),
        resolution: SharedString::from(m.resolution.as_str()),
        is_primary: m.is_primary,
        ddc_supported: m.ddc_supported,
        is_internal_wmi: m.is_internal_wmi,
        brightness: m.brightness.current as i32,
        brightness_max: m.brightness.max.max(1) as i32,
        brightness_sup: m.brightness.supported,
        contrast: m.contrast.current as i32,
        contrast_max: m.contrast.max.max(1) as i32,
        contrast_sup: m.contrast.supported,
        sharpness: m.sharpness.current as i32,
        sharpness_max: m.sharpness.max.max(1) as i32,
        sharpness_sup: m.sharpness.supported,
        saturation: m.saturation.current as i32,
        saturation_max: m.saturation.max.max(1) as i32,
        saturation_sup: m.saturation.supported,
        red_gain: m.red_gain.current as i32,
        green_gain: m.green_gain.current as i32,
        blue_gain: m.blue_gain.current as i32,
        rgb_sup: m.red_gain.supported || m.green_gain.supported || m.blue_gain.supported,
        color_temp: m.color_temp.current as i32,
        color_temp_sup: m.color_temp.supported,
        volume: m.volume.current as i32,
        volume_sup: m.volume.supported,
        input_source: m.input_source.current as i32,
        input_sup: m.input_source.supported,
        current_hz: m.os_info.current_refresh_rate as i32,
        available_hz: ModelRc::from(Rc::new(VecModel::from(available_hz_vec))),
        hdr_supported: m.os_info.hdr_supported,
        hdr_enabled: m.os_info.hdr_enabled,
        mccs_version: SharedString::from(m.mccs_version.as_str()),
        vcps_probing: vcps_probing && m.ddc_supported && !m.is_internal_wmi,
        discovered_vcps: ModelRc::from(Rc::new(VecModel::from(discovered_vec))),
        sw_lut_enabled: m.software_lut.enabled,
        sw_gamma: m.software_lut.gamma_pct,
        sw_warmth: m.software_lut.warmth,
        sw_contrast: m.software_lut.contrast_pct,
        sw_red: m.software_lut.red_pct,
        sw_green: m.software_lut.green_pct,
        sw_blue: m.software_lut.blue_pct,
        is_asus: m.asus.is_asus,
        is_oled: m.asus.is_oled_panel,
        dual_mode_sup: m.asus.dual_mode_supported,
        gamevisual_sup: m.asus.gamevisual_supported,
        gamevisual_mode: m.asus.gamevisual_mode,
        variable_od: m.asus.variable_od as i32,
        variable_od_max: m.asus.variable_od_max as i32,
        variable_od_sup: m.asus.variable_od_supported,
        blue_light: m.asus.blue_light_filter as i32,
        blue_light_max: m.asus.blue_light_max as i32,
        blue_light_sup: m.asus.blue_light_supported,
        vivid_pixel: m.asus.vivid_pixel as i32,
        vivid_pixel_max: m.asus.vivid_pixel_max as i32,
        vivid_pixel_sup: m.asus.vivid_pixel_supported,
        elmb: m.asus.elmb as i32,
        elmb_max: m.asus.elmb_max as i32,
        elmb_sup: m.asus.elmb_supported,
        oled_flicker: m.asus.oled_anti_flicker as i32,
        oled_flicker_max: m.asus.oled_anti_flicker_max as i32,
        shadow_boost: m.asus.shadow_boost as i32,
        shadow_boost_max: m.asus.shadow_boost_max as i32,
        shadow_boost_sup: m.asus.shadow_boost_supported,
        dynamic_shadow_boost: m.asus.dynamic_shadow_boost,
        dynamic_shadow_boost_sup: m.asus.dynamic_shadow_boost_supported,
        frame_rate_boost: m.asus.frame_rate_boost,
        vrr_enabled: m.asus.vrr_enabled,
        vrr_sup: m.asus.vrr_supported,
        aspect_control: m.asus.aspect_control as i32,
        aspect_control_sup: m.asus.aspect_control_supported,
        color_space: m.asus.color_space as i32,
        color_space_sup: m.asus.color_space_supported,
        crosshair_style: m.asus.crosshair_style as i32,
        timer_preset: m.asus.timer_preset as i32,
        fps_counter: m.asus.fps_counter_mode != 0,
        alignment_grid: m.asus.alignment_enabled,
        caps_raw: SharedString::from(m.asus.caps_raw.as_str()),
        mpu_ver: SharedString::from(m.asus.mpu_ver.as_str()),
    }
}

fn push_monitors_to_ui(ui: &AppWindow, list: &[MonitorInfo], vcps_probing: bool) {
    let vec_ui: Vec<UiMonitor> = list.iter().map(|m| to_ui_monitor(m, vcps_probing)).collect();
    ui.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
    ui.invoke_sync_current_values();
}

fn hotkey_labels(action: &str) -> (&'static str, &'static str) {
    match action {
        "brightness_up" => ("Aumenta Luminosità (+10)", "Brightness Up (+10)"),
        "brightness_down" => ("Riduci Luminosità (-10)", "Brightness Down (-10)"),
        "cycle_gamevisual" => ("Cicla Modalità GameVisual", "Cycle GameVisual Mode"),
        "toggle_crosshair" => ("Attiva / Disattiva Mirino GamePlus", "Toggle GamePlus Crosshair"),
        "toggle_hdr" => ("Attiva / Disattiva Display HDR", "Toggle Display HDR"),
        "cycle_input" => ("Cambia Sorgente Input (DP/HDMI)", "Cycle Input Source (DP/HDMI)"),
        "night_care" => ("Filtro Luce Blu (Cura Notturna)", "Blue Light Night Care"),
        _ => ("Scorciatoia Personalizzata", "Custom Shortcut"),
    }
}

fn sync_hotkeys_to_ui(ui: &AppWindow, cfg: &DwcConfig) {
    let vec: Vec<UiHotkeyItem> = cfg
        .hotkeys
        .iter()
        .map(|h| {
            let (it, en) = hotkey_labels(&h.action);
            UiHotkeyItem {
                id: h.id,
                action: SharedString::from(h.action.as_str()),
                label_it: SharedString::from(it),
                label_en: SharedString::from(en),
                shortcut: SharedString::from(h.shortcut_label()),
                enabled: h.enabled,
            }
        })
        .collect();
    ui.set_hotkeys_enabled(cfg.hotkeys_enabled);
    ui.set_hotkey_items(ModelRc::from(Rc::new(VecModel::from(vec))));
}

fn sync_rules_to_ui(ui: &AppWindow, cfg: &DwcConfig) {
    let vec: Vec<UiAppRule> = cfg
        .app_rules
        .iter()
        .enumerate()
        .map(|(i, r)| UiAppRule {
            index: i as i32,
            exe_name: SharedString::from(r.exe_name.as_str()),
            display_name: SharedString::from(r.display_name.as_str()),
            gamevisual_mode: r.gamevisual_mode,
            brightness: r.brightness,
            blue_light: r.blue_light,
            variable_od: r.variable_od,
            shadow_boost: r.shadow_boost,
            enabled: r.enabled,
        })
        .collect();
    ui.set_app_tweaker_enabled(cfg.app_tweaker_enabled);
    ui.set_app_rules(ModelRc::from(Rc::new(VecModel::from(vec))));
}

fn push_open_windows_to_ui(ui_weak: slint::Weak<AppWindow>) {
    thread::spawn(move || {
        let wins = win_system::enumerate_open_windows();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(u) = ui_weak.upgrade() {
                let entries: Vec<UiWindowEntry> = wins
                    .into_iter()
                    .map(|w| UiWindowEntry {
                        hwnd_str: SharedString::from(w.hwnd.to_string()),
                        title: SharedString::from(w.title),
                        exe_name: SharedString::from(w.exe_name),
                    })
                    .collect();
                u.set_open_windows(ModelRc::from(Rc::new(VecModel::from(entries))));
            }
        });
    });
}

fn active_ui_monitor(u: &AppWindow) -> Option<UiMonitor> {
    let mons = u.get_monitors();
    let count = mons.row_count();
    if count == 0 {
        return None;
    }
    let idx = (u.get_selected_monitor_index().max(0) as usize).min(count - 1);
    mons.row_data(idx)
}

fn sync_gameplus_overlay(
    overlay: &GamePlusOverlayEngine,
    dwc: &DwcConfig,
    timer_paused: bool,
    mon: &MonitorInfo,
) {
    let (_work, full) = win_system::get_monitor_rects(mon.id).unwrap_or(([0, 0, 1920, 1080], [0, 0, 1920, 1080]));
    let any_active = mon.asus.crosshair_style > 0
        || mon.asus.timer_preset > 0
        || mon.asus.fps_counter_mode != 0
        || mon.asus.alignment_enabled;
    let delivery = dwc.gameplus_delivery_mode.as_str();
    let needs_sw_overlay = match delivery {
        "hardware" => false,
        "overlay" => any_active,
        _ => {
            if !mon.asus.is_asus || !mon.ddc_supported {
                any_active
            } else {
                // On ASUS hardware in Auto mode: stopwatch (6), FPS HUD, and edge alignment grid always get OS overlay support; crosshair/timer 1..5 use monitor OSD cleanly without overlapping unless overlay-only is chosen
                mon.asus.timer_preset == 6
                    || mon.asus.fps_counter_mode != 0
                    || mon.asus.alignment_enabled
            }
        }
    };
    let overlay_crosshair = if delivery == "overlay" || !mon.asus.is_asus || !mon.ddc_supported {
        mon.asus.crosshair_style as i32
    } else {
        0
    };
    let overlay_timer = if delivery == "overlay" || !mon.asus.is_asus || !mon.ddc_supported || mon.asus.timer_preset == 6 {
        mon.asus.timer_preset as i32
    } else {
        0
    };
    overlay.update_config(OverlayConfig {
        enabled: needs_sw_overlay,
        target_rect: full,
        crosshair_style: overlay_crosshair.max(0) as u32,
        timer_mode: overlay_timer.max(0) as u32,
        timer_paused,
        fps_counter: mon.asus.fps_counter_mode != 0 && delivery != "hardware",
        alignment_grid: mon.asus.alignment_enabled && delivery != "hardware",
        offset_x: dwc.gameplus_offset_x,
        offset_y: dwc.gameplus_offset_y,
    });
}

fn apply_gamevisual_mode(
    ddc: &DdcEngine,
    monitor_id: usize,
    mode_idx: i32,
    sync_all: bool,
    dwc_state: &Arc<Mutex<DwcConfig>>,
    ui_weak: &slint::Weak<AppWindow>,
) {
    let mode = GameVisualMode::from_index(mode_idx);
    if matches!(mode, GameVisualMode::AiVisual) {
        let mut cfg = dwc_state.lock().unwrap();
        cfg.ai_visual_enabled = true;
        cfg.save();
        let uw = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(u) = uw.upgrade() {
                u.set_ai_visual_active(true);
            }
        });
    } else if matches!(mode, GameVisualMode::Hdr) {
        let _ = ddc.set_monitor_hdr(monitor_id, true);
        if let Some(v) = mode.hardware_vcp_value() {
            ddc.send_discrete_vcp(monitor_id, avcp::GAME_VISUAL, v);
        }
    } else if let Some(v) = mode.hardware_vcp_value() {
        ddc.set_vcp_value(monitor_id, avcp::GAME_VISUAL, v, sync_all);
    }

    let uw = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(u) = uw.upgrade() {
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if sync_all || row.id as usize == monitor_id {
                        row.gamevisual_mode = mode_idx;
                        monitors_rc.set_row_data(i, row);
                    }
                }
            }
        }
    });
}

fn apply_gamevisual_setting_key(
    ddc: &DdcEngine,
    monitor_id: usize,
    key: &str,
    value: i32,
    sync_all: bool,
    ui_weak: &slint::Weak<AppWindow>,
) {
    let val = value.max(0) as u32;
    let mons = ddc.get_cached_monitors();
    let target = mons.iter().find(|m| m.id == monitor_id);
    match key {
        "brightness" => ddc.set_vcp_value(monitor_id, avcp::BRIGHTNESS, val, sync_all),
        "contrast" => ddc.set_vcp_value(monitor_id, avcp::CONTRAST, val, sync_all),
        "variable_od" => {
            let code = target.map(|m| m.asus.variable_od_vcp).unwrap_or(avcp::VARIABLE_OD);
            ddc.set_vcp_value(monitor_id, code, val, sync_all);
        }
        "blue_light" => {
            let code = target.map(|m| m.asus.blue_light_vcp).unwrap_or(avcp::BLUE_LIGHT_FILTER);
            ddc.set_vcp_value(monitor_id, code, val, sync_all);
        }
        "vivid_pixel" => ddc.set_vcp_value(monitor_id, avcp::VIVID_PIXEL, val, sync_all),
        "elmb" => ddc.set_vcp_value(monitor_id, avcp::ELMB, val, sync_all),
        "oled_flicker" => ddc.set_vcp_value(monitor_id, avcp::OLED_ANTI_FLICKER, val, sync_all),
        "shadow_boost" => {
            let code = target.map(|m| m.asus.shadow_boost_vcp).unwrap_or(avcp::SHADOW_BOOST);
            ddc.set_vcp_value(monitor_id, code, val, sync_all);
        }
        "dynamic_shadow_boost" => {
            let code = target.map(|m| m.asus.dynamic_shadow_boost_vcp).unwrap_or(avcp::DYNAMIC_SHADOW_BOOST);
            ddc.send_discrete_vcp(monitor_id, code, val.min(1));
        }
        "frame_rate_boost" => ddc.send_discrete_vcp(monitor_id, avcp::FRAME_RATE_BOOST, val.min(1)),
        "vrr_enabled" => {
            let code = target.map(|m| m.asus.vrr_vcp).unwrap_or(avcp::VRR_ENABLE);
            ddc.send_discrete_vcp(monitor_id, code, val.min(1));
        }
        "aspect_control" => ddc.send_discrete_vcp(monitor_id, avcp::ASPECT_CONTROL, val),
        "color_space" => ddc.send_discrete_vcp(monitor_id, avcp::COLOR_SPACE, val),
        "saturation" => ddc.set_vcp_value(monitor_id, avcp::SATURATION, val, sync_all),
        "color_preset" => ddc.send_discrete_vcp(monitor_id, avcp::COLOR_PRESET, val),
        "red" => ddc.set_vcp_value(monitor_id, avcp::RED_GAIN, val, sync_all),
        "green" => ddc.set_vcp_value(monitor_id, avcp::GREEN_GAIN, val, sync_all),
        "blue" => ddc.set_vcp_value(monitor_id, avcp::BLUE_GAIN, val, sync_all),
        _ => {}
    }

    let key_owned = key.to_string();
    let uw = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(u) = uw.upgrade() {
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if sync_all || row.id as usize == monitor_id {
                        match key_owned.as_str() {
                            "brightness" => row.brightness = value,
                            "contrast" => row.contrast = value,
                            "variable_od" => row.variable_od = value,
                            "blue_light" => row.blue_light = value,
                            "vivid_pixel" => {
                                row.vivid_pixel = value;
                                row.sharpness = value;
                            }
                            "elmb" => row.elmb = value,
                            "oled_flicker" => row.oled_flicker = value,
                            "shadow_boost" => row.shadow_boost = value,
                            "dynamic_shadow_boost" => row.dynamic_shadow_boost = value != 0,
                            "frame_rate_boost" => row.frame_rate_boost = value != 0,
                            "vrr_enabled" => row.vrr_enabled = value != 0,
                            "aspect_control" => row.aspect_control = value,
                            "color_space" => row.color_space = value,
                            "saturation" => row.saturation = value,
                            "color_preset" => row.color_temp = value,
                            "red" => row.red_gain = value,
                            "green" => row.green_gain = value,
                            "blue" => row.blue_gain = value,
                            _ => {}
                        }
                        monitors_rc.set_row_data(i, row);
                    }
                }
            }
            u.invoke_sync_current_values();
        }
    });
}

fn run_two_phase_scan(
    ddc: &Arc<DdcEngine>,
    order: &[String],
    ui_weak: &slint::Weak<AppWindow>,
) {
    let list = ddc.scan_monitors_ordered(order);
    let uw1 = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(u) = uw1.upgrade() {
            push_monitors_to_ui(&u, &list, true);
        }
    });

    let probed = ddc.probe_capabilities_for_all();
    let uw2 = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(u) = uw2.upgrade() {
            push_monitors_to_ui(&u, &probed, false);
        }
    });
}

fn drop_tray_icon() {
    MAIN_TRAY_ICON.with(|cell| {
        let _ = cell.borrow_mut().take();
    });
}

fn main() {
    if env::args().any(|a| a == "--uninstall") {
        perform_uninstall();
        return;
    }

    // Guardia Single-Instance rigida: impedisce icone duplicate in System Tray
    // e risveglia la finestra già aperta (o in tray) se l'utente rilancia l'app.
    let (instance_mutex, wake_event) = unsafe {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetLastError, ERROR_ALREADY_EXISTS,
        };
        use windows_sys::Win32::System::Threading::{
            CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE,
        };

        let mutex_name = utf16_z("Local\\MonitorManager_SingleInstance_Mutex_v1");
        let event_name = utf16_z("Local\\MonitorManager_WakeUp_Event_v1");

        let mutex_handle = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        if !mutex_handle.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let existing_ev = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
            if !existing_ev.is_null() {
                let _ = SetEvent(existing_ev);
                let _ = CloseHandle(existing_ev);
            }
            let _ = CloseHandle(mutex_handle);
            return;
        }

        let ev_handle = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
        (mutex_handle, ev_handle)
    };
    let shutting_down = Arc::new(AtomicBool::new(false));

    updater::cleanup_old_executable();

    let start_minimized = env::args().any(|a| a == "--minimized");
    let settings_state = Arc::new(Mutex::new(AppSettings::load()));
    let dwc_state = Arc::new(Mutex::new(DwcConfig::load()));
    let ddc_engine = Arc::new(DdcEngine::new());
    let overlay_engine = Arc::new(GamePlusOverlayEngine::spawn());
    let compare_backup: Arc<Mutex<Option<DisplayProfileSnapshot>>> = Arc::new(Mutex::new(None));
    let timer_paused = Arc::new(AtomicBool::new(false));
    let latest_download_url: Arc<Mutex<Option<(Option<String>, String, u64)>>> =
        Arc::new(Mutex::new(None));
    let update_checking = Arc::new(AtomicBool::new(false));
    let update_downloading = Arc::new(AtomicBool::new(false));
    let update_cancel_flag = Arc::new(AtomicBool::new(false));
    let restart_after_exit: Arc<Mutex<Option<std::path::PathBuf>>> =
        Arc::new(Mutex::new(None));

    let ui = AppWindow::new().expect("Impossibile inizializzare la finestra nativa Slint");

    // Inizializza impostazioni nella UI
    {
        let s = settings_state.lock().unwrap();
        ui.set_app_version(SharedString::from(APP_VERSION));
        ui.set_lang(SharedString::from(s.language.as_str()));
        ui.set_accent_palette(SharedString::from(s.accent_palette.as_str()));
        ui.set_glow_style(SharedString::from(s.glow_style.as_str()));
        ui.set_layout_density(SharedString::from(s.layout_density.as_str()));
        ui.set_layout_mode(SharedString::from(s.layout_mode.as_str()));
        ui.set_active_view(SharedString::from("home"));
        ui.set_sync_monitors(s.sync_monitors);
        ui.set_hide_unsupported(s.hide_unsupported);
        ui.set_close_to_tray(s.close_to_tray);
        ui.set_start_with_windows(s.start_with_windows);
        ui.set_auto_check_updates(s.auto_check_updates);
        ui.set_acrylic_enabled(s.acrylic_transparency);
        ui.set_animations_enabled(s.animations_enabled);
    }

    // Inizializza stato DisplayWidget Center & Windows nella UI
    {
        let dwc = dwc_state.lock().unwrap();
        let theme = win_system::get_windows_theme();
        ui.set_taskbar_autohide(win_system::get_taskbar_autohide());
        ui.set_win_system_light(theme.system_light);
        ui.set_win_apps_light(theme.apps_light);
        ui.set_power_saving_mode(dwc.power_saving_mode);
        ui.set_update_policy(SharedString::from(dwc.update_policy.as_str()));
        ui.set_gameplus_delivery(SharedString::from(dwc.gameplus_delivery_mode.as_str()));
        ui.set_gameplus_offset_x(dwc.gameplus_offset_x);
        ui.set_gameplus_offset_y(dwc.gameplus_offset_y);
        ui.set_ai_visual_active(dwc.ai_visual_enabled);
        ui.set_ai_night_schedule(dwc.ai_night_schedule);
        ui.set_ai_night_start(dwc.ai_night_start_hour as i32);
        ui.set_ai_night_end(dwc.ai_night_end_hour as i32);
        sync_hotkeys_to_ui(&ui, &dwc);
        sync_rules_to_ui(&ui, &dwc);
    }

    // Callback di resync hardware automatico da DDC worker (es. cambio GameVisual -> reload slider reali)
    {
        let uw_resync = ui.as_weak();
        ddc_engine.set_resync_callback(Arc::new(move |list| {
            let uw_c = uw_resync.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw_c.upgrade() {
                    push_monitors_to_ui(&u, &list, false);
                }
            });
        }));
    }

    // Configurazione System Tray
    let (icon_rgba, icon_w, icon_h) = generate_app_icon_rgba();
    let tray_menu = Menu::new();
    let item_open = MenuItem::new("Apri Monitor Manager", true, None);
    let item_rescan = MenuItem::new("Rileva Monitor Collegati", true, None);
    let item_sep = PredefinedMenuItem::separator();
    let item_quit = MenuItem::new("Esci completamente", true, None);

    let _ = tray_menu.append(&item_open);
    let _ = tray_menu.append(&item_rescan);
    let _ = tray_menu.append(&item_sep);
    let _ = tray_menu.append(&item_quit);

    let open_id = item_open.id().clone();
    let rescan_id = item_rescan.id().clone();
    let quit_id = item_quit.id().clone();

    let tray_icon_data =
        TrayIconData::from_rgba(icon_rgba, icon_w, icon_h).expect("Errore creazione icona tray");

    let tray_built = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(format!("Monitor Manager v{} (Dark Matte Titanium)", APP_VERSION))
        .with_icon(tray_icon_data)
        .build()
        .ok();
    MAIN_TRAY_ICON.with(|cell| {
        *cell.borrow_mut() = tray_built;
    });

    // Thread listener risveglio Single-Instance (0% CPU bloccato su WaitForSingleObject)
    if !wake_event.is_null() {
        let uw_wake = ui.as_weak();
        let s_wake = Arc::clone(&settings_state);
        let sd_wake = Arc::clone(&shutting_down);
        let ev_int = wake_event as isize;
        thread::spawn(move || {
            use windows_sys::Win32::Foundation::HANDLE;
            use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
            let ev_h = ev_int as HANDLE;
            loop {
                unsafe {
                    WaitForSingleObject(ev_h, INFINITE);
                }
                if sd_wake.load(Ordering::SeqCst) {
                    break;
                }
                let uw_c = uw_wake.clone();
                let s_c = Arc::clone(&s_wake);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw_c.upgrade() {
                        let acrylic = s_c.lock().unwrap().acrylic_transparency;
                        show_and_focus_window(&u, acrylic);
                    }
                });
            }
        });
    }

    // Gestione Menu System Tray
    let ui_weak_menu = ui.as_weak();
    let ddc_menu = Arc::clone(&ddc_engine);
    let settings_menu = Arc::clone(&settings_state);
    let sd_menu_quit = Arc::clone(&shutting_down);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            let uw = ui_weak_menu.clone();
            let s_menu = Arc::clone(&settings_menu);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let acrylic = s_menu.lock().unwrap().acrylic_transparency;
                    show_and_focus_window(&u, acrylic);
                }
            });
        } else if event.id == rescan_id {
            let uw = ui_weak_menu.clone();
            let ddc_c = Arc::clone(&ddc_menu);
            let s_c = Arc::clone(&settings_menu);
            thread::spawn(move || {
                let order = s_c.lock().unwrap().monitor_order.clone();
                let uw_show = uw.clone();
                let s_show = Arc::clone(&s_c);
                run_two_phase_scan(&ddc_c, &order, &uw);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw_show.upgrade() {
                        let acrylic = s_show.lock().unwrap().acrylic_transparency;
                        show_and_focus_window(&u, acrylic);
                    }
                });
            });
        } else if event.id == quit_id {
            sd_menu_quit.store(true, Ordering::SeqCst);
            let _ = slint::invoke_from_event_loop(move || {
                drop_tray_icon();
                let _ = slint::quit_event_loop();
            });
        }
    }));

    // Click sinistro sull'icona della System Tray riapre immediatamente la finestra
    let ui_weak_tray = ui.as_weak();
    let settings_tray = Arc::clone(&settings_state);
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let should_open = matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        );
        if should_open {
            let uw = ui_weak_tray.clone();
            let s_tr = Arc::clone(&settings_tray);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let acrylic = s_tr.lock().unwrap().acrylic_transparency;
                    show_and_focus_window(&u, acrylic);
                }
            });
        }
    }));

    // Chiusura finestra -> riduce nella System Tray e libera la RAM (Working Set Trim)
    let settings_close = Arc::clone(&settings_state);
    let ui_weak_close = ui.as_weak();
    let sd_close_req = Arc::clone(&shutting_down);
    ui.window().on_close_requested(move || {
        let close_to_tray = settings_close.lock().unwrap().close_to_tray;
        if close_to_tray {
            if let Some(u) = ui_weak_close.upgrade() {
                let _ = u.hide();
            }
            trim_process_working_set();
            slint::CloseRequestResponse::KeepWindowShown
        } else {
            sd_close_req.store(true, Ordering::SeqCst);
            drop_tray_icon();
            let _ = slint::quit_event_loop();
            slint::CloseRequestResponse::HideWindow
        }
    });

    // Callback barra superiore borderless: Riduci a icona (—)
    let ui_weak_min = ui.as_weak();
    ui.on_minimize_window(move || {
        if let Some(u) = ui_weak_min.upgrade() {
            u.window().set_minimized(true);
            trim_process_working_set();
        }
    });

    // Callback barra superiore borderless: Ingrandisci / Ripristina (▢)
    // Con salvataggio dimensione precedente per prevenire il collasso o blocco su Windows
    let ui_weak_max = ui.as_weak();
    let s_max = Arc::clone(&settings_state);
    let last_window_size = Arc::new(Mutex::new(None::<(u32, u32)>));
    let last_ws_clone = Arc::clone(&last_window_size);
    ui.on_toggle_maximize_window(move || {
        if let Some(u) = ui_weak_max.upgrade() {
            let is_max = u.window().is_maximized();
            if !is_max {
                let sz = u.window().size();
                if sz.width > 0 && sz.height > 0 {
                    *last_ws_clone.lock().unwrap() = Some((sz.width, sz.height));
                }
                u.window().set_maximized(true);
                u.set_window_maximized(true);
            } else {
                u.window().set_maximized(false);
                u.set_window_maximized(false);
                let (w, h) = last_ws_clone.lock().unwrap().unwrap_or((980, 600));
                let w = w.max(860);
                let h = h.max(540);
                u.window().set_size(slint::PhysicalSize::new(w, h));
                let acrylic = s_max.lock().unwrap().acrylic_transparency;
                set_window_acrylic(get_slint_hwnd(&u), acrylic);
            }
        }
    });

    // Callback barra superiore borderless: Chiudi / Minimizza nella System Tray (✕)
    let settings_btn_close = Arc::clone(&settings_state);
    let ui_weak_btn_close = ui.as_weak();
    let sd_btn_close = Arc::clone(&shutting_down);
    ui.on_close_window(move || {
        let close_to_tray = settings_btn_close.lock().unwrap().close_to_tray;
        if close_to_tray {
            if let Some(u) = ui_weak_btn_close.upgrade() {
                let _ = u.hide();
            }
            trim_process_working_set();
        } else {
            sd_btn_close.store(true, Ordering::SeqCst);
            drop_tray_icon();
            let _ = slint::quit_event_loop();
        }
    });

    // Callback: Modifica codice VCP ultra-veloce (Zero lag, invio asincrono coalesced)
    let ddc_vcp = Arc::clone(&ddc_engine);
    let ui_weak_vcp = ui.as_weak();
    ui.on_set_vcp(move |monitor_id, vcp_code, value| {
        if let Some(u) = ui_weak_vcp.upgrade() {
            let sync_all = u.get_sync_monitors();
            ddc_vcp.set_vcp_value(
                monitor_id as usize,
                vcp_code as u8,
                value as u32,
                sync_all,
            );

            // Sincronizza istantaneamente i modelli UI tra tutte le sezioni (Home, Dashboard, Presets)
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if sync_all || row.id == monitor_id {
                        if row.is_internal_wmi && vcp_code != 16 {
                            continue;
                        }
                        match vcp_code {
                            16 => row.brightness = value,
                            18 => row.contrast = value,
                            135 => row.sharpness = value,
                            138 => row.saturation = value,
                            22 => row.red_gain = value,
                            24 => row.green_gain = value,
                            26 => row.blue_gain = value,
                            20 => row.color_temp = value,
                            98 => row.volume = value,
                            96 => row.input_source = value,
                            _ => {}
                        }
                        for j in 0..row.discovered_vcps.row_count() {
                            if let Some(mut dv) = row.discovered_vcps.row_data(j) {
                                if dv.code == vcp_code {
                                    dv.current = value;
                                    row.discovered_vcps.set_row_data(j, dv);
                                    break;
                                }
                            }
                        }
                        monitors_rc.set_row_data(i, row);

                        if u.get_selected_monitor_index() == i as i32 {
                            match vcp_code {
                                16 => u.set_current_brightness(value),
                                18 => u.set_current_contrast(value),
                                135 => u.set_current_sharpness(value),
                                138 => u.set_current_saturation(value),
                                22 => u.set_current_red_gain(value),
                                24 => u.set_current_green_gain(value),
                                26 => u.set_current_blue_gain(value),
                                98 => u.set_current_volume(value),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    });

    // Callback: Preset rapido (Luminosità + Contrasto)
    let ddc_preset = Arc::clone(&ddc_engine);
    let ui_weak_preset = ui.as_weak();
    ui.on_apply_preset(move |monitor_id, lum, con| {
        if let Some(u) = ui_weak_preset.upgrade() {
            let sync_all = u.get_sync_monitors();
            ddc_preset.set_vcp_value(
                monitor_id as usize,
                ddc::VCP_BRIGHTNESS,
                lum as u32,
                sync_all,
            );
            ddc_preset.set_vcp_value(
                monitor_id as usize,
                ddc::VCP_CONTRAST,
                con as u32,
                sync_all,
            );

            // Sincronizza modelli UI
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if sync_all || row.id == monitor_id {
                        if row.brightness_sup {
                            row.brightness = lum;
                        }
                        if row.contrast_sup {
                            row.contrast = con;
                        }
                        monitors_rc.set_row_data(i, row);

                        if u.get_selected_monitor_index() == i as i32 {
                            if let Some(updated_row) = monitors_rc.row_data(i) {
                                u.set_current_brightness(updated_row.brightness);
                                u.set_current_contrast(updated_row.contrast);
                            }
                        }
                    }
                }
            }
        }
    });

    // Callback: Reset RGB al 50%
    let ddc_rgb = Arc::clone(&ddc_engine);
    let ui_weak_rgb = ui.as_weak();
    ui.on_reset_rgb(move |monitor_id| {
        if let Some(u) = ui_weak_rgb.upgrade() {
            let sync_all = u.get_sync_monitors();
            ddc_rgb.set_vcp_value(monitor_id as usize, ddc::VCP_RED_GAIN, 50, sync_all);
            ddc_rgb.set_vcp_value(monitor_id as usize, ddc::VCP_GREEN_GAIN, 50, sync_all);
            ddc_rgb.set_vcp_value(monitor_id as usize, ddc::VCP_BLUE_GAIN, 50, sync_all);

            // Sincronizza modelli UI
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if (sync_all || row.id == monitor_id) && row.rgb_sup {
                        row.red_gain = 50;
                        row.green_gain = 50;
                        row.blue_gain = 50;
                        monitors_rc.set_row_data(i, row);

                        if u.get_selected_monitor_index() == i as i32 {
                            u.set_current_red_gain(50);
                            u.set_current_green_gain(50);
                            u.set_current_blue_gain(50);
                        }
                    }
                }
            }
        }
    });

    // Callback: Rescan Monitor (2-phase: veloce + capabilities)
    let ddc_rescan = Arc::clone(&ddc_engine);
    let settings_rescan = Arc::clone(&settings_state);
    let ui_weak_rescan = ui.as_weak();
    ui.on_rescan_monitors(move || {
        let ddc_c = Arc::clone(&ddc_rescan);
        let s_c = Arc::clone(&settings_rescan);
        let uw = ui_weak_rescan.clone();
        thread::spawn(move || {
            let order = s_c.lock().unwrap().monitor_order.clone();
            run_two_phase_scan(&ddc_c, &order, &uw);
        });
    });

    // Callback: Riscansiona Capabilities MCCS / Brand VCP
    let ddc_caps = Arc::clone(&ddc_engine);
    let ui_weak_caps = ui.as_weak();
    ui.on_rescan_capabilities(move |_monitor_id| {
        let ddc_c = Arc::clone(&ddc_caps);
        let uw = ui_weak_caps.clone();
        if let Some(u) = uw.upgrade() {
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if row.ddc_supported && !row.is_internal_wmi {
                        row.vcps_probing = true;
                        monitors_rc.set_row_data(i, row);
                    }
                }
            }
        }
        thread::spawn(move || {
            let probed = ddc_c.probe_capabilities_for_all();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    push_monitors_to_ui(&u, &probed, false);
                }
            });
        });
    });

    // Callback: Frequenza di Aggiornamento Display Windows (Hz)
    let ddc_hz = Arc::clone(&ddc_engine);
    let ui_weak_hz = ui.as_weak();
    ui.on_set_refresh_rate(move |monitor_id, hz| {
        let ddc_c = Arc::clone(&ddc_hz);
        let uw = ui_weak_hz.clone();
        thread::spawn(move || {
            if let Ok(updated) = ddc_c.set_monitor_refresh_rate(monitor_id as usize, hz as u32) {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw.upgrade() {
                        push_monitors_to_ui(&u, &updated, false);
                    }
                });
            }
        });
    });

    // Callback: Toggle HDR Nativo Windows
    let ddc_hdr = Arc::clone(&ddc_engine);
    let ui_weak_hdr = ui.as_weak();
    ui.on_set_hdr_enabled(move |monitor_id, enabled| {
        let ddc_c = Arc::clone(&ddc_hdr);
        let uw = ui_weak_hdr.clone();
        thread::spawn(move || {
            if let Ok(updated) = ddc_c.set_monitor_hdr(monitor_id as usize, enabled) {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw.upgrade() {
                        push_monitors_to_ui(&u, &updated, false);
                    }
                });
            }
        });
    });

    // Callback: Calibrazione Software GDI LUT (Gamma, Warmth/Luce Blu, Contrasto, RGB)
    let ddc_lut = Arc::clone(&ddc_engine);
    let ui_weak_lut = ui.as_weak();
    ui.on_set_software_lut(move |monitor_id, enabled, gamma, warmth, contrast, r, g, b| {
        let cfg = SoftwareLutSettings {
            enabled,
            gamma_pct: gamma.clamp(50, 200),
            warmth: warmth.clamp(0, 100),
            contrast_pct: contrast.clamp(50, 150),
            red_pct: r.clamp(50, 150),
            green_pct: g.clamp(50, 150),
            blue_pct: b.clamp(50, 150),
        };
        ddc_lut.update_software_lut(monitor_id as usize, cfg.clone());
        if let Some(u) = ui_weak_lut.upgrade() {
            let monitors_rc = u.get_monitors();
            for i in 0..monitors_rc.row_count() {
                if let Some(mut row) = monitors_rc.row_data(i) {
                    if row.id == monitor_id {
                        row.sw_lut_enabled = cfg.enabled;
                        row.sw_gamma = cfg.gamma_pct;
                        row.sw_warmth = cfg.warmth;
                        row.sw_contrast = cfg.contrast_pct;
                        row.sw_red = cfg.red_pct;
                        row.sw_green = cfg.green_pct;
                        row.sw_blue = cfg.blue_pct;
                        monitors_rc.set_row_data(i, row);
                        break;
                    }
                }
            }
        }
    });

    // Callback: Ripristina Calibrazione Software LUT
    let ddc_lut_rst = Arc::clone(&ddc_engine);
    let ui_weak_lut_rst = ui.as_weak();
    ui.on_reset_software_lut(move |monitor_id| {
        let updated = ddc_lut_rst.reset_monitor_software_lut(monitor_id as usize);
        if let Some(u) = ui_weak_lut_rst.upgrade() {
            push_monitors_to_ui(&u, &updated, false);
        }
    });

    // Callback: Riordina Monitor da from_idx a to_idx con aggiornamento in-place dopo lo snap animato
    let ddc_move = Arc::clone(&ddc_engine);
    let settings_move = Arc::clone(&settings_state);
    let ui_weak_move = ui.as_weak();
    ui.on_move_monitor(move |from_idx, to_idx| {
        if from_idx < 0 || to_idx < 0 || from_idx == to_idx {
            return;
        }
        let from_u = from_idx as usize;
        let to_u = to_idx as usize;
        let ddc_c = Arc::clone(&ddc_move);
        let settings_c = Arc::clone(&settings_move);
        let uw = ui_weak_move.clone();

        let max_len = ddc_c.monitor_count();
        if from_u >= max_len || to_u >= max_len {
            return;
        }

        let updated = ddc_c.reorder_monitors(from_u, to_u);
        let order: Vec<String> = updated
            .iter()
            .map(|m| format!("{} {}", m.brand, m.name))
            .collect();
        thread::spawn(move || {
            let mut s = settings_c.lock().unwrap();
            s.monitor_order = order;
            let _ = s.save();
        });

        if let Some(u) = uw.upgrade() {
            let monitors_rc = u.get_monitors();
            let count = monitors_rc.row_count();
            if from_u < count && to_u < count {
                let prev_sel_id = monitors_rc
                    .row_data(u.get_selected_monitor_index().max(0) as usize)
                    .map(|r| r.id);

                let lo = from_u.min(to_u);
                let hi = from_u.max(to_u);
                let mut rows: Vec<UiMonitor> = (lo..=hi)
                    .filter_map(|i| monitors_rc.row_data(i))
                    .collect();
                if rows.len() == hi - lo + 1 {
                    let picked = rows.remove(from_u - lo);
                    rows.insert(to_u - lo, picked);
                    for (offset, item) in rows.into_iter().enumerate() {
                        monitors_rc.set_row_data(lo + offset, item);
                    }
                }

                if let Some(sel_id) = prev_sel_id {
                    for i in 0..monitors_rc.row_count() {
                        if monitors_rc.row_data(i).is_some_and(|r| r.id == sel_id) {
                            u.set_selected_monitor_index(i as i32);
                            break;
                        }
                    }
                }
                u.invoke_sync_current_values();
            }
        }
    });

    // Callback: Salvataggio Impostazioni (Zero RAM Overhead)
    let settings_save = Arc::clone(&settings_state);
    let ui_weak_save = ui.as_weak();
    ui.on_save_settings(move || {
        if let Some(u) = ui_weak_save.upgrade() {
            let mut s = settings_save.lock().unwrap();
            s.language = u.get_lang().to_string();
            s.accent_palette = u.get_accent_palette().to_string();
            s.glow_style = u.get_glow_style().to_string();
            s.layout_density = u.get_layout_density().to_string();
            s.layout_mode = u.get_layout_mode().to_string();
            s.sync_monitors = u.get_sync_monitors();
            s.hide_unsupported = u.get_hide_unsupported();
            s.close_to_tray = u.get_close_to_tray();
            s.start_with_windows = u.get_start_with_windows();
            s.auto_check_updates = u.get_auto_check_updates();
            s.acrylic_transparency = u.get_acrylic_enabled();
            s.animations_enabled = u.get_animations_enabled();
            let _ = s.save();
        }
    });

    // Callback: Toggle Trasparenza Acrylic in tempo reale
    let settings_acrylic = Arc::clone(&settings_state);
    let ui_weak_acrylic = ui.as_weak();
    ui.on_toggle_acrylic(move |enabled| {
        if let Some(u) = ui_weak_acrylic.upgrade() {
            set_window_acrylic(get_slint_hwnd(&u), enabled);
        }
        let mut s = settings_acrylic.lock().unwrap();
        s.acrylic_transparency = enabled;
        let _ = s.save();
    });

    // Callback: Verifica Aggiornamenti GitHub
    let ui_weak_upd = ui.as_weak();
    let dl_store_upd = Arc::clone(&latest_download_url);
    let chk_busy = Arc::clone(&update_checking);
    let dl_busy_chk = Arc::clone(&update_downloading);
    ui.on_check_updates(move || {
        if chk_busy.load(Ordering::SeqCst) || dl_busy_chk.load(Ordering::SeqCst) {
            return;
        }
        chk_busy.store(true, Ordering::SeqCst);
        if let Some(u) = ui_weak_upd.upgrade() {
            let is_en = u.get_lang() == "en";
            u.set_update_checking(true);
            u.set_update_failed(false);
            u.set_update_status_text(SharedString::from(if is_en {
                "Checking GitHub releases..."
            } else {
                "Verifica release su GitHub in corso..."
            }));
        }
        let uw = ui_weak_upd.clone();
        let dl_s = Arc::clone(&dl_store_upd);
        let chk_flag = Arc::clone(&chk_busy);
        thread::spawn(move || {
            let res = updater::check_for_updates();
            *dl_s.lock().unwrap() = Some((
                res.download_url.clone(),
                res.html_url.clone(),
                res.download_size,
            ));
            chk_flag.store(false, Ordering::SeqCst);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    u.set_update_checking(false);
                    u.set_update_available(res.update_available);
                    u.set_latest_version(SharedString::from(res.latest_version.as_str()));
                    u.set_update_status_text(SharedString::from(res.status_message.as_str()));
                }
            });
        });
    });

    // Callback: Annulla Download Aggiornamento
    let cancel_flag_cb = Arc::clone(&update_cancel_flag);
    let ui_weak_cancel = ui.as_weak();
    ui.on_cancel_update(move || {
        cancel_flag_cb.store(true, Ordering::SeqCst);
        if let Some(u) = ui_weak_cancel.upgrade() {
            let is_en = u.get_lang() == "en";
            u.set_update_progress_phase(SharedString::from(if is_en {
                "Canceling download..."
            } else {
                "Annullamento download..."
            }));
        }
    });

    // Callback: Installa Aggiornamento
    let dl_store_inst = Arc::clone(&latest_download_url);
    let ui_weak_inst = ui.as_weak();
    let dl_busy_inst = Arc::clone(&update_downloading);
    let chk_busy_inst = Arc::clone(&update_checking);
    let cancel_flag_inst = Arc::clone(&update_cancel_flag);
    let restart_store_inst = Arc::clone(&restart_after_exit);
    ui.on_install_update(move || {
        if dl_busy_inst.load(Ordering::SeqCst) || chk_busy_inst.load(Ordering::SeqCst) {
            return;
        }
        let info = dl_store_inst.lock().unwrap().clone();
        if let Some((download_url, html_url, expected_size)) = info {
            if let Some(url) = download_url {
                dl_busy_inst.store(true, Ordering::SeqCst);
                cancel_flag_inst.store(false, Ordering::SeqCst);

                let target_ver = if let Some(u) = ui_weak_inst.upgrade() {
                    let en = u.get_lang() == "en";
                    let ver = u.get_latest_version().to_string();
                    u.set_update_downloading(true);
                    u.set_update_installing(false);
                    u.set_update_ready_to_restart(false);
                    u.set_update_failed(false);
                    u.set_update_progress(0.0);
                    u.set_update_indeterminate(expected_size == 0);
                    u.set_update_progress_phase(SharedString::from(if en {
                        format!("Phase 2/3 · Downloading v{}...", ver)
                    } else {
                        format!("Fase 2/3 · Download v{}...", ver)
                    }));
                    u.set_update_progress_detail(SharedString::from(
                        updater::format_progress_detail(0, expected_size, 0.0),
                    ));
                    u.set_update_status_text(SharedString::from(if en {
                        format!("Downloading Monitor Manager v{}...", ver)
                    } else {
                        format!("Download Monitor Manager v{} in corso...", ver)
                    }));
                    ver
                } else {
                    String::new()
                };

                let uw = ui_weak_inst.clone();
                let dl_busy_done = Arc::clone(&dl_busy_inst);
                let cancel_flag_bg = Arc::clone(&cancel_flag_inst);
                let restart_bg = Arc::clone(&restart_store_inst);

                thread::spawn(move || {
                    let uw_prog = uw.clone();
                    let ver_prog = target_ver.clone();
                    let ver_cb = ver_prog.clone();
                    let result = updater::perform_self_update_with_progress(
                        &url,
                        &ver_prog,
                        expected_size,
                        &cancel_flag_bg,
                        move |prog| {
                            let uw_ui = uw_prog.clone();
                            let ver_str = ver_cb.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(u) = uw_ui.upgrade() {
                                    let en = u.get_lang() == "en";
                                    match prog.phase {
                                        updater::UpdatePhase::Downloading => {
                                            u.set_update_downloading(true);
                                            u.set_update_installing(false);
                                            u.set_update_indeterminate(prog.total_bytes == 0);
                                            u.set_update_progress(prog.fraction);
                                            u.set_update_progress_phase(SharedString::from(
                                                if en {
                                                    format!("Phase 2/3 · Downloading v{}...", ver_str)
                                                } else {
                                                    format!("Fase 2/3 · Download v{}...", ver_str)
                                                },
                                            ));
                                            u.set_update_progress_detail(SharedString::from(
                                                updater::format_progress_detail(
                                                    prog.downloaded_bytes,
                                                    prog.total_bytes,
                                                    prog.speed_bps,
                                                ),
                                            ));
                                        }
                                        updater::UpdatePhase::Installing => {
                                            u.set_update_downloading(false);
                                            u.set_update_installing(true);
                                            u.set_update_indeterminate(false);
                                            u.set_update_progress(1.0);
                                            u.set_update_progress_phase(SharedString::from(
                                                if en {
                                                    "Phase 3/3 · Preparing update...".to_string()
                                                } else {
                                                    "Fase 3/3 · Preparazione aggiornamento...".to_string()
                                                },
                                            ));
                                            u.set_update_progress_detail(SharedString::from(
                                                updater::format_progress_detail(
                                                    prog.downloaded_bytes,
                                                    prog.total_bytes,
                                                    0.0,
                                                ),
                                            ));
                                            u.set_update_status_text(SharedString::from(if en {
                                                format!("Staging v{}...", ver_str)
                                            } else {
                                                format!("Preparazione v{} in corso...", ver_str)
                                            }));
                                        }
                                    }
                                }
                            });
                        },
                    );

                    match result {
                        Ok(target_exe) => {
                            *restart_bg.lock().unwrap() = Some(target_exe);
                            dl_busy_done.store(false, Ordering::SeqCst);
                            let uw_done = uw.clone();
                            let ver_done = target_ver.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(u) = uw_done.upgrade() {
                                    let en = u.get_lang() == "en";
                                    u.set_update_downloading(false);
                                    u.set_update_installing(false);
                                    u.set_update_ready_to_restart(true);
                                    u.set_update_indeterminate(false);
                                    u.set_update_progress(1.0);
                                    u.set_update_progress_phase(SharedString::from(if en {
                                        "Phase 3/3 · Ready to restart"
                                    } else {
                                        "Fase 3/3 · Pronto al riavvio"
                                    }));
                                    u.set_update_status_text(SharedString::from(if en {
                                        format!("v{} downloaded and staged. Click Restart Now to apply.", ver_done)
                                    } else {
                                        format!("v{} scaricata e pronta. Premi Riavvia Ora per applicare.", ver_done)
                                    }));
                                    u.set_show_update_modal(true);
                                }
                            });
                        }
                        Err(err) => {
                            dl_busy_done.store(false, Ordering::SeqCst);
                            let cancelled = err.contains("annullato") || err.contains("canceled");
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(u) = uw.upgrade() {
                                    let en = u.get_lang() == "en";
                                    u.set_update_downloading(false);
                                    u.set_update_installing(false);
                                    u.set_update_indeterminate(false);
                                    u.set_update_progress(0.0);
                                    u.set_update_progress_phase(SharedString::from(""));
                                    u.set_update_progress_detail(SharedString::from(""));
                                    if cancelled {
                                        u.set_update_failed(false);
                                        u.set_update_status_text(SharedString::from(if en {
                                            format!(
                                                "Download canceled. Update v{} ready to retry.",
                                                u.get_latest_version()
                                            )
                                        } else {
                                            format!(
                                                "Download annullato. Aggiornamento v{} pronto.",
                                                u.get_latest_version()
                                            )
                                        }));
                                    } else {
                                        u.set_update_failed(true);
                                        u.set_update_status_text(SharedString::from(err.as_str()));
                                    }
                                }
                            });
                        }
                    }
                });
            } else {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &html_url])
                    .spawn();
            }
        }
    });

    // Callback: Riavvia App per applicare aggiornamento già scaricato
    let sd_restart_btn = Arc::clone(&shutting_down);
    ui.on_restart_for_update(move || {
        sd_restart_btn.store(true, Ordering::SeqCst);
        drop_tray_icon();
        let _ = slint::quit_event_loop();
    });

    // =========================================================================
    // ASUS DISPLAYWIDGET CENTER PARITY CALLBACKS, HOTKEYS & AUTOMATION WORKER
    // =========================================================================
    {
        let ddc_gv = Arc::clone(&ddc_engine);
        let dwc_gv = Arc::clone(&dwc_state);
        let uw_gv = ui.as_weak();
        ui.on_select_gamevisual_mode(move |monitor_id, mode_idx| {
            let sync_all = uw_gv.upgrade().map(|u| u.get_sync_monitors()).unwrap_or(false);
            apply_gamevisual_mode(
                &ddc_gv,
                monitor_id as usize,
                mode_idx,
                sync_all,
                &dwc_gv,
                &uw_gv,
            );
        });
    }

    {
        let ddc_rst = Arc::clone(&ddc_engine);
        let uw_rst = ui.as_weak();
        ui.on_reset_gamevisual_mode(move |monitor_id| {
            let sync_all = uw_rst.upgrade().map(|u| u.get_sync_monitors()).unwrap_or(false);
            let mid = monitor_id as usize;
            ddc_rst.send_discrete_vcp(mid, avcp::COLOR_RESET, 1);
            ddc_rst.set_vcp_value(mid, avcp::BRIGHTNESS, 80, sync_all);
            ddc_rst.set_vcp_value(mid, avcp::CONTRAST, 75, sync_all);
            ddc_rst.set_vcp_value(mid, avcp::VIVID_PIXEL, 50, sync_all);
            ddc_rst.set_vcp_value(mid, avcp::SATURATION, 50, sync_all);
        });
    }

    {
        let ddc_cmp = Arc::clone(&ddc_engine);
        let cmp_bk = Arc::clone(&compare_backup);
        let uw_cmp = ui.as_weak();
        ui.on_toggle_compare(move |monitor_id| {
            let mid = monitor_id as usize;
            let sync_all = uw_cmp.upgrade().map(|u| u.get_sync_monitors()).unwrap_or(false);
            let mut bk_guard = cmp_bk.lock().unwrap();
            if let Some(prev) = bk_guard.take() {
                ddc_cmp.set_vcp_value(mid, avcp::BRIGHTNESS, prev.brightness, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::CONTRAST, prev.contrast, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::SATURATION, prev.saturation, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::RED_GAIN, prev.red_gain, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::GREEN_GAIN, prev.green_gain, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::BLUE_GAIN, prev.blue_gain, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::VIVID_PIXEL, prev.vivid_pixel, sync_all);
                if let Some(u) = uw_cmp.upgrade() {
                    u.set_compare_active(false);
                }
            } else if let Some(cur) = ddc_cmp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                *bk_guard = Some(DisplayProfileSnapshot {
                    profile_name: "Compare Backup".into(),
                    model: cur.name.clone(),
                    gamevisual_mode: cur.asus.gamevisual_mode,
                    brightness: cur.brightness.current,
                    contrast: cur.contrast.current,
                    variable_od: cur.asus.variable_od,
                    blue_light_filter: cur.asus.blue_light_filter,
                    vivid_pixel: cur.asus.vivid_pixel,
                    elmb: cur.asus.elmb,
                    shadow_boost: cur.asus.shadow_boost,
                    dynamic_shadow_boost: cur.asus.dynamic_shadow_boost,
                    vrr_enabled: cur.asus.vrr_enabled,
                    aspect_control: cur.asus.aspect_control,
                    color_space: cur.asus.color_space,
                    saturation: cur.saturation.current,
                    color_temp: cur.color_temp.current,
                    red_gain: cur.red_gain.current,
                    green_gain: cur.green_gain.current,
                    blue_gain: cur.blue_gain.current,
                    volume: cur.volume.current,
                });
                // Reference baseline for instant visual A/B comparison
                ddc_cmp.set_vcp_value(mid, avcp::BRIGHTNESS, 50, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::CONTRAST, 50, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::SATURATION, 50, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::RED_GAIN, 50, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::GREEN_GAIN, 50, sync_all);
                ddc_cmp.set_vcp_value(mid, avcp::BLUE_GAIN, 50, sync_all);
                if let Some(u) = uw_cmp.upgrade() {
                    u.set_compare_active(true);
                }
            }
        });
    }

    {
        let ddc_imp = Arc::clone(&ddc_engine);
        let dwc_imp = Arc::clone(&dwc_state);
        let uw_imp = ui.as_weak();
        ui.on_import_display_profile(move |monitor_id| {
            let ddc_c = Arc::clone(&ddc_imp);
            let dwc_c = Arc::clone(&dwc_imp);
            let uw_c = uw_imp.clone();
            thread::spawn(move || {
                if let Some(path) = win_system::pick_open_file(
                    "Importa Profilo Display (JSON)",
                    &[("Monitor Profile JSON (*.json)", "*.json")],
                ) {
                    if let Ok(raw) = std::fs::read_to_string(&path) {
                        if let Ok(snap) = serde_json::from_str::<DisplayProfileSnapshot>(&raw) {
                            let mid = monitor_id as usize;
                            apply_gamevisual_mode(&ddc_c, mid, snap.gamevisual_mode, false, &dwc_c, &uw_c);
                            ddc_c.set_vcp_value(mid, avcp::BRIGHTNESS, snap.brightness, false);
                            ddc_c.set_vcp_value(mid, avcp::CONTRAST, snap.contrast, false);
                            ddc_c.set_vcp_value(mid, avcp::VIVID_PIXEL, snap.vivid_pixel, false);
                            ddc_c.set_vcp_value(mid, avcp::SATURATION, snap.saturation, false);
                            ddc_c.set_vcp_value(mid, avcp::RED_GAIN, snap.red_gain, false);
                            ddc_c.set_vcp_value(mid, avcp::GREEN_GAIN, snap.green_gain, false);
                            ddc_c.set_vcp_value(mid, avcp::BLUE_GAIN, snap.blue_gain, false);
                            ddc_c.send_discrete_vcp(mid, avcp::VARIABLE_OD, snap.variable_od);
                            ddc_c.send_discrete_vcp(mid, avcp::BLUE_LIGHT_FILTER, snap.blue_light_filter);
                            ddc_c.send_discrete_vcp(mid, avcp::ELMB, snap.elmb);
                            ddc_c.send_discrete_vcp(mid, avcp::SHADOW_BOOST, snap.shadow_boost);
                            ddc_c.send_discrete_vcp(mid, avcp::ASPECT_CONTROL, snap.aspect_control);
                            ddc_c.send_discrete_vcp(mid, avcp::COLOR_SPACE, snap.color_space);
                        }
                    }
                }
            });
        });
    }

    {
        let ddc_exp = Arc::clone(&ddc_engine);
        ui.on_export_display_profile(move |monitor_id| {
            let mid = monitor_id as usize;
            if let Some(cur) = ddc_exp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                let snap = DisplayProfileSnapshot {
                    profile_name: format!("{} GameVisual", cur.name),
                    model: cur.name.clone(),
                    gamevisual_mode: cur.asus.gamevisual_mode,
                    brightness: cur.brightness.current,
                    contrast: cur.contrast.current,
                    variable_od: cur.asus.variable_od,
                    blue_light_filter: cur.asus.blue_light_filter,
                    vivid_pixel: cur.asus.vivid_pixel,
                    elmb: cur.asus.elmb,
                    shadow_boost: cur.asus.shadow_boost,
                    dynamic_shadow_boost: cur.asus.dynamic_shadow_boost,
                    vrr_enabled: cur.asus.vrr_enabled,
                    aspect_control: cur.asus.aspect_control,
                    color_space: cur.asus.color_space,
                    saturation: cur.saturation.current,
                    color_temp: cur.color_temp.current,
                    red_gain: cur.red_gain.current,
                    green_gain: cur.green_gain.current,
                    blue_gain: cur.blue_gain.current,
                    volume: cur.volume.current,
                };
                let default_name = format!("{}_profile.json", cur.name.replace([' ', '/', '\\', ':'], "_"));
                thread::spawn(move || {
                    if let Some(path) = win_system::pick_save_file(
                        "Esporta Profilo Display (JSON)",
                        &default_name,
                        &[("Monitor Profile JSON (*.json)", "*.json")],
                    ) {
                        if let Ok(json) = serde_json::to_string_pretty(&snap) {
                            let _ = std::fs::write(path, json);
                        }
                    }
                });
            }
        });
    }

    {
        let ddc_set = Arc::clone(&ddc_engine);
        let dwc_set = Arc::clone(&dwc_state);
        let uw_set = ui.as_weak();
        ui.on_set_gamevisual_setting(move |monitor_id, key, value| {
            let power_saving = dwc_set.lock().unwrap().power_saving_mode;
            if power_saving && key.as_str() != "brightness" {
                return;
            }
            let sync_all = uw_set.upgrade().map(|u| u.get_sync_monitors()).unwrap_or(false);
            apply_gamevisual_setting_key(
                &ddc_set,
                monitor_id as usize,
                key.as_str(),
                value,
                sync_all,
                &uw_set,
            );
        });
    }

    // GamePlus Callbacks (Crosshair, Timer, FPS, Alignment, D-Pad, Delivery)
    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_set_crosshair(move |monitor_id, style| {
            let mid = monitor_id as usize;
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                mon.asus.crosshair_style = style.max(0) as u32;
                if let Some(u) = uw_gp.upgrade() {
                    let monitors_rc = u.get_monitors();
                    for i in 0..monitors_rc.row_count() {
                        if let Some(mut row) = monitors_rc.row_data(i) {
                            if row.id == monitor_id {
                                row.crosshair_style = style;
                                mon.asus.timer_preset = row.timer_preset.max(0) as u32;
                                mon.asus.fps_counter_mode = if row.fps_counter { 1 } else { 0 };
                                mon.asus.alignment_enabled = row.alignment_grid;
                                monitors_rc.set_row_data(i, row);
                            }
                        }
                    }
                }
                if dwc.gameplus_delivery_mode != "overlay" && mon.asus.is_asus && mon.ddc_supported {
                    let vcp_val = mon.asus.crosshair_ui_to_vcp(style.max(0) as u32);
                    ddc_gp.send_discrete_vcp(mid, avcp::GAMEPLUS_CROSSHAIR, vcp_val);
                }
                sync_gameplus_overlay(&ov_gp, &dwc, tp_gp.load(Ordering::Relaxed), &mon);
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_set_timer(move |monitor_id, preset| {
            let mid = monitor_id as usize;
            tp_gp.store(false, Ordering::Relaxed);
            ov_gp.reset_timer();
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                mon.asus.timer_preset = preset.max(0) as u32;
                if let Some(u) = uw_gp.upgrade() {
                    let monitors_rc = u.get_monitors();
                    for i in 0..monitors_rc.row_count() {
                        if let Some(mut row) = monitors_rc.row_data(i) {
                            if row.id == monitor_id {
                                row.timer_preset = preset;
                                mon.asus.crosshair_style = row.crosshair_style.max(0) as u32;
                                mon.asus.fps_counter_mode = if row.fps_counter { 1 } else { 0 };
                                mon.asus.alignment_enabled = row.alignment_grid;
                                monitors_rc.set_row_data(i, row);
                            }
                        }
                    }
                }
                if dwc.gameplus_delivery_mode != "overlay" && mon.asus.is_asus && mon.ddc_supported && preset <= 5 {
                    ddc_gp.send_discrete_vcp(mid, avcp::GAMEPLUS_TIMER, preset.max(0) as u32);
                }
                sync_gameplus_overlay(&ov_gp, &dwc, false, &mon);
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_toggle_timer_pause(move || {
            let new_paused = !tp_gp.load(Ordering::Relaxed);
            tp_gp.store(new_paused, Ordering::Relaxed);
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(u) = uw_gp.upgrade() {
                let Some(cm) = active_ui_monitor(&u) else { return; };
                if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id as i32 == cm.id) {
                    mon.asus.crosshair_style = cm.crosshair_style.max(0) as u32;
                    mon.asus.timer_preset = cm.timer_preset.max(0) as u32;
                    mon.asus.fps_counter_mode = if cm.fps_counter { 1 } else { 0 };
                    mon.asus.alignment_enabled = cm.alignment_grid;
                    sync_gameplus_overlay(&ov_gp, &dwc, new_paused, &mon);
                }
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_reset_timer(move || {
            tp_gp.store(false, Ordering::Relaxed);
            ov_gp.reset_timer();
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(u) = uw_gp.upgrade() {
                let Some(cm) = active_ui_monitor(&u) else { return; };
                if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id as i32 == cm.id) {
                    mon.asus.crosshair_style = cm.crosshair_style.max(0) as u32;
                    mon.asus.timer_preset = cm.timer_preset.max(0) as u32;
                    mon.asus.fps_counter_mode = if cm.fps_counter { 1 } else { 0 };
                    mon.asus.alignment_enabled = cm.alignment_grid;
                    sync_gameplus_overlay(&ov_gp, &dwc, false, &mon);
                }
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_set_fps(move |monitor_id, enabled| {
            let mid = monitor_id as usize;
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                mon.asus.fps_counter_mode = if enabled { 1 } else { 0 };
                if let Some(u) = uw_gp.upgrade() {
                    let monitors_rc = u.get_monitors();
                    for i in 0..monitors_rc.row_count() {
                        if let Some(mut row) = monitors_rc.row_data(i) {
                            if row.id == monitor_id {
                                row.fps_counter = enabled;
                                mon.asus.crosshair_style = row.crosshair_style.max(0) as u32;
                                mon.asus.timer_preset = row.timer_preset.max(0) as u32;
                                mon.asus.alignment_enabled = row.alignment_grid;
                                monitors_rc.set_row_data(i, row);
                            }
                        }
                    }
                }
                if dwc.gameplus_delivery_mode != "overlay" && mon.asus.is_asus && mon.ddc_supported {
                    ddc_gp.send_discrete_vcp(mid, avcp::GAMEPLUS_FPS_COUNTER, if enabled { 1 } else { 0 });
                }
                sync_gameplus_overlay(&ov_gp, &dwc, tp_gp.load(Ordering::Relaxed), &mon);
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_set_alignment(move |monitor_id, enabled| {
            let mid = monitor_id as usize;
            let dwc = dwc_gp.lock().unwrap().clone();
            if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id == mid) {
                mon.asus.alignment_enabled = enabled;
                if let Some(u) = uw_gp.upgrade() {
                    let monitors_rc = u.get_monitors();
                    for i in 0..monitors_rc.row_count() {
                        if let Some(mut row) = monitors_rc.row_data(i) {
                            if row.id == monitor_id {
                                row.alignment_grid = enabled;
                                mon.asus.crosshair_style = row.crosshair_style.max(0) as u32;
                                mon.asus.timer_preset = row.timer_preset.max(0) as u32;
                                mon.asus.fps_counter_mode = if row.fps_counter { 1 } else { 0 };
                                monitors_rc.set_row_data(i, row);
                            }
                        }
                    }
                }
                if dwc.gameplus_delivery_mode != "overlay" && mon.asus.is_asus && mon.ddc_supported {
                    ddc_gp.send_discrete_vcp(mid, avcp::GAMEPLUS_ALIGNMENT, if enabled { 1 } else { 0 });
                }
                sync_gameplus_overlay(&ov_gp, &dwc, tp_gp.load(Ordering::Relaxed), &mon);
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_nudge_position(move |monitor_id, dir| {
            let mid = monitor_id as usize;
            let (ox, oy, dwc) = {
                let mut g = dwc_gp.lock().unwrap();
                match dir {
                    0 => {
                        g.gameplus_offset_x = 0;
                        g.gameplus_offset_y = 0;
                    }
                    1 => g.gameplus_offset_y = (g.gameplus_offset_y - 16).clamp(-480, 480),
                    2 => g.gameplus_offset_y = (g.gameplus_offset_y + 16).clamp(-480, 480),
                    3 => g.gameplus_offset_x = (g.gameplus_offset_x - 16).clamp(-640, 640),
                    4 => g.gameplus_offset_x = (g.gameplus_offset_x + 16).clamp(-640, 640),
                    _ => {}
                }
                g.save();
                (g.gameplus_offset_x, g.gameplus_offset_y, g.clone())
            };
            if dir >= 1 && dir <= 4 && dwc.gameplus_delivery_mode != "overlay" {
                ddc_gp.send_discrete_vcp(mid, avcp::GAMEPLUS_DPAD_NUDGE, dir as u32);
            }
            if let Some(u) = uw_gp.upgrade() {
                u.set_gameplus_offset_x(ox);
                u.set_gameplus_offset_y(oy);
                let Some(cm) = active_ui_monitor(&u) else { return; };
                if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id as i32 == cm.id) {
                    mon.asus.crosshair_style = cm.crosshair_style.max(0) as u32;
                    mon.asus.timer_preset = cm.timer_preset.max(0) as u32;
                    mon.asus.fps_counter_mode = if cm.fps_counter { 1 } else { 0 };
                    mon.asus.alignment_enabled = cm.alignment_grid;
                    sync_gameplus_overlay(&ov_gp, &dwc, tp_gp.load(Ordering::Relaxed), &mon);
                }
            }
        });
    }

    {
        let ddc_gp = Arc::clone(&ddc_engine);
        let dwc_gp = Arc::clone(&dwc_state);
        let ov_gp = Arc::clone(&overlay_engine);
        let tp_gp = Arc::clone(&timer_paused);
        let uw_gp = ui.as_weak();
        ui.on_gp_set_delivery(move |mode| {
            let dwc = {
                let mut g = dwc_gp.lock().unwrap();
                g.gameplus_delivery_mode = mode.to_string();
                g.save();
                g.clone()
            };
            if let Some(u) = uw_gp.upgrade() {
                u.set_gameplus_delivery(mode);
                let Some(cm) = active_ui_monitor(&u) else { return; };
                if let Some(mut mon) = ddc_gp.get_cached_monitors().into_iter().find(|m| m.id as i32 == cm.id) {
                    mon.asus.crosshair_style = cm.crosshair_style.max(0) as u32;
                    mon.asus.timer_preset = cm.timer_preset.max(0) as u32;
                    mon.asus.fps_counter_mode = if cm.fps_counter { 1 } else { 0 };
                    mon.asus.alignment_enabled = cm.alignment_grid;
                    sync_gameplus_overlay(&ov_gp, &dwc, tp_gp.load(Ordering::Relaxed), &mon);
                }
            }
        });
    }

    // MultiScreen Callbacks
    {
        let uw_ms = ui.as_weak();
        ui.on_refresh_open_windows(move || {
            push_open_windows_to_ui(uw_ms.clone());
        });
    }

    {
        let ddc_ms = Arc::clone(&ddc_engine);
        ui.on_snap_window_zone(move |monitor_idx, hwnd_str, layout_id, zone_idx| {
            if let Ok(hwnd) = hwnd_str.parse::<isize>() {
                let mons = ddc_ms.get_cached_monitors();
                let mon_id = mons
                    .get(monitor_idx.max(0) as usize)
                    .map(|m| m.id)
                    .unwrap_or(0);
                let (work, _) = win_system::get_monitor_rects(mon_id)
                    .unwrap_or(([0, 0, 1920, 1040], [0, 0, 1920, 1080]));
                let zones = win_system::compute_multiscreen_zones(work, layout_id.as_str());
                if !zones.is_empty() {
                    let target = zones[(zone_idx.max(0) as usize) % zones.len()];
                    thread::spawn(move || {
                        win_system::snap_window_to_rect(hwnd, target);
                    });
                }
            }
        });
    }

    // Global Hotkey Service & Callbacks
    let hotkey_service = {
        let ddc_hk = Arc::clone(&ddc_engine);
        let dwc_hk = Arc::clone(&dwc_state);
        let ov_hk = Arc::clone(&overlay_engine);
        let tp_hk = Arc::clone(&timer_paused);
        let uw_hk = ui.as_weak();
        let initial_dwc = dwc_hk.lock().unwrap().clone();
        let svc = Arc::new(GlobalHotkeyService::spawn(move |action| {
            let mons = ddc_hk.get_cached_monitors();
            let active_idx = uw_hk
                .upgrade()
                .map(|u| u.get_selected_monitor_index().max(0) as usize)
                .unwrap_or(0);
            let mon = mons.get(active_idx).or_else(|| mons.first()).cloned();
            let Some(mut mon) = mon else { return; };
            let mid = mon.id;
            match action.as_str() {
                "brightness_up" => {
                    let next = (mon.brightness.current + 10).min(mon.brightness.max.max(100));
                    apply_gamevisual_setting_key(&ddc_hk, mid, "brightness", next as i32, false, &uw_hk);
                }
                "brightness_down" => {
                    let next = mon.brightness.current.saturating_sub(10);
                    apply_gamevisual_setting_key(&ddc_hk, mid, "brightness", next as i32, false, &uw_hk);
                }
                "cycle_gamevisual" => {
                    let next = ((mon.asus.gamevisual_mode + 1) % 8).max(1);
                    apply_gamevisual_mode(&ddc_hk, mid, next, false, &dwc_hk, &uw_hk);
                }
                "toggle_crosshair" => {
                    let next = if mon.asus.crosshair_style == 0 { 1 } else { 0 };
                    mon.asus.crosshair_style = next;
                    let dwc = dwc_hk.lock().unwrap().clone();
                    if dwc.gameplus_delivery_mode != "overlay" && mon.asus.is_asus && mon.ddc_supported {
                        let vcp_val = mon.asus.crosshair_ui_to_vcp(next);
                        ddc_hk.send_discrete_vcp(mid, avcp::GAMEPLUS_CROSSHAIR, vcp_val);
                    }
                    sync_gameplus_overlay(&ov_hk, &dwc, tp_hk.load(Ordering::Relaxed), &mon);
                    let uw_c = uw_hk.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(u) = uw_c.upgrade() {
                            let rc = u.get_monitors();
                            for i in 0..rc.row_count() {
                                if let Some(mut r) = rc.row_data(i) {
                                    if r.id as usize == mid {
                                        r.crosshair_style = next as i32;
                                        rc.set_row_data(i, r);
                                    }
                                }
                            }
                        }
                    });
                }
                "toggle_hdr" => {
                    let next = !mon.os_info.hdr_enabled;
                    if let Ok(list) = ddc_hk.set_monitor_hdr(mid, next) {
                        let uw_c = uw_hk.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(u) = uw_c.upgrade() {
                                push_monitors_to_ui(&u, &list, false);
                            }
                        });
                    }
                }
                "cycle_input" => {
                    let opts = [15u32, 17, 18];
                    let pos = opts.iter().position(|&v| v == mon.input_source.current).unwrap_or(0);
                    let next = opts[(pos + 1) % opts.len()];
                    ddc_hk.send_discrete_vcp(mid, avcp::INPUT_SOURCE, next);
                }
                "night_care" => {
                    let next = if mon.asus.blue_light_filter == 0 { 3 } else { 0 };
                    apply_gamevisual_setting_key(&ddc_hk, mid, "blue_light", next, false, &uw_hk);
                }
                _ => {}
            }
        }));
        svc.update_bindings(initial_dwc.hotkeys_enabled, initial_dwc.hotkeys);
        svc
    };

    {
        let dwc_hkt = Arc::clone(&dwc_state);
        let svc_c = Arc::clone(&hotkey_service);
        let uw_hkt = ui.as_weak();
        ui.on_toggle_hotkeys_global(move |enabled| {
            let mut cfg = dwc_hkt.lock().unwrap();
            cfg.hotkeys_enabled = enabled;
            cfg.save();
            svc_c.update_bindings(cfg.hotkeys_enabled, cfg.hotkeys.clone());
            if let Some(u) = uw_hkt.upgrade() {
                sync_hotkeys_to_ui(&u, &cfg);
            }
        });
    }

    {
        let dwc_hki = Arc::clone(&dwc_state);
        let svc_c = Arc::clone(&hotkey_service);
        let uw_hki = ui.as_weak();
        ui.on_toggle_hotkey_item(move |id, enabled| {
            let mut cfg = dwc_hki.lock().unwrap();
            if let Some(item) = cfg.hotkeys.iter_mut().find(|h| h.id == id) {
                item.enabled = enabled;
            }
            cfg.save();
            svc_c.update_bindings(cfg.hotkeys_enabled, cfg.hotkeys.clone());
            if let Some(u) = uw_hki.upgrade() {
                sync_hotkeys_to_ui(&u, &cfg);
            }
        });
    }

    {
        let dwc_hks = Arc::clone(&dwc_state);
        let svc_c = Arc::clone(&hotkey_service);
        let uw_hks = ui.as_weak();
        ui.on_update_hotkey_shortcut(move |id, text| {
            if let Some((mods, vk)) = automation::parse_shortcut_text(text.as_str()) {
                let mut cfg = dwc_hks.lock().unwrap();
                if let Some(item) = cfg.hotkeys.iter_mut().find(|h| h.id == id) {
                    item.modifiers = mods;
                    item.vk = vk;
                }
                cfg.save();
                svc_c.update_bindings(cfg.hotkeys_enabled, cfg.hotkeys.clone());
                if let Some(u) = uw_hks.upgrade() {
                    sync_hotkeys_to_ui(&u, &cfg);
                }
            }
        });
    }

    // App Tweaker Callbacks
    {
        let dwc_at = Arc::clone(&dwc_state);
        let uw_at = ui.as_weak();
        ui.on_toggle_app_tweaker(move |enabled| {
            let mut cfg = dwc_at.lock().unwrap();
            cfg.app_tweaker_enabled = enabled;
            cfg.save();
            if let Some(u) = uw_at.upgrade() {
                sync_rules_to_ui(&u, &cfg);
            }
        });
    }

    {
        let dwc_at = Arc::clone(&dwc_state);
        let uw_at = ui.as_weak();
        ui.on_add_app_rule(move |exe, name, mode, bright, bluelight, od, shadow| {
            let exe_s = exe.trim().to_lowercase();
            if exe_s.is_empty() {
                return;
            }
            let mut cfg = dwc_at.lock().unwrap();
            cfg.app_rules.retain(|r| !r.exe_name.eq_ignore_ascii_case(&exe_s));
            cfg.app_rules.push(AppTweakerRule {
                exe_name: exe_s.clone(),
                display_name: if name.trim().is_empty() { exe_s } else { name.to_string() },
                gamevisual_mode: mode,
                brightness: bright,
                blue_light: bluelight,
                variable_od: od,
                shadow_boost: shadow,
                enabled: true,
            });
            cfg.save();
            if let Some(u) = uw_at.upgrade() {
                sync_rules_to_ui(&u, &cfg);
            }
        });
    }

    {
        let dwc_at = Arc::clone(&dwc_state);
        let uw_at = ui.as_weak();
        ui.on_remove_app_rule(move |idx| {
            let mut cfg = dwc_at.lock().unwrap();
            let i = idx.max(0) as usize;
            if i < cfg.app_rules.len() {
                cfg.app_rules.remove(i);
                cfg.save();
                if let Some(u) = uw_at.upgrade() {
                    sync_rules_to_ui(&u, &cfg);
                }
            }
        });
    }

    {
        let dwc_at = Arc::clone(&dwc_state);
        let uw_at = ui.as_weak();
        ui.on_toggle_app_rule(move |idx, enabled| {
            let mut cfg = dwc_at.lock().unwrap();
            let i = idx.max(0) as usize;
            if let Some(r) = cfg.app_rules.get_mut(i) {
                r.enabled = enabled;
                cfg.save();
                if let Some(u) = uw_at.upgrade() {
                    sync_rules_to_ui(&u, &cfg);
                }
            }
        });
    }

    // AI Visual & Assistente IA Callbacks
    {
        let dwc_ai = Arc::clone(&dwc_state);
        let uw_ai = ui.as_weak();
        ui.on_toggle_ai_visual(move |enabled| {
            let mut cfg = dwc_ai.lock().unwrap();
            cfg.ai_visual_enabled = enabled;
            cfg.save();
            if let Some(u) = uw_ai.upgrade() {
                u.set_ai_visual_active(enabled);
            }
        });
    }

    {
        let dwc_ai = Arc::clone(&dwc_state);
        let uw_ai = ui.as_weak();
        ui.on_toggle_ai_night_schedule(move |enabled| {
            let mut cfg = dwc_ai.lock().unwrap();
            cfg.ai_night_schedule = enabled;
            cfg.save();
            if let Some(u) = uw_ai.upgrade() {
                u.set_ai_night_schedule(enabled);
            }
        });
    }

    {
        let dwc_ai = Arc::clone(&dwc_state);
        let uw_ai = ui.as_weak();
        ui.on_set_ai_night_hours(move |start, end| {
            let mut cfg = dwc_ai.lock().unwrap();
            cfg.ai_night_start_hour = start.clamp(0, 23) as u32;
            cfg.ai_night_end_hour = end.clamp(0, 23) as u32;
            cfg.save();
            if let Some(u) = uw_ai.upgrade() {
                u.set_ai_night_start(cfg.ai_night_start_hour as i32);
                u.set_ai_night_end(cfg.ai_night_end_hour as i32);
            }
        });
    }

    {
        let ddc_ai = Arc::clone(&ddc_engine);
        let dwc_ai = Arc::clone(&dwc_state);
        let ov_ai = Arc::clone(&overlay_engine);
        let tp_ai = Arc::clone(&timer_paused);
        let uw_ai = ui.as_weak();
        ui.on_run_ai_command(move |cmd_text| {
            let actions = automation::parse_ai_command(cmd_text.as_str());
            let Some(u) = uw_ai.upgrade() else { return; };
            let Some(cm) = active_ui_monitor(&u) else { return; };
            let mid = cm.id as usize;
            let sync_all = u.get_sync_monitors();
            if actions.is_empty() {
                u.set_last_ai_feedback(SharedString::from(
                    "Comando non riconosciuto. Prova es: 'FPS', 'luminosità 80', 'HDR on', 'barra automatica'.",
                ));
                return;
            }
            let mut feedback = Vec::new();
            for act in actions {
                match act {
                    AiAction::SetBrightness(v) => {
                        apply_gamevisual_setting_key(&ddc_ai, mid, "brightness", v as i32, sync_all, &uw_ai);
                        feedback.push(format!("Luminosità -> {}%", v));
                    }
                    AiAction::SetContrast(v) => {
                        apply_gamevisual_setting_key(&ddc_ai, mid, "contrast", v as i32, sync_all, &uw_ai);
                        feedback.push(format!("Contrasto -> {}%", v));
                    }
                    AiAction::SetBlueLight(v) => {
                        apply_gamevisual_setting_key(&ddc_ai, mid, "blue_light", v as i32, sync_all, &uw_ai);
                        feedback.push(format!("Filtro luce blu -> Lv {}", v));
                    }
                    AiAction::SetGameVisual(m) => {
                        apply_gamevisual_mode(&ddc_ai, mid, m, sync_all, &dwc_ai, &uw_ai);
                        feedback.push(format!("GameVisual -> {}", GameVisualMode::from_index(m).label_it()));
                    }
                    AiAction::SetCrosshair(s) => {
                        u.invoke_gp_set_crosshair(mid as i32, s as i32);
                        let _ = (&ov_ai, &tp_ai);
                        feedback.push(if s == 0 { "Mirino -> Off".into() } else { format!("Mirino -> Stile {}", s) });
                    }
                    AiAction::SetHdr(en) => {
                        u.invoke_set_hdr_enabled(mid as i32, en);
                        feedback.push(if en { "HDR -> Attivo".into() } else { "HDR -> Disattivato".into() });
                    }
                    AiAction::SetTaskbarAutohide(en) => {
                        u.invoke_set_taskbar_autohide(en);
                        feedback.push(if en { "Taskbar auto-hide -> ON".into() } else { "Taskbar auto-hide -> OFF".into() });
                    }
                    AiAction::SetDarkTheme(dark) => {
                        u.invoke_set_windows_theme(!dark, !dark);
                        feedback.push(if dark { "Tema Windows -> Scuro".into() } else { "Tema Windows -> Chiaro".into() });
                    }
                    AiAction::SetPowerSaving(ps) => {
                        u.invoke_set_power_saving_mode(ps);
                        feedback.push(if ps { "Alimentazione -> Risparmio".into() } else { "Alimentazione -> Standard".into() });
                    }
                    AiAction::SetInputSource(v) => {
                        u.invoke_set_vcp(mid as i32, 96, v as i32);
                        feedback.push(format!("Input VCP -> {}", v));
                    }
                    AiAction::SetVolume(v) => {
                        u.invoke_set_vcp(mid as i32, 98, v as i32);
                        feedback.push(format!("Volume -> {}", v));
                    }
                    AiAction::ResetMonitor => {
                        u.invoke_reset_gamevisual_mode(mid as i32);
                        feedback.push("Reset GameVisual avviato".into());
                    }
                }
            }
            u.set_last_ai_feedback(SharedString::from(feedback.join(" · ")));
        });
    }

    // Impostazioni Applicazione Windows (Taskbar Auto-Hide, Windows/App Theme, Power Mode, Update Policy)
    {
        let uw_tb = ui.as_weak();
        ui.on_set_taskbar_autohide(move |enabled| {
            if let Some(u) = uw_tb.upgrade() {
                u.set_taskbar_autohide(enabled);
            }
            thread::spawn(move || {
                let _ = win_system::set_taskbar_autohide(enabled);
            });
        });
    }

    {
        let uw_th = ui.as_weak();
        ui.on_set_windows_theme(move |sys_light, apps_light| {
            if let Some(u) = uw_th.upgrade() {
                u.set_win_system_light(sys_light);
                u.set_win_apps_light(apps_light);
            }
            thread::spawn(move || {
                let _ = win_system::set_windows_theme(sys_light, apps_light);
            });
        });
    }

    {
        let dwc_ps = Arc::clone(&dwc_state);
        let uw_ps = ui.as_weak();
        ui.on_set_power_saving_mode(move |enabled| {
            let mut cfg = dwc_ps.lock().unwrap();
            cfg.power_saving_mode = enabled;
            cfg.save();
            if let Some(u) = uw_ps.upgrade() {
                u.set_power_saving_mode(enabled);
            }
        });
    }

    {
        let dwc_up = Arc::clone(&dwc_state);
        let set_up = Arc::clone(&settings_state);
        let uw_up = ui.as_weak();
        ui.on_set_update_policy(move |policy| {
            let pol = if policy.as_str() == "auto" { "auto" } else { "manual" };
            {
                let mut cfg = dwc_up.lock().unwrap();
                cfg.update_policy = pol.to_string();
                cfg.save();
            }
            {
                let mut s = set_up.lock().unwrap();
                s.auto_check_updates = pol == "auto";
                let _ = s.save();
            }
            if let Some(u) = uw_up.upgrade() {
                u.set_update_policy(SharedString::from(pol));
                u.set_auto_check_updates(pol == "auto");
            }
        });
    }

    // Worker di Automazione in Background (1.5s poll, saltato a costo zero in Risparmio Energetico)
    {
        let ddc_auto = Arc::clone(&ddc_engine);
        let dwc_auto = Arc::clone(&dwc_state);
        let sd_auto = Arc::clone(&shutting_down);
        let uw_auto = ui.as_weak();
        thread::spawn(move || {
            let mut last_rule_key = String::new();
            let mut last_ai_mode: i32 = -1;
            let mut last_night_active = false;
            loop {
                thread::sleep(std::time::Duration::from_millis(1500));
                if sd_auto.load(Ordering::Relaxed) {
                    break;
                }
                let cfg = dwc_auto.lock().unwrap().clone();
                if cfg.power_saving_mode {
                    continue;
                }
                let mons = ddc_auto.get_cached_monitors();
                let Some(mon) = mons.first() else { continue; };
                let mid = mon.id;

                let fg = win_system::get_foreground_app();
                let mut matched_rule = false;

                if cfg.app_tweaker_enabled {
                    if let Some((ref exe, ref title)) = fg {
                        if let Some(rule) = cfg.app_rules.iter().find(|r| r.enabled && r.exe_name.eq_ignore_ascii_case(exe)) {
                            matched_rule = true;
                            if last_rule_key != *exe {
                                last_rule_key = exe.clone();
                                if rule.gamevisual_mode >= 0 {
                                    apply_gamevisual_mode(&ddc_auto, mid, rule.gamevisual_mode, false, &dwc_auto, &uw_auto);
                                }
                                if rule.brightness >= 0 {
                                    apply_gamevisual_setting_key(&ddc_auto, mid, "brightness", rule.brightness, false, &uw_auto);
                                }
                                if rule.blue_light >= 0 {
                                    apply_gamevisual_setting_key(&ddc_auto, mid, "blue_light", rule.blue_light, false, &uw_auto);
                                }
                                if rule.variable_od >= 0 {
                                    apply_gamevisual_setting_key(&ddc_auto, mid, "variable_od", rule.variable_od, false, &uw_auto);
                                }
                                if rule.shadow_boost >= 0 {
                                    apply_gamevisual_setting_key(&ddc_auto, mid, "shadow_boost", rule.shadow_boost, false, &uw_auto);
                                }
                            }
                        } else if !last_rule_key.is_empty() {
                            last_rule_key.clear();
                            let _ = title;
                        }
                    }
                }

                if !matched_rule && (cfg.ai_visual_enabled || mon.asus.gamevisual_mode == 0) {
                    let (exe_s, title_s) = fg.as_ref().map(|(e, t)| (e.as_str(), t.as_str())).unwrap_or(("", ""));
                    let (luma, dark_pct, sat) = automation::sample_screen_metrics();
                    let metrics = automation::classify_ai_scene(exe_s, title_s, luma, dark_pct, sat);
                    let uw_c = uw_auto.clone();
                    let scene = metrics.detected_scene.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(u) = uw_c.upgrade() {
                            u.set_ai_detected_scene(SharedString::from(scene.as_str()));
                            u.set_ai_avg_luma(metrics.avg_luma as i32);
                            u.set_ai_dark_pct(metrics.dark_pixel_pct as i32);
                        }
                    });
                    if metrics.recommended_mode != last_ai_mode {
                        last_ai_mode = metrics.recommended_mode;
                        apply_gamevisual_mode(&ddc_auto, mid, metrics.recommended_mode, false, &dwc_auto, &uw_auto);
                    }
                }

                if cfg.ai_night_schedule {
                    let hour = automation::current_local_hour();
                    let night_now = automation::is_night_care_hour(
                        hour,
                        cfg.ai_night_start_hour as u8,
                        cfg.ai_night_end_hour as u8,
                    );
                    if night_now != last_night_active {
                        last_night_active = night_now;
                        let target_bl = if night_now { 3 } else { 0 };
                        apply_gamevisual_setting_key(&ddc_auto, mid, "blue_light", target_bl, false, &uw_auto);
                    }
                }
            }
        });
    }

    // Callback: Quit App
    let sd_quit_btn = Arc::clone(&shutting_down);
    let ov_quit = Arc::clone(&overlay_engine);
    ui.on_quit_app(move || {
        sd_quit_btn.store(true, Ordering::SeqCst);
        ov_quit.update_config(OverlayConfig {
            enabled: false,
            target_rect: [0, 0, 1920, 1080],
            crosshair_style: 0,
            timer_mode: 0,
            timer_paused: false,
            fps_counter: false,
            alignment_grid: false,
            offset_x: 0,
            offset_y: 0,
        });
        drop_tray_icon();
        let _ = slint::quit_event_loop();
    });

    // Scansione iniziale monitor (2 fasi) e controllo aggiornamenti in background
    {
        let ddc_init = Arc::clone(&ddc_engine);
        let uw_init = ui.as_weak();
        let auto_upd = settings_state.lock().unwrap().auto_check_updates
            || dwc_state.lock().unwrap().update_policy == "auto";
        let order = settings_state.lock().unwrap().monitor_order.clone();
        let dl_init = Arc::clone(&latest_download_url);

        thread::spawn(move || {
            run_two_phase_scan(&ddc_init, &order, &uw_init);

            if auto_upd {
                let res = updater::check_for_updates();
                *dl_init.lock().unwrap() = Some((
                    res.download_url.clone(),
                    res.html_url.clone(),
                    res.download_size,
                ));
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw_init.upgrade() {
                        u.set_update_available(res.update_available);
                        u.set_latest_version(SharedString::from(res.latest_version.as_str()));
                        u.set_update_status_text(SharedString::from(res.status_message.as_str()));
                    }
                });
            }
        });
    }

    if !start_minimized {
        let acrylic = settings_state.lock().unwrap().acrylic_transparency;
        show_and_focus_window(&ui, acrylic);
        let hwnd_int = get_slint_hwnd(&ui).map(|h| h as isize);
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(80));
            set_window_acrylic(
                hwnd_int.map(|h| h as windows_sys::Win32::Foundation::HWND),
                acrylic,
            );
        });
    } else {
        trim_process_working_set();
    }

    let _ = slint::run_event_loop_until_quit();

    shutting_down.store(true, Ordering::SeqCst);
    drop_tray_icon();
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::SetEvent;
        if !wake_event.is_null() {
            let _ = SetEvent(wake_event);
            let _ = CloseHandle(wake_event);
        }
        if !instance_mutex.is_null() {
            let _ = CloseHandle(instance_mutex);
        }
    }

    let target_exe = restart_after_exit.lock().unwrap().take();
    if let Some(target_exe) = target_exe {
        let _ = std::process::Command::new(target_exe).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::hit_test_resize_border;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
    };

    #[test]
    fn border_hit_test_covers_edges_corners_and_negative_monitors() {
        // Finestra su monitor secondario a sinistra (coordinate X negative)
        let (l, t, r, b) = (-1000, 100, -20, 700);
        let (edge, corner) = (6, 12);

        assert_eq!(hit_test_resize_border(-1000, 100, l, t, r, b, edge, corner), Some(HTTOPLEFT));
        assert_eq!(hit_test_resize_border(-990, 103, l, t, r, b, edge, corner), Some(HTTOPLEFT));
        assert_eq!(hit_test_resize_border(-21, 102, l, t, r, b, edge, corner), Some(HTTOPRIGHT));
        assert_eq!(hit_test_resize_border(-998, 697, l, t, r, b, edge, corner), Some(HTBOTTOMLEFT));
        assert_eq!(hit_test_resize_border(-24, 695, l, t, r, b, edge, corner), Some(HTBOTTOMRIGHT));

        assert_eq!(hit_test_resize_border(-999, 400, l, t, r, b, edge, corner), Some(HTLEFT));
        assert_eq!(hit_test_resize_border(-22, 400, l, t, r, b, edge, corner), Some(HTRIGHT));
        assert_eq!(hit_test_resize_border(-500, 102, l, t, r, b, edge, corner), Some(HTTOP));
        assert_eq!(hit_test_resize_border(-500, 696, l, t, r, b, edge, corner), Some(HTBOTTOM));

        // Area interna e bottoni in alto a destra sotto il bordo (es. X chiudi a 15px dal top)
        assert_eq!(hit_test_resize_border(-35, 115, l, t, r, b, edge, corner), None);
        assert_eq!(hit_test_resize_border(-500, 400, l, t, r, b, edge, corner), None);
        // Fuori finestra
        assert_eq!(hit_test_resize_border(-1005, 400, l, t, r, b, edge, corner), None);
    }
}
