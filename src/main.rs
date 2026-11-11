#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ddc;
mod settings;
mod updater;

slint::include_modules!();

use ddc::{DdcEngine, MonitorInfo};
use settings::AppSettings;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::env;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon as TrayIconData, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
use updater::APP_VERSION;

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

fn get_window_hwnd() -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    let my_pid = std::process::id();

    unsafe extern "system" fn find_hwnd_cb(hwnd: HWND, lparam: isize) -> i32 {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let my_pid = *(lparam as *const u32);
        if pid == my_pid && IsWindowVisible(hwnd) != 0 {
            let target_ptr = (lparam as *mut usize).add(1) as *mut HWND;
            *target_ptr = hwnd;
            return 0; // trovato, interrompe enumerazione
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

/// Applica la vera trasparenza Acrylic di Windows (DWM) combinata con angoli arrotondati e drop-shadow.
/// Supporta Windows 11 nativo (DWMWA_SYSTEMBACKDROP_TYPE) e Windows 10 (SetWindowCompositionAttribute).
fn set_window_acrylic(enabled: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE};

    if let Some(hwnd) = get_window_hwnd() {
        unsafe {
            // 1. Assicura sempre gli angoli arrotondati standard Windows 11 (DWMWCP_ROUND = 2)
            let corner_pref: u32 = 2;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &corner_pref as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );

            // 2. Windows 11 DWM Backdrop (DWMSBT_TRANSIENTWINDOW = 3 per Acrylic, DWMSBT_NONE = 1 per opaco)
            let backdrop_type: u32 = if enabled { 3 } else { 1 };
            let _ = DwmSetWindowAttribute(
                hwnd,
                38, // DWMWA_SYSTEMBACKDROP_TYPE
                &backdrop_type as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );

            // 3. Fallback per Windows 10 tramite SetWindowCompositionAttribute (user32.dll)
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

            if let Ok(lib) = std::ffi::CString::new("user32.dll") {
                #[link(name = "kernel32")]
                extern "system" {
                    fn GetModuleHandleA(lpModuleName: *const i8) -> isize;
                    fn GetProcAddress(hModule: isize, lpProcName: *const i8) -> *const std::ffi::c_void;
                }
                let h_user32 = GetModuleHandleA(lib.as_ptr());
                if h_user32 != 0 {
                    if let Ok(fn_name) = std::ffi::CString::new("SetWindowCompositionAttribute") {
                        let proc = GetProcAddress(h_user32, fn_name.as_ptr());
                        if !proc.is_null() {
                            let set_attr: SetWindowCompositionAttributeFn = std::mem::transmute(proc);
                            let mut policy = AccentPolicy {
                                accent_state: if enabled { 4 /* ACCENT_ENABLE_ACRYLICBLURBEHIND */ } else { 0 },
                                accent_flags: if enabled { 2 } else { 0 },
                                gradient_color: if enabled { 0x66181c22 } else { 0 }, // Fumé scuro 40% alfa
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
    }
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

fn to_ui_monitor(m: &MonitorInfo) -> UiMonitor {
    UiMonitor {
        id: m.id as i32,
        name: SharedString::from(m.name.as_str()),
        brand: SharedString::from(m.brand.as_str()),
        resolution: SharedString::from(m.resolution.as_str()),
        is_primary: m.is_primary,
        ddc_supported: m.ddc_supported,
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
    }
}

fn main() {
    if env::args().any(|a| a == "--uninstall") {
        perform_uninstall();
        return;
    }

    updater::cleanup_old_executable();

    let start_minimized = env::args().any(|a| a == "--minimized");
    let settings_state = Arc::new(Mutex::new(AppSettings::load()));
    let ddc_engine = Arc::new(DdcEngine::new());
    let latest_download_url: Arc<Mutex<Option<(Option<String>, String)>>> =
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

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(format!("Monitor Manager v{} (Dark Matte Titanium)", APP_VERSION))
        .with_icon(tray_icon_data)
        .build()
        .ok();

    // Gestione Menu System Tray
    let ui_weak_menu = ui.as_weak();
    let ddc_menu = Arc::clone(&ddc_engine);
    let settings_menu = Arc::clone(&settings_state);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            let uw = ui_weak_menu.clone();
            let s_menu = Arc::clone(&settings_menu);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let _ = u.show();
                    let acrylic = s_menu.lock().unwrap().acrylic_transparency;
                    set_window_acrylic(acrylic);
                }
            });
        } else if event.id == rescan_id {
            let uw = ui_weak_menu.clone();
            let ddc_c = Arc::clone(&ddc_menu);
            let s_c = Arc::clone(&settings_menu);
            thread::spawn(move || {
                let order = s_c.lock().unwrap().monitor_order.clone();
                let list = ddc_c.scan_monitors_ordered(&order);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw.upgrade() {
                        let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                        u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                        u.invoke_sync_current_values();
                        let _ = u.show();
                    }
                });
            });
        } else if event.id == quit_id {
            let _ = slint::quit_event_loop();
        }
    }));

    // Click sinistro sull'icona della System Tray riapre immediatamente la finestra
    let ui_weak_tray = ui.as_weak();
    let settings_tray = Arc::clone(&settings_state);
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let uw = ui_weak_tray.clone();
            let s_tr = Arc::clone(&settings_tray);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let _ = u.show();
                    let acrylic = s_tr.lock().unwrap().acrylic_transparency;
                    set_window_acrylic(acrylic);
                }
            });
        }
    }));

    // Chiusura finestra -> riduce nella System Tray e libera la RAM (Working Set Trim)
    let settings_close = Arc::clone(&settings_state);
    let ui_weak_close = ui.as_weak();
    ui.window().on_close_requested(move || {
        let close_to_tray = settings_close.lock().unwrap().close_to_tray;
        if close_to_tray {
            if let Some(u) = ui_weak_close.upgrade() {
                let _ = u.hide();
            }
            trim_process_working_set();
            slint::CloseRequestResponse::KeepWindowShown
        } else {
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
                set_window_acrylic(acrylic);
            }
        }
    });

    // Callback barra superiore borderless: Chiudi / Minimizza nella System Tray (✕)
    let settings_btn_close = Arc::clone(&settings_state);
    let ui_weak_btn_close = ui.as_weak();
    ui.on_close_window(move || {
        let close_to_tray = settings_btn_close.lock().unwrap().close_to_tray;
        if close_to_tray {
            if let Some(u) = ui_weak_btn_close.upgrade() {
                let _ = u.hide();
            }
            trim_process_working_set();
        } else {
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
                        row.brightness = lum;
                        row.contrast = con;
                        monitors_rc.set_row_data(i, row);

                        if u.get_selected_monitor_index() == i as i32 {
                            u.set_current_brightness(lum);
                            u.set_current_contrast(con);
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
                    if sync_all || row.id == monitor_id {
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

    // Callback: Rescan Monitor
    let ddc_rescan = Arc::clone(&ddc_engine);
    let settings_rescan = Arc::clone(&settings_state);
    let ui_weak_rescan = ui.as_weak();
    ui.on_rescan_monitors(move || {
        let ddc_c = Arc::clone(&ddc_rescan);
        let s_c = Arc::clone(&settings_rescan);
        let uw = ui_weak_rescan.clone();
        thread::spawn(move || {
            let order = s_c.lock().unwrap().monitor_order.clone();
            let list = ddc_c.scan_monitors_ordered(&order);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                    u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                    u.invoke_sync_current_values();
                }
            });
        });
    });

    // Callback: Riordina Monitor (Su / Giù)
    let ddc_move = Arc::clone(&ddc_engine);
    let settings_move = Arc::clone(&settings_state);
    let ui_weak_move = ui.as_weak();
    ui.on_move_monitor(move |idx, dir| {
        let idx = idx as usize;
        let ddc_c = Arc::clone(&ddc_move);
        let settings_c = Arc::clone(&settings_move);
        let uw = ui_weak_move.clone();

        let max_len = ddc_c.monitor_count();
        let new_idx = if dir < 0 {
            if idx > 0 { idx - 1 } else { return; }
        } else {
            if idx + 1 < max_len { idx + 1 } else { return; }
        };

        let updated = ddc_c.reorder_monitors(idx, new_idx);
        let order: Vec<String> = updated.iter().map(|m| format!("{} {}", m.brand, m.name)).collect();
        {
            let mut s = settings_c.lock().unwrap();
            s.monitor_order = order;
            let _ = s.save();
        }

        let vec_ui: Vec<UiMonitor> = updated.iter().map(to_ui_monitor).collect();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(u) = uw.upgrade() {
                u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                u.set_selected_monitor_index(new_idx as i32);
                u.invoke_sync_current_values();
            }
        });
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
            let _ = s.save();
        }
    });

    // Callback: Toggle Trasparenza Acrylic in tempo reale
    let settings_acrylic = Arc::clone(&settings_state);
    ui.on_toggle_acrylic(move |enabled| {
        set_window_acrylic(enabled);
        let mut s = settings_acrylic.lock().unwrap();
        s.acrylic_transparency = enabled;
        let _ = s.save();
    });

    // Callback: Verifica Aggiornamenti GitHub
    let ui_weak_upd = ui.as_weak();
    let dl_store_upd = Arc::clone(&latest_download_url);
    ui.on_check_updates(move || {
        if let Some(u) = ui_weak_upd.upgrade() {
            u.set_update_status_text(SharedString::from(
                "Verifica release su GitHub in corso...",
            ));
        }
        let uw = ui_weak_upd.clone();
        let dl_s = Arc::clone(&dl_store_upd);
        thread::spawn(move || {
            let res = updater::check_for_updates();
            *dl_s.lock().unwrap() = Some((res.download_url.clone(), res.html_url.clone()));
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    u.set_update_available(res.update_available);
                    u.set_latest_version(SharedString::from(res.latest_version.as_str()));
                    u.set_update_status_text(SharedString::from(res.status_message.as_str()));
                }
            });
        });
    });

    // Callback: Installa Aggiornamento
    let dl_store_inst = Arc::clone(&latest_download_url);
    let ui_weak_inst = ui.as_weak();
    ui.on_install_update(move || {
        let info = dl_store_inst.lock().unwrap().clone();
        if let Some((download_url, html_url)) = info {
            if let Some(u) = ui_weak_inst.upgrade() {
                u.set_update_status_text(SharedString::from(
                    "Download e installazione nuova versione in corso...",
                ));
            }
            let uw = ui_weak_inst.clone();
            thread::spawn(move || {
                if let Some(url) = download_url {
                    if let Err(err) = updater::perform_self_update(&url) {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(u) = uw.upgrade() {
                                u.set_update_status_text(SharedString::from(err.as_str()));
                            }
                        });
                    }
                } else {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", "", &html_url])
                        .spawn();
                }
            });
        }
    });

    // Callback: Quit App
    ui.on_quit_app(move || {
        let _ = slint::quit_event_loop();
    });

    // Scansione iniziale monitor e controllo aggiornamenti in background
    {
        let ddc_init = Arc::clone(&ddc_engine);
        let uw_init = ui.as_weak();
        let auto_upd = settings_state.lock().unwrap().auto_check_updates;
        let order = settings_state.lock().unwrap().monitor_order.clone();
        let dl_init = Arc::clone(&latest_download_url);

        thread::spawn(move || {
            let list = ddc_init.scan_monitors_ordered(&order);
            let uw_scan = uw_init.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw_scan.upgrade() {
                    let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                    u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                    u.invoke_sync_current_values();
                }
            });

            if auto_upd {
                let res = updater::check_for_updates();
                *dl_init.lock().unwrap() = Some((res.download_url.clone(), res.html_url.clone()));
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
        let _ = ui.show();
        let acrylic = settings_state.lock().unwrap().acrylic_transparency;
        set_window_acrylic(acrylic);
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(80));
            set_window_acrylic(acrylic);
        });
    } else {
        trim_process_working_set();
    }

    let _ = slint::run_event_loop_until_quit();
}
