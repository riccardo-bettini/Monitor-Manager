use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// "system" | "dark" | "light"
    pub theme: String,
    /// "multi" (Schede Affiancate) | "single" (Selettore Superiore)
    pub layout_mode: String,
    /// Nasconde gli slider per i codici VCP non supportati dal singolo monitor
    pub hide_unsupported: bool,
    /// Sincronizza le modifiche su tutti i monitor collegati
    pub sync_monitors: bool,
    /// Avvia automaticamente Monitor Manager all'avvio di Windows
    pub start_with_windows: bool,
    /// Controlla automaticamente gli aggiornamenti su GitHub all'apertura
    pub auto_check_updates: bool,
    /// Minimizza nella System Tray invece di chiudere quando si preme la X
    pub close_to_tray: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            layout_mode: "multi".to_string(),
            hide_unsupported: false,
            sync_monitors: false,
            start_with_windows: false,
            auto_check_updates: true,
            close_to_tray: true,
        }
    }
}

impl AppSettings {
    fn settings_path() -> PathBuf {
        let base = env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let dir = base.join("MonitorManager");
        let _ = fs::create_dir_all(&dir);
        dir.join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::settings_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                return settings;
            }
        }
        let default = Self::default();
        let _ = default.save();
        default
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::settings_path();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;
        self.apply_autostart();
        Ok(())
    }

    pub fn apply_autostart(&self) {
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };

        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        let value_name: Vec<u16> = "MonitorManager\0".encode_utf16().collect();

        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            );
            if status == 0 && !hkey.is_null() {
                if self.start_with_windows {
                    if let Ok(exe_path) = env::current_exe() {
                        let cmd = format!("\"{}\" --minimized\0", exe_path.display());
                        let wide: Vec<u16> = cmd.encode_utf16().collect();
                        let _ = RegSetValueExW(
                            hkey,
                            value_name.as_ptr(),
                            0,
                            REG_SZ,
                            wide.as_ptr() as *const u8,
                            (wide.len() * 2) as u32,
                        );
                    }
                } else {
                    let _ = RegDeleteValueW(hkey, value_name.as_ptr());
                }
                let _ = RegCloseKey(hkey);
            }
        }
    }
}
