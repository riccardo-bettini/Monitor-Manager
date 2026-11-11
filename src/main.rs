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
                // Gradiente Coral Sunset
                let r = (223.0 * (1.0 - t) + 184.0 * t) as u8;
                let g = (75.0 * (1.0 - t) + 50.0 * t) as u8;
                let b = (59.0 * (1.0 - t) + 39.0 * t) as u8;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;

                let in_screen_border = (x >= 6 && x <= 25 && (y == 7 || y == 19))
                    || (y >= 7 && y <= 19 && (x == 6 || x == 25));
                let in_stand_neck = (x == 15 || x == 16) && (y >= 20 && y <= 23);
                let in_stand_base = y == 24 && (x >= 11 && x <= 20);

                if in_screen_border || in_stand_neck || in_stand_base {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
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

    // Usa il renderer software ultra-leggero di Slint per azzerare l'impatto sulla VRAM/GPU durante il gaming
    if env::var("SLINT_BACKEND").is_err() {
        env::set_var("SLINT_BACKEND", "winit-software");
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
        ui.set_theme_pref(SharedString::from(s.theme.as_str()));
        ui.set_dark_mode(s.effective_dark_mode());
        ui.set_accent_palette(SharedString::from(s.accent_palette.as_str()));
        ui.set_layout_mode(SharedString::from(s.layout_mode.as_str()));
        ui.set_active_view(SharedString::from(if s.layout_mode == "multi" {
            "multi"
        } else {
            "dashboard"
        }));
        ui.set_sync_monitors(s.sync_monitors);
        ui.set_hide_unsupported(s.hide_unsupported);
        ui.set_close_to_tray(s.close_to_tray);
        ui.set_start_with_windows(s.start_with_windows);
        ui.set_auto_check_updates(s.auto_check_updates);
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
        .with_tooltip("Monitor Manager v1.02 (Ultra-Light Native)")
        .with_icon(tray_icon_data)
        .build()
        .ok();

    // Gestione Menu System Tray
    let ui_weak_menu = ui.as_weak();
    let ddc_menu = Arc::clone(&ddc_engine);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            let uw = ui_weak_menu.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let _ = u.show();
                }
            });
        } else if event.id == rescan_id {
            let uw = ui_weak_menu.clone();
            let ddc_c = Arc::clone(&ddc_menu);
            thread::spawn(move || {
                let list = ddc_c.scan_monitors();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = uw.upgrade() {
                        let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                        u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
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
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let uw = ui_weak_tray.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let _ = u.show();
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

    // Callback: Modifica codice VCP (Luminosità, Contrasto, Nitidezza, Saturazione, RGB, Volume, Input)
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

            // Aggiorna il modello Slint in memoria per riflettere il nuovo valore istantaneamente
            let monitors_model = u.get_monitors();
            let mut updated: Vec<UiMonitor> = (0..monitors_model.row_count())
                .filter_map(|i| monitors_model.row_data(i))
                .collect();

            for m in updated.iter_mut() {
                if sync_all || m.id == monitor_id {
                    match vcp_code as u8 {
                        ddc::VCP_BRIGHTNESS => m.brightness = value,
                        ddc::VCP_CONTRAST => m.contrast = value,
                        ddc::VCP_SHARPNESS => m.sharpness = value,
                        ddc::VCP_SATURATION => m.saturation = value,
                        ddc::VCP_RED_GAIN => m.red_gain = value,
                        ddc::VCP_GREEN_GAIN => m.green_gain = value,
                        ddc::VCP_BLUE_GAIN => m.blue_gain = value,
                        ddc::VCP_COLOR_TEMP => m.color_temp = value,
                        ddc::VCP_VOLUME => m.volume = value,
                        ddc::VCP_INPUT_SOURCE => m.input_source = value,
                        _ => {}
                    }
                }
            }
            u.set_monitors(ModelRc::from(Rc::new(VecModel::from(updated))));
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

            let monitors_model = u.get_monitors();
            let mut updated: Vec<UiMonitor> = (0..monitors_model.row_count())
                .filter_map(|i| monitors_model.row_data(i))
                .collect();
            for m in updated.iter_mut() {
                if sync_all || m.id == monitor_id {
                    m.brightness = lum;
                    m.contrast = con;
                }
            }
            u.set_monitors(ModelRc::from(Rc::new(VecModel::from(updated))));
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

            let monitors_model = u.get_monitors();
            let mut updated: Vec<UiMonitor> = (0..monitors_model.row_count())
                .filter_map(|i| monitors_model.row_data(i))
                .collect();
            for m in updated.iter_mut() {
                if sync_all || m.id == monitor_id {
                    m.red_gain = 50;
                    m.green_gain = 50;
                    m.blue_gain = 50;
                }
            }
            u.set_monitors(ModelRc::from(Rc::new(VecModel::from(updated))));
        }
    });

    // Callback: Rescan Monitor
    let ddc_rescan = Arc::clone(&ddc_engine);
    let ui_weak_rescan = ui.as_weak();
    ui.on_rescan_monitors(move || {
        let ddc_c = Arc::clone(&ddc_rescan);
        let uw = ui_weak_rescan.clone();
        thread::spawn(move || {
            let list = ddc_c.scan_monitors();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw.upgrade() {
                    let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                    u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                }
            });
        });
    });

    // Callback: Salvataggio Impostazioni
    let settings_save = Arc::clone(&settings_state);
    let ui_weak_save = ui.as_weak();
    ui.on_save_settings(move || {
        if let Some(u) = ui_weak_save.upgrade() {
            let mut s = settings_save.lock().unwrap();
            s.theme = u.get_theme_pref().to_string();
            s.accent_palette = u.get_accent_palette().to_string();
            s.layout_mode = u.get_layout_mode().to_string();
            s.sync_monitors = u.get_sync_monitors();
            s.hide_unsupported = u.get_hide_unsupported();
            s.close_to_tray = u.get_close_to_tray();
            s.start_with_windows = u.get_start_with_windows();
            s.auto_check_updates = u.get_auto_check_updates();
            u.set_dark_mode(s.effective_dark_mode());
            let _ = s.save();
        }
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
        let dl_init = Arc::clone(&latest_download_url);

        thread::spawn(move || {
            let list = ddc_init.scan_monitors();
            let uw_scan = uw_init.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(u) = uw_scan.upgrade() {
                    let vec_ui: Vec<UiMonitor> = list.iter().map(to_ui_monitor).collect();
                    u.set_monitors(ModelRc::from(Rc::new(VecModel::from(vec_ui))));
                }
                trim_process_working_set();
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
    } else {
        trim_process_working_set();
    }

    let _ = slint::run_event_loop_until_quit();
}
