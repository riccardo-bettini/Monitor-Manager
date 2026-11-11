use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

fn default_theme() -> String {
    "system".to_string()
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
fn default_custom_accent_hex() -> String {
    "#e15b48".to_string()
}
fn default_glass_blur() -> u32 {
    44
}
fn default_blob_intensity() -> u32 {
    85
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// "system" | "dark" | "light"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// "single" (Dashboard Monitor Singolo) | "multi" (Schede Affiancate)
    #[serde(default = "default_layout_mode")]
    pub layout_mode: String,
    /// Nasconde gli slider per i codici VCP non supportati dal singolo monitor
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
    /// Palette colore accento e Hero Card: "coral" | "cyan" | "emerald" | "purple" | "amber" | "custom"
    #[serde(default = "default_accent_palette")]
    pub accent_palette: String,
    /// Colore esadecimale scelto dall'utente quando accent_palette == "custom"
    #[serde(default = "default_custom_accent_hex")]
    pub custom_accent_hex: String,
    /// Intensità sfocatura vetro (backdrop-filter blur in px, 16..=64)
    #[serde(default = "default_glass_blur")]
    pub glass_blur: u32,
    /// Opacità / intensità delle macchie di colore fluttuanti sullo sfondo (0..=100)
    #[serde(default = "default_blob_intensity")]
    pub blob_intensity: u32,
    /// Attiva/disattiva il movimento fluttuante delle macchie di colore sullo sfondo
    #[serde(default = "default_true")]
    pub animate_blobs: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            layout_mode: default_layout_mode(),
            hide_unsupported: false,
            sync_monitors: false,
            start_with_windows: false,
            auto_check_updates: true,
            close_to_tray: true,
            accent_palette: default_accent_palette(),
            custom_accent_hex: default_custom_accent_hex(),
            glass_blur: default_glass_blur(),
            blob_intensity: default_blob_intensity(),
            animate_blobs: true,
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

    pub fn effective_dark_mode(&self) -> bool {
        match self.theme.as_str() {
            "light" => false,
            "dark" => true,
            _ => is_windows_dark_mode(),
        }
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

pub fn is_windows_dark_mode() -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };
    let subkey: Vec<u16> =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
            .encode_utf16()
            .collect();
    let val_name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0
            && !hkey.is_null()
        {
            let mut data: u32 = 0;
            let mut size: u32 = 4;
            let status = RegQueryValueExW(
                hkey,
                val_name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut data as *mut u32 as *mut u8,
                &mut size,
            );
            let _ = RegCloseKey(hkey);
            if status == 0 {
                return data == 0;
            }
        }
    }
    true
}
