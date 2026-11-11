use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

fn default_language() -> String {
    "it".to_string()
}
fn default_layout_mode() -> String {
    "single".to_string()
}
fn default_true() -> bool {
    true
}
fn default_accent_palette() -> String {
    "coral".to_string()
}
fn default_glow_style() -> String {
    "subtle".to_string()
}
fn default_layout_density() -> String {
    "standard".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Lingua interfaccia: "it" (Italiano) | "en" (English)
    #[serde(default = "default_language")]
    pub language: String,
    /// "single" (Dashboard Monitor Singolo) | "multi" (Tutti i Monitor Affiancati)
    #[serde(default = "default_layout_mode")]
    pub layout_mode: String,
    /// Nasconde i controlli VCP non supportati dal singolo monitor
    #[serde(default)]
    pub hide_unsupported: bool,
    /// Sincronizza le modifiche su tutti i monitor collegati
    #[serde(default)]
    pub sync_monitors: bool,
    /// Avvia automaticamente Monitor Manager all'avvio di Windows
    #[serde(default)]
    pub start_with_windows: bool,
    /// Controlla automaticamente gli aggiornamenti su GitHub all'apertura
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    /// Minimizza nella System Tray invece di chiudere quando si preme la X
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    /// Palette colore d'accento Dark Matte Titanium: "coral" | "cyan" | "emerald" | "violet" | "amber"
    #[serde(default = "default_accent_palette")]
    pub accent_palette: String,
    /// Stile Glow dei bordi: "none" | "subtle" | "intense"
    #[serde(default = "default_glow_style")]
    pub glow_style: String,
    /// Densità del layout: "standard" | "compact"
    #[serde(default = "default_layout_density")]
    pub layout_density: String,
    /// Elenco ordinato dei monitor per memorizzare l'ordinamento manuale utente
    #[serde(default)]
    pub monitor_order: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            layout_mode: default_layout_mode(),
            hide_unsupported: false,
            sync_monitors: false,
            start_with_windows: false,
            auto_check_updates: true,
            close_to_tray: true,
            accent_palette: default_accent_palette(),
            glow_style: default_glow_style(),
            layout_density: default_layout_density(),
            monitor_order: Vec::new(),
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
            if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&content) {
                // Normalizza eventuali valori legacy
                if settings.accent_palette == "purple" {
                    settings.accent_palette = "violet".to_string();
                } else if !matches!(
                    settings.accent_palette.as_str(),
                    "coral" | "cyan" | "emerald" | "violet" | "amber"
                ) {
                    settings.accent_palette = "coral".to_string();
                }
                if !matches!(settings.glow_style.as_str(), "none" | "subtle" | "intense") {
                    settings.glow_style = "subtle".to_string();
                }
                if !matches!(settings.layout_density.as_str(), "standard" | "compact") {
                    settings.layout_density = "standard".to_string();
                }
                if !matches!(settings.language.as_str(), "it" | "en") {
                    settings.language = "it".to_string();
                }
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
