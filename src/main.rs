#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ddc;
mod settings;
mod updater;

use ddc::{DdcEngine, MonitorInfo};
use serde::Deserialize;
use serde_json::json;
use settings::AppSettings;
use std::env;
use std::sync::{Arc, Mutex};
use std::thread;
use tao::dpi::{LogicalSize, Size};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::windows::WindowExtWindows;
use tao::window::{Icon as WindowIcon, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon as TrayIconData, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use updater::{UpdateCheckResult, APP_VERSION};
use wry::WebViewBuilder;

#[derive(Debug)]
enum UserEvent {
    InitReady {
        monitors: Vec<MonitorInfo>,
        settings: AppSettings,
    },
    MonitorsScanned(Vec<MonitorInfo>),
    UpdateChecked(UpdateCheckResult),
    UpdateError(String),
    ShowMainWindow,
    RescanRequested,
    QuitApp,
    DragWindow,
    WinAction(String),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum IpcCommand {
    #[serde(rename = "init")]
    Init,
    #[serde(rename = "drag_window")]
    DragWindow,
    #[serde(rename = "window_action")]
    WindowAction { action: String },
    #[serde(rename = "set_vcp")]
    SetVcp {
        monitor_id: usize,
        vcp_code: u8,
        value: u32,
        sync_all: bool,
    },
    #[serde(rename = "rescan_monitors")]
    RescanMonitors,
    #[serde(rename = "save_settings")]
    SaveSettings { settings: AppSettings },
    #[serde(rename = "check_updates")]
    CheckUpdates,
    #[serde(rename = "install_update")]
    InstallUpdate {
        download_url: Option<String>,
        html_url: String,
    },
}

/// Genera in memoria un'icona 32x32 RGBA in stile Liquid Glass (nessun file esterno necessario)
fn generate_app_icon_rgba() -> (Vec<u8>, u32, u32) {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            // Cornice arrotondata esterna
            let dx = (x as f32 - 15.5).abs();
            let dy = (y as f32 - 15.5).abs();
            if dx <= 14.5 && dy <= 14.5 && (dx < 11.5 || dy < 11.5 || ((dx - 11.5).powi(2) + (dy - 11.5).powi(2)).sqrt() <= 3.2) {
                // Gradiente ciano -> indaco -> viola
                let t = (x + y) as f32 / 64.0;
                let r = (56.0 * (1.0 - t) + 139.0 * t) as u8;
                let g = (189.0 * (1.0 - t) + 92.0 * t) as u8;
                let b = (248.0 * (1.0 - t) + 246.0 * t) as u8;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;

                // Disegna la sagoma bianca del monitor al centro
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

/// Applica gli angoli arrotondati nativi di Windows 11 (DWMWCP_ROUND) alla finestra frameless
fn enable_win11_rounded_corners(hwnd: isize) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE};
    const DWMWCP_ROUND: u32 = 2;
    unsafe {
        let pref: u32 = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &pref as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

fn build_bundled_html() -> String {
    let raw_html = include_str!("ui/index.html");
    let raw_css = include_str!("ui/styles.css");
    let raw_js = include_str!("ui/app.js");

    raw_html
        .replace("/*__INLINE_CSS__*/", raw_css)
        .replace("/*__INLINE_JS__*/", raw_js)
}

const WEBVIEW2_LOADER_BYTES: &[u8] = include_bytes!("../target/release/WebView2Loader.dll");

fn ensure_webview2_loader() {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let loader_path = dir.join("WebView2Loader.dll");
            if !loader_path.exists() {
                let _ = std::fs::write(loader_path, WEBVIEW2_LOADER_BYTES);
            }
        }
    }
}

fn perform_uninstall() {
    use std::fs;
    use std::path::PathBuf;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        KEY_ALL_ACCESS,
    };

    // Rimuove collegamenti Desktop e Start Menu
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

    // Rimuove chiavi di registro Run e Uninstall
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

    // Pianifica la rimozione della cartella di installazione appena il processo termina
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

fn main() {
    if env::args().any(|a| a == "--uninstall") {
        perform_uninstall();
        return;
    }

    ensure_webview2_loader();
    updater::cleanup_old_executable();

    let start_minimized = env::args().any(|a| a == "--minimized");
    let settings_state = Arc::new(Mutex::new(AppSettings::load()));
    let ddc_engine = Arc::new(DdcEngine::new());

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();

    let (icon_rgba, icon_w, icon_h) = generate_app_icon_rgba();
    let window_icon = WindowIcon::from_rgba(icon_rgba.clone(), icon_w, icon_h).ok();

    let window = WindowBuilder::new()
        .with_title("Monitor Manager")
        .with_inner_size(Size::Logical(LogicalSize::new(1120.0, 780.0)))
        .with_min_inner_size(Size::Logical(LogicalSize::new(720.0, 560.0)))
        .with_decorations(false)
        .with_transparent(false)
        .with_visible(!start_minimized)
        .with_window_icon(window_icon)
        .build(&event_loop)
        .expect("Impossibile creare la finestra principale");

    enable_win11_rounded_corners(window.hwnd());

    // Configurazione System Tray
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

    let tray_icon_data = TrayIconData::from_rgba(icon_rgba, icon_w, icon_h)
        .expect("Impossibile generare l'icona di sistema");

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Monitor Manager v1.01")
        .with_icon(tray_icon_data)
        .build()
        .ok();

    // Listener eventi menu System Tray
    let proxy_menu = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            let _ = proxy_menu.send_event(UserEvent::ShowMainWindow);
        } else if event.id == rescan_id {
            let _ = proxy_menu.send_event(UserEvent::RescanRequested);
        } else if event.id == quit_id {
            let _ = proxy_menu.send_event(UserEvent::QuitApp);
        }
    }));

    // Listener click singolo sull'icona della System Tray per riaprire subito la finestra
    let proxy_tray = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = proxy_tray.send_event(UserEvent::ShowMainWindow);
        }
    }));

    // Preparazione WebView2 con IPC Handler
    let html_content = build_bundled_html();
    let proxy_ipc = proxy.clone();
    let ddc_ipc = Arc::clone(&ddc_engine);
    let settings_ipc = Arc::clone(&settings_state);

    let webview = WebViewBuilder::new()
        .with_html(html_content)
        .with_ipc_handler(move |req| {
            let body = req.body();
            if let Ok(cmd) = serde_json::from_str::<IpcCommand>(body) {
                match cmd {
                    IpcCommand::Init => {
                        let ddc_clone = Arc::clone(&ddc_ipc);
                        let settings_clone = Arc::clone(&settings_ipc);
                        let p = proxy_ipc.clone();

                        thread::spawn(move || {
                            let current_settings = settings_clone.lock().unwrap().clone();
                            let monitors = ddc_clone.scan_monitors();
                            let _ = p.send_event(UserEvent::InitReady {
                                monitors,
                                settings: current_settings.clone(),
                            });

                            if current_settings.auto_check_updates {
                                let res = updater::check_for_updates();
                                let _ = p.send_event(UserEvent::UpdateChecked(res));
                            }
                        });
                    }
                    IpcCommand::DragWindow => {
                        let _ = proxy_ipc.send_event(UserEvent::DragWindow);
                    }
                    IpcCommand::WindowAction { action } => {
                        let _ = proxy_ipc.send_event(UserEvent::WinAction(action));
                    }
                    IpcCommand::SetVcp {
                        monitor_id,
                        vcp_code,
                        value,
                        sync_all,
                    } => {
                        ddc_ipc.set_vcp_value(monitor_id, vcp_code, value, sync_all);
                    }
                    IpcCommand::RescanMonitors => {
                        let ddc_clone = Arc::clone(&ddc_ipc);
                        let p = proxy_ipc.clone();
                        thread::spawn(move || {
                            let monitors = ddc_clone.scan_monitors();
                            let _ = p.send_event(UserEvent::MonitorsScanned(monitors));
                        });
                    }
                    IpcCommand::SaveSettings { settings } => {
                        let mut guard = settings_ipc.lock().unwrap();
                        *guard = settings;
                        let _ = guard.save();
                    }
                    IpcCommand::CheckUpdates => {
                        let p = proxy_ipc.clone();
                        thread::spawn(move || {
                            let res = updater::check_for_updates();
                            let _ = p.send_event(UserEvent::UpdateChecked(res));
                        });
                    }
                    IpcCommand::InstallUpdate {
                        download_url,
                        html_url,
                    } => {
                        let p = proxy_ipc.clone();
                        thread::spawn(move || {
                            if let Some(url) = download_url {
                                if let Err(err) = updater::perform_self_update(&url) {
                                    let _ = p.send_event(UserEvent::UpdateError(err));
                                }
                            } else {
                                // Se la release su GitHub non ha ancora un asset .exe diretto, apri la pagina release
                                let _ = std::process::Command::new("cmd")
                                    .args(["/C", "start", "", &html_url])
                                    .spawn();
                            }
                        });
                    }
                }
            }
        })
        .build(&window)
        .expect("Impossibile inizializzare WebView2");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let close_to_tray = settings_state.lock().unwrap().close_to_tray;
                if close_to_tray {
                    window.set_visible(false);
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(user_ev) => match user_ev {
                UserEvent::InitReady { monitors, settings } => {
                    let payload = json!({
                        "type": "init_data",
                        "version": APP_VERSION,
                        "settings": settings,
                        "monitors": monitors,
                    });
                    let script = format!("window.onBackendEvent({});", payload);
                    let _ = webview.evaluate_script(&script);
                }
                UserEvent::MonitorsScanned(monitors) => {
                    let payload = json!({
                        "type": "monitors_updated",
                        "monitors": monitors,
                    });
                    let script = format!("window.onBackendEvent({});", payload);
                    let _ = webview.evaluate_script(&script);
                }
                UserEvent::UpdateChecked(result) => {
                    let payload = json!({
                        "type": "update_check_result",
                        "result": result,
                    });
                    let script = format!("window.onBackendEvent({});", payload);
                    let _ = webview.evaluate_script(&script);
                }
                UserEvent::UpdateError(msg) => {
                    let payload = json!({
                        "type": "update_error",
                        "message": msg,
                    });
                    let script = format!("window.onBackendEvent({});", payload);
                    let _ = webview.evaluate_script(&script);
                }
                UserEvent::ShowMainWindow => {
                    window.set_visible(true);
                    window.set_minimized(false);
                    window.set_focus();
                }
                UserEvent::RescanRequested => {
                    window.set_visible(true);
                    window.set_focus();
                    let ddc_clone = Arc::clone(&ddc_engine);
                    let p = proxy.clone();
                    thread::spawn(move || {
                        let monitors = ddc_clone.scan_monitors();
                        let _ = p.send_event(UserEvent::MonitorsScanned(monitors));
                    });
                }
                UserEvent::DragWindow => {
                    let _ = window.drag_window();
                }
                UserEvent::WinAction(action) => match action.as_str() {
                    "minimize" => window.set_minimized(true),
                    "maximize" => window.set_maximized(!window.is_maximized()),
                    "close" => {
                        let close_to_tray = settings_state.lock().unwrap().close_to_tray;
                        if close_to_tray {
                            window.set_visible(false);
                        } else {
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    "quit" => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                },
                UserEvent::QuitApp => {
                    *control_flow = ControlFlow::Exit;
                }
            },
            _ => {}
        }
    });
}
