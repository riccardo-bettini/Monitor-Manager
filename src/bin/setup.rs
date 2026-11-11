#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

use slint::{ComponentHandle, SharedString};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

const APP_VERSION: &str = "1.06";
const APP_NAME: &str = "Monitor Manager";
const PAYLOAD_EXE: &[u8] = include_bytes!("../../target/release/monitor-manager.exe");

#[derive(Debug, Clone)]
struct InstallOptions {
    desktop_shortcut: bool,
    start_menu_shortcut: bool,
    autostart: bool,
}

fn get_install_dir() -> PathBuf {
    let base = env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\Program Files"));
    base.join("Programs").join("Monitor Manager")
}

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

fn build_ico_bytes() -> Vec<u8> {
    let (rgba, w, h) = generate_icon_rgba();
    let xor_size = (w * h * 4) as u32;
    let and_row_bytes = ((w + 31) / 32) * 4;
    let and_size = and_row_bytes * h;
    let bmp_size = 40 + xor_size + and_size;

    let mut ico = Vec::with_capacity((6 + 16 + bmp_size) as usize);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());

    ico.push(w as u8);
    ico.push(h as u8);
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&bmp_size.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());

    ico.extend_from_slice(&40u32.to_le_bytes());
    ico.extend_from_slice(&(w as i32).to_le_bytes());
    ico.extend_from_slice(&((h * 2) as i32).to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&(xor_size + and_size).to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());

    for y in (0..h).rev() {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            ico.push(rgba[idx + 2]);
            ico.push(rgba[idx + 1]);
            ico.push(rgba[idx]);
            ico.push(rgba[idx + 3]);
        }
    }

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
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &ps_script,
        ])
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

fn perform_installation(options: &InstallOptions) -> Result<PathBuf, String> {
    // Termina altre istanze in esecuzione dell'app, escludendo rigorosamente il processo dell'installer corrente
    let my_pid = std::process::id();
    let filter = format!("PID ne {}", my_pid);
    let _ = Command::new("taskkill")
        .args(["/F", "/FI", &filter, "/IM", "monitor-manager.exe"])
        .output();

    // Breve pausa per assicurarsi che i file handle siano stati rilasciati da Windows
    thread::sleep(std::time::Duration::from_millis(300));

    let install_dir = get_install_dir();
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Impossibile creare la cartella di installazione: {}", e))?;

    // Rimuove eventuali residui WebView2 di versioni precedenti (v1.00 / v1.01)
    let _ = fs::remove_file(install_dir.join("WebView2Loader.dll"));
    let _ = fs::remove_dir_all(install_dir.join("monitor-manager.exe.WebView2"));

    let exe_path = install_dir.join("monitor-manager.exe");
    let icon_path = install_dir.join("MonitorManager.ico");

    fs::write(&exe_path, PAYLOAD_EXE)
        .map_err(|e| format!("Impossibile scrivere monitor-manager.exe: {}", e))?;

    let ico_bytes = build_ico_bytes();
    let _ = fs::write(&icon_path, ico_bytes);

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

    register_uninstaller_and_autostart(&install_dir, &exe_path, &icon_path, options.autostart);
    Ok(exe_path)
}

fn main() {
    if env::var("SLINT_BACKEND").is_err() {
        env::set_var("SLINT_BACKEND", "winit-software");
    }

    if env::args().any(|a| a == "--silent") {
        let opts = InstallOptions {
            desktop_shortcut: true,
            start_menu_shortcut: true,
            autostart: false,
        };
        let _ = perform_installation(&opts);
        return;
    }

    let ui = SetupWindow::new().expect("Errore avvio finestra Setup Slint");
    ui.set_install_path(SharedString::from(
        get_install_dir().display().to_string().as_str(),
    ));

    let uw = ui.as_weak();
    ui.on_start_install(move || {
        if let Some(u) = uw.upgrade() {
            u.set_installing(true);
            u.set_status_text(SharedString::from(
                format!("Installazione di Monitor Manager v{} in corso...", APP_VERSION).as_str(),
            ));
            let opts = InstallOptions {
                desktop_shortcut: u.get_opt_desktop(),
                start_menu_shortcut: u.get_opt_start_menu(),
                autostart: u.get_opt_autostart(),
            };
            let uw_thread = uw.clone();
            thread::spawn(move || match perform_installation(&opts) {
                Ok(_) => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(u) = uw_thread.upgrade() {
                            u.set_installing(false);
                            u.set_installed(true);
                            u.set_status_text(SharedString::from(
                                format!("✓ Monitor Manager v{} installato con successo!", APP_VERSION).as_str(),
                            ));
                        }
                    });
                }
                Err(err) => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(u) = uw_thread.upgrade() {
                            u.set_installing(false);
                            u.set_status_text(SharedString::from(err.as_str()));
                        }
                    });
                }
            });
        }
    });

    let uw_fin = ui.as_weak();
    ui.on_finish_setup(move || {
        if let Some(u) = uw_fin.upgrade() {
            if u.get_opt_launch() {
                let exe_path = get_install_dir().join("monitor-manager.exe");
                let _ = Command::new(exe_path).spawn();
            }
        }
        let _ = slint::quit_event_loop();
    });

    let _ = ui.run();
}
