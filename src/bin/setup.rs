#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use tao::dpi::{LogicalSize, Size};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::windows::WindowExtWindows;
use tao::window::{Icon as WindowIcon, WindowBuilder};
use wry::WebViewBuilder;

const APP_VERSION: &str = "1.01";
const APP_NAME: &str = "Monitor Manager";
const PAYLOAD_EXE: &[u8] = include_bytes!("../../target/release/monitor-manager.exe");
const WEBVIEW2_LOADER: &[u8] = include_bytes!("../../target/release/WebView2Loader.dll");

#[derive(Debug)]
enum SetupEvent {
    DragWindow,
    CloseWindow,
    InstallProgress { step: String, percent: u32 },
    #[allow(dead_code)]
    InstallSuccess { install_dir: String, launch: bool },
    InstallError(String),
}

#[derive(Debug, Clone, Deserialize)]
struct InstallOptions {
    desktop_shortcut: bool,
    start_menu_shortcut: bool,
    autostart: bool,
    launch_after: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum SetupIpc {
    #[serde(rename = "drag_window")]
    DragWindow,
    #[serde(rename = "close")]
    Close,
    #[serde(rename = "start_install")]
    StartInstall { options: InstallOptions },
    #[serde(rename = "finish")]
    Finish { launch: bool },
}

fn get_install_dir() -> PathBuf {
    let base = env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\Program Files"));
    base.join("Programs").join("Monitor Manager")
}

/// Genera l'icona RGBA 32x32 sia per la finestra sia per costruire il file .ico ufficiale
fn generate_icon_rgba() -> (Vec<u8>, u32, u32) {
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
                let r = (56.0 * (1.0 - t) + 139.0 * t) as u8;
                let g = (189.0 * (1.0 - t) + 92.0 * t) as u8;
                let b = (248.0 * (1.0 - t) + 246.0 * t) as u8;
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

/// Crea un file .ico Windows valido (32x32 32-bit BGRA) a partire dal buffer RGBA
fn build_ico_bytes() -> Vec<u8> {
    let (rgba, w, h) = generate_icon_rgba();
    let xor_size = (w * h * 4) as u32;
    let and_row_bytes = ((w + 31) / 32) * 4;
    let and_size = and_row_bytes * h;
    let bmp_size = 40 + xor_size + and_size;

    let mut ico = Vec::with_capacity((6 + 16 + bmp_size) as usize);
    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (icon)
    ico.extend_from_slice(&1u16.to_le_bytes()); // count = 1

    // ICONDIRENTRY
    ico.push(w as u8);
    ico.push(h as u8);
    ico.push(0); // color count
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bit count
    ico.extend_from_slice(&bmp_size.to_le_bytes()); // bytes in res
    ico.extend_from_slice(&22u32.to_le_bytes()); // image offset (6 + 16)

    // BITMAPINFOHEADER
    ico.extend_from_slice(&40u32.to_le_bytes()); // biSize
    ico.extend_from_slice(&(w as i32).to_le_bytes()); // biWidth
    ico.extend_from_slice(&((h * 2) as i32).to_le_bytes()); // biHeight (XOR + AND)
    ico.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    ico.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    ico.extend_from_slice(&0u32.to_le_bytes()); // biCompression
    ico.extend_from_slice(&(xor_size + and_size).to_le_bytes()); // biSizeImage
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());

    // Pixel data bottom-up in BGRA
    for y in (0..h).rev() {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];
            ico.push(b);
            ico.push(g);
            ico.push(r);
            ico.push(a);
        }
    }

    // AND mask
    ico.extend(std::iter::repeat(0u8).take(and_size as usize));
    ico
}

fn create_windows_shortcut(lnk_path: &Path, target_exe: &Path, icon_path: &Path, work_dir: &Path) {
    let ps_script = format!(
        "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.IconLocation = '{},0'; $s.WorkingDirectory = '{}'; $s.Description = 'Monitor Manager - Controllo Universale Monitor DDC/CI'; $s.Save()",
        lnk_path.display().to_string().replace('\'', "''"),
        target_exe.display().to_string().replace('\'', "''"),
        icon_path.display().to_string().replace('\'', "''"),
        work_dir.display().to_string().replace('\'', "''"),
    );

    let _ = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_script])
        .status();
}

fn register_uninstaller_and_autostart(
    install_dir: &Path,
    exe_path: &Path,
    icon_path: &Path,
    autostart: bool,
) {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_SZ,
    };

    unsafe {
        // 1. Registra in App Installate di Windows (Uninstall\MonitorManager)
        let subkey: Vec<u16> =
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\MonitorManager\0"
                .encode_utf16()
                .collect();
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegCreateKeyW(HKEY_CURRENT_USER, subkey.as_ptr(), &mut hkey) == 0 && !hkey.is_null() {
            let set_sz = |name: &str, val: &str| {
                let n: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
                let v: Vec<u16> = format!("{}\0", val).encode_utf16().collect();
                let _ = RegSetValueExW(
                    hkey,
                    n.as_ptr(),
                    0,
                    REG_SZ,
                    v.as_ptr() as *const u8,
                    (v.len() * 2) as u32,
                );
            };
            let set_dword = |name: &str, val: u32| {
                let n: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
                let _ = RegSetValueExW(
                    hkey,
                    n.as_ptr(),
                    0,
                    REG_DWORD,
                    &val as *const u32 as *const u8,
                    4,
                );
            };

            set_sz("DisplayName", APP_NAME);
            set_sz("DisplayVersion", APP_VERSION);
            set_sz("Publisher", "Riccardo Bettini");
            set_sz("DisplayIcon", &icon_path.display().to_string());
            set_sz("InstallLocation", &install_dir.display().to_string());
            set_sz(
                "UninstallString",
                &format!("\"{}\" --uninstall", exe_path.display()),
            );
            set_dword("NoModify", 1);
            set_dword("NoRepair", 1);
            set_dword("EstimatedSize", (PAYLOAD_EXE.len() / 1024) as u32);

            let _ = RegCloseKey(hkey);
        }

        // 2. Autostart con Windows
        let run_key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        let mut hrun: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, run_key.as_ptr(), 0, KEY_SET_VALUE, &mut hrun) == 0
            && !hrun.is_null()
        {
            let val_name: Vec<u16> = "MonitorManager\0".encode_utf16().collect();
            if autostart {
                let cmd: Vec<u16> = format!("\"{}\" --minimized\0", exe_path.display())
                    .encode_utf16()
                    .collect();
                let _ = RegSetValueExW(
                    hrun,
                    val_name.as_ptr(),
                    0,
                    REG_SZ,
                    cmd.as_ptr() as *const u8,
                    (cmd.len() * 2) as u32,
                );
            } else {
                let _ = RegDeleteValueW(hrun, val_name.as_ptr());
            }
            let _ = RegCloseKey(hrun);
        }
    }
}

fn perform_installation(
    options: &InstallOptions,
    proxy: Option<&EventLoopProxy<SetupEvent>>,
) -> Result<PathBuf, String> {
    let report = |step: &str, pct: u32| {
        if let Some(p) = proxy {
            let _ = p.send_event(SetupEvent::InstallProgress {
                step: step.to_string(),
                percent: pct,
            });
        }
    };

    report("Chiusura eventuali istanze aperte...", 15);
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "monitor-manager.exe"])
        .output();

    let install_dir = get_install_dir();
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Impossibile creare la cartella di installazione: {}", e))?;

    let exe_path = install_dir.join("monitor-manager.exe");
    let loader_path = install_dir.join("WebView2Loader.dll");
    let icon_path = install_dir.join("MonitorManager.ico");

    report("Copia dei file nativi di Monitor Manager v1.01...", 45);
    fs::write(&exe_path, PAYLOAD_EXE)
        .map_err(|e| format!("Impossibile scrivere monitor-manager.exe: {}", e))?;
    fs::write(&loader_path, WEBVIEW2_LOADER)
        .map_err(|e| format!("Impossibile scrivere WebView2Loader.dll: {}", e))?;

    let ico_bytes = build_ico_bytes();
    let _ = fs::write(&icon_path, ico_bytes);

    report("Configurazione scorciatoie e Menu Start...", 75);
    if options.start_menu_shortcut {
        if let Ok(appdata) = env::var("APPDATA") {
            let start_dir = PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs");
            let _ = fs::create_dir_all(&start_dir);
            let start_lnk = start_dir.join("Monitor Manager.lnk");
            create_windows_shortcut(&start_lnk, &exe_path, &icon_path, &install_dir);
        }
    }

    if options.desktop_shortcut {
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(userprofile)
                .join("Desktop")
                .join("Monitor Manager.lnk");
            create_windows_shortcut(&desktop_lnk, &exe_path, &icon_path, &install_dir);
        }
    }

    report("Registrazione app in Windows...", 92);
    register_uninstaller_and_autostart(&install_dir, &exe_path, &icon_path, options.autostart);

    report("Installazione completata!", 100);
    Ok(exe_path)
}

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

fn setup_html(install_path_str: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="it">
<head>
<meta charset="UTF-8" />
<title>Monitor Manager Setup</title>
<style>
:root {{
    --spring: cubic-bezier(0.22, 1.38, 0.36, 1);
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; user-select: none; }}
body {{
    width: 100vw; height: 100vh; overflow: hidden;
    font-family: "Segoe UI Variable Display", "Segoe UI", -apple-system, sans-serif;
    color: #f8fafc;
    background: linear-gradient(145deg, #090d1a 0%, #11182d 52%, #0b1324 100%);
    display: flex; flex-direction: column; padding: 14px;
}}
.blob {{
    position: fixed; border-radius: 50%; filter: blur(70px); pointer-events: none; z-index: 0;
}}
.b1 {{ width: 280px; height: 280px; top: -60px; left: -60px; background: rgba(56, 189, 248, 0.35); }}
.b2 {{ width: 300px; height: 300px; bottom: -80px; right: -60px; background: rgba(129, 140, 248, 0.34); }}
.b3 {{ width: 220px; height: 220px; top: 38%; left: 40%; background: rgba(232, 121, 249, 0.24); }}
.card {{
    position: relative; z-index: 1; flex: 1;
    display: flex; flex-direction: column; justify-content: space-between;
    border-radius: 26px;
    background: rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(44px) saturate(190%);
    border: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: inset 0 1.5px 0 rgba(255, 255, 255, 0.42), 0 20px 50px rgba(0, 0, 0, 0.45);
    padding: 22px 24px;
}}
.header {{
    display: flex; align-items: center; justify-content: space-between;
    padding-bottom: 14px; border-bottom: 1px solid rgba(255,255,255,0.14);
    cursor: grab;
}}
.brand {{ display: flex; align-items: center; gap: 13px; }}
.orb {{
    width: 44px; height: 44px; border-radius: 14px;
    background: linear-gradient(135deg, #38bdf8 0%, #818cf8 55%, #c084fc 100%);
    display: flex; align-items: center; justify-content: center;
    box-shadow: inset 0 1px 1px rgba(255,255,255,0.65), 0 6px 18px rgba(56,189,248,0.38);
}}
.orb svg {{ width: 24px; height: 24px; color: #fff; }}
.title h1 {{ font-size: 18px; font-weight: 700; letter-spacing: -0.02em; }}
.title span {{ font-size: 12px; color: #94a3b8; }}
.close-btn {{
    width: 30px; height: 30px; border-radius: 10px;
    border: 1px solid rgba(255,255,255,0.16);
    background: rgba(255,255,255,0.08); color: #e2e8f0;
    cursor: pointer; transition: all 0.25s var(--spring);
}}
.close-btn:hover {{ background: rgba(239,68,68,0.85); color: #fff; transform: scale(1.06); }}
.body {{ display: flex; flex-direction: column; gap: 12px; margin: 14px 0; }}
.path-box {{
    padding: 10px 14px; border-radius: 16px;
    background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.15);
    font-size: 11.5px; color: #cbd5e1;
}}
.path-box strong {{ display: block; color: #38bdf8; margin-bottom: 2px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; }}
.opt-group {{
    display: flex; flex-direction: column; gap: 10px;
    padding: 14px 16px; border-radius: 20px;
    background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.15);
}}
.opt-row {{ display: flex; align-items: center; justify-content: space-between; font-size: 13px; font-weight: 600; }}
.ios-switch {{ position: relative; width: 44px; height: 25px; }}
.ios-switch input {{ opacity: 0; width: 0; height: 0; }}
.slider {{
    position: absolute; inset: 0; cursor: pointer;
    background: rgba(255,255,255,0.15); border: 1px solid rgba(255,255,255,0.22);
    border-radius: 999px; transition: all 0.32s var(--spring);
}}
.slider::before {{
    content: ""; position: absolute; width: 19px; height: 19px; left: 2px; bottom: 2px;
    background: #fff; border-radius: 50%; transition: transform 0.32s var(--spring);
}}
.ios-switch input:checked + .slider {{
    background: linear-gradient(135deg, #38bdf8, #818cf8);
}}
.ios-switch input:checked + .slider::before {{ transform: translateX(19px); }}
.progress-wrap {{ display: none; flex-direction: column; gap: 8px; margin-top: 4px; }}
.progress-bar-bg {{
    width: 100%; height: 10px; border-radius: 999px;
    background: rgba(255,255,255,0.12); overflow: hidden;
}}
.progress-bar-fill {{
    height: 100%; width: 0%;
    background: linear-gradient(90deg, #38bdf8, #818cf8, #c084fc);
    transition: width 0.3s ease;
}}
.status-label {{ font-size: 12px; color: #cbd5e1; text-align: center; }}
.footer {{ display: flex; justify-content: flex-end; gap: 10px; }}
.btn-primary {{
    width: 100%; padding: 12px 18px; border-radius: 16px;
    background: linear-gradient(135deg, #38bdf8 0%, #818cf8 55%, #c084fc 100%);
    border: 1px solid rgba(255,255,255,0.5);
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.65), 0 8px 22px rgba(56,189,248,0.35);
    color: #fff; font-size: 14px; font-weight: 700; cursor: pointer;
    transition: transform 0.3s var(--spring), opacity 0.2s;
}}
.btn-primary:hover {{ transform: translateY(-1.5px) scale(1.01); }}
.btn-primary:active {{ transform: scale(0.98); }}
.btn-primary:disabled {{ opacity: 0.6; cursor: default; transform: none; }}
</style>
</head>
<body>
    <div class="blob b1"></div>
    <div class="blob b2"></div>
    <div class="blob b3"></div>
    <div class="card">
        <div class="header" id="drag-header">
            <div class="brand">
                <div class="orb">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="3"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
                </div>
                <div class="title">
                    <h1>Monitor Manager — Setup</h1>
                    <span>Versione v1.01 • Windows Native</span>
                </div>
            </div>
            <button class="close-btn" onclick="send({{cmd:'close'}})">✕</button>
        </div>

        <div class="body">
            <div class="path-box">
                <strong>Cartella di installazione</strong>
                {install_path}
            </div>

            <div class="opt-group" id="options-box">
                <div class="opt-row">
                    <span>Crea collegamento sul Desktop</span>
                    <label class="ios-switch"><input type="checkbox" id="opt-desktop" checked><span class="slider"></span></label>
                </div>
                <div class="opt-row">
                    <span>Aggiungi al Menu Start di Windows</span>
                    <label class="ios-switch"><input type="checkbox" id="opt-start" checked><span class="slider"></span></label>
                </div>
                <div class="opt-row">
                    <span>Avvia automaticamente con Windows</span>
                    <label class="ios-switch"><input type="checkbox" id="opt-auto"><span class="slider"></span></label>
                </div>
                <div class="opt-row">
                    <span>Avvia Monitor Manager al termine</span>
                    <label class="ios-switch"><input type="checkbox" id="opt-launch" checked><span class="slider"></span></label>
                </div>
            </div>

            <div class="progress-wrap" id="progress-wrap">
                <div class="progress-bar-bg"><div class="progress-bar-fill" id="progress-fill"></div></div>
                <div class="status-label" id="status-label">Preparazione installazione...</div>
            </div>
        </div>

        <div class="footer">
            <button class="btn-primary" id="action-btn" onclick="onMainAction()">Installa Monitor Manager v1.01</button>
        </div>
    </div>

<script>
let installed = false;
function send(obj) {{
    if (window.ipc) window.ipc.postMessage(JSON.stringify(obj));
}}
document.getElementById('drag-header').addEventListener('mousedown', (e) => {{
    if (e.button === 0 && e.target.tagName !== 'BUTTON') send({{cmd: 'drag_window'}});
}});
function onMainAction() {{
    if (installed) {{
        send({{cmd: 'finish', launch: document.getElementById('opt-launch').checked}});
        return;
    }}
    const btn = document.getElementById('action-btn');
    btn.disabled = true;
    btn.textContent = 'Installazione in corso...';
    document.getElementById('progress-wrap').style.display = 'flex';
    send({{
        cmd: 'start_install',
        options: {{
            desktop_shortcut: document.getElementById('opt-desktop').checked,
            start_menu_shortcut: document.getElementById('opt-start').checked,
            autostart: document.getElementById('opt-auto').checked,
            launch_after: document.getElementById('opt-launch').checked,
        }}
    }});
}}
window.onSetupEvent = function(ev) {{
    if (ev.type === 'progress') {{
        document.getElementById('progress-fill').style.width = ev.percent + '%';
        document.getElementById('status-label').textContent = ev.step;
    }} else if (ev.type === 'done') {{
        installed = true;
        document.getElementById('progress-fill').style.width = '100%';
        document.getElementById('status-label').textContent = '✓ Monitor Manager v1.01 è stato installato sul tuo PC!';
        const btn = document.getElementById('action-btn');
        btn.disabled = false;
        btn.textContent = ev.launch ? 'Avvia Monitor Manager e Chiudi' : 'Chiudi Setup';
    }} else if (ev.type === 'error') {{
        document.getElementById('status-label').textContent = 'Errore: ' + ev.message;
        const btn = document.getElementById('action-btn');
        btn.disabled = false;
        btn.textContent = 'Riprova Installazione';
    }}
}};
</script>
</body>
</html>"#,
        install_path = install_path_str
    )
}

fn main() {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let loader = dir.join("WebView2Loader.dll");
            if !loader.exists() {
                let _ = fs::write(loader, WEBVIEW2_LOADER);
            }
        }
    }

    // Supporto installazione rapida silenziosa (--silent)
    if env::args().any(|a| a == "--silent") {
        let opts = InstallOptions {
            desktop_shortcut: true,
            start_menu_shortcut: true,
            autostart: false,
            launch_after: false,
        };
        let _ = perform_installation(&opts, None);
        return;
    }

    let event_loop = EventLoopBuilder::<SetupEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let (rgba, w, h) = generate_icon_rgba();
    let icon = WindowIcon::from_rgba(rgba, w, h).ok();

    let window = WindowBuilder::new()
        .with_title("Monitor Manager v1.01 — Setup")
        .with_inner_size(Size::Logical(LogicalSize::new(540.0, 470.0)))
        .with_resizable(false)
        .with_decorations(false)
        .with_window_icon(icon)
        .build(&event_loop)
        .expect("Impossibile avviare la finestra del Setup");

    enable_win11_rounded_corners(window.hwnd());

    let install_dir_display = get_install_dir().display().to_string();
    let html = setup_html(&install_dir_display);
    let proxy_ipc = proxy.clone();

    let webview = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |req| {
            if let Ok(cmd) = serde_json::from_str::<SetupIpc>(req.body()) {
                match cmd {
                    SetupIpc::DragWindow => {
                        let _ = proxy_ipc.send_event(SetupEvent::DragWindow);
                    }
                    SetupIpc::Close => {
                        let _ = proxy_ipc.send_event(SetupEvent::CloseWindow);
                    }
                    SetupIpc::StartInstall { options } => {
                        let p = proxy_ipc.clone();
                        thread::spawn(move || {
                            match perform_installation(&options, Some(&p)) {
                                Ok(exe_path) => {
                                    let _ = p.send_event(SetupEvent::InstallSuccess {
                                        install_dir: exe_path.display().to_string(),
                                        launch: options.launch_after,
                                    });
                                }
                                Err(e) => {
                                    let _ = p.send_event(SetupEvent::InstallError(e));
                                }
                            }
                        });
                    }
                    SetupIpc::Finish { launch } => {
                        if launch {
                            let exe_path = get_install_dir().join("monitor-manager.exe");
                            let _ = Command::new(exe_path).spawn();
                        }
                        let _ = proxy_ipc.send_event(SetupEvent::CloseWindow);
                    }
                }
            }
        })
        .build(&window)
        .expect("Impossibile creare la WebView del Setup");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(ev) => match ev {
                SetupEvent::DragWindow => {
                    let _ = window.drag_window();
                }
                SetupEvent::CloseWindow => {
                    *control_flow = ControlFlow::Exit;
                }
                SetupEvent::InstallProgress { step, percent } => {
                    let payload = json!({
                        "type": "progress",
                        "step": step,
                        "percent": percent,
                    });
                    let _ = webview.evaluate_script(&format!("window.onSetupEvent({});", payload));
                }
                SetupEvent::InstallSuccess { launch, .. } => {
                    let payload = json!({
                        "type": "done",
                        "launch": launch,
                    });
                    let _ = webview.evaluate_script(&format!("window.onSetupEvent({});", payload));
                }
                SetupEvent::InstallError(msg) => {
                    let payload = json!({
                        "type": "error",
                        "message": msg,
                    });
                    let _ = webview.evaluate_script(&format!("window.onSetupEvent({});", payload));
                }
            },
            _ => {}
        }
    });
}
