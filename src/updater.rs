use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Read;
use std::process::Command;

pub const APP_VERSION: &str = "1.13";
pub const GITHUB_REPO: &str = "riccardo-bettini/Monitor-Manager";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_notes: String,
    pub html_url: String,
    pub download_url: Option<String>,
    pub status_message: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Rimuove eventuali file `.old.exe` lasciati da un aggiornamento precedente
pub fn cleanup_old_executable() {
    if let Ok(exe) = env::current_exe() {
        let old_exe = exe.with_extension("old.exe");
        if old_exe.exists() {
            let _ = fs::remove_file(old_exe);
        }
    }
}

/// Converte una stringa di versione come "v1.00", "1.01", "1.0.1" in un valore numerico confrontabile
fn parse_version_score(ver: &str) -> i64 {
    let clean = ver.trim().trim_start_matches(['v', 'V']);
    // Se è nel formato "1.00", "1.01", "1.02", moltiplichiamo per 100
    if let Ok(val) = clean.parse::<f64>() {
        return (val * 100.0).round() as i64;
    }
    // Fallback per X.Y.Z
    let parts: Vec<i64> = clean
        .split('.')
        .filter_map(|p| p.parse::<i64>().ok())
        .collect();
    match parts.as_slice() {
        [maj, min] => maj * 100 + min,
        [maj, min, patch] => maj * 100 + min * 10 + patch,
        [maj] => maj * 100,
        _ => 100,
    }
}

pub fn check_for_updates() -> UpdateCheckResult {
    let api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = ureq::get(&api_url)
        .set("User-Agent", "Monitor-Manager-Updater/1.00")
        .set("Accept", "application/vnd.github+json")
        .call();

    match response {
        Ok(resp) => {
            if let Ok(release) = resp.into_json::<GitHubRelease>() {
                let latest_clean = release
                    .tag_name
                    .trim()
                    .trim_start_matches(['v', 'V'])
                    .to_string();
                let current_score = parse_version_score(APP_VERSION);
                let latest_score = parse_version_score(&latest_clean);
                let update_available = latest_score > current_score;

                let exe_asset = release
                    .assets
                    .iter()
                    .find(|a| {
                        let n = a.name.to_lowercase();
                        n == "monitor-manager.exe" || (n.ends_with(".exe") && !n.contains("setup"))
                    })
                    .or_else(|| {
                        release
                            .assets
                            .iter()
                            .find(|a| a.name.to_lowercase().ends_with(".exe"))
                    })
                    .map(|a| a.browser_download_url.clone());

                let status_message = if update_available {
                    format!("Nuova versione v{} disponibile su GitHub!", latest_clean)
                } else {
                    format!("Monitor Manager è aggiornato (v{})", APP_VERSION)
                };

                UpdateCheckResult {
                    current_version: APP_VERSION.to_string(),
                    latest_version: latest_clean,
                    update_available,
                    release_notes: release.body.unwrap_or_default(),
                    html_url: release.html_url,
                    download_url: exe_asset,
                    status_message,
                }
            } else {
                UpdateCheckResult {
                    current_version: APP_VERSION.to_string(),
                    latest_version: APP_VERSION.to_string(),
                    update_available: false,
                    release_notes: String::new(),
                    html_url: format!("https://github.com/{}/releases", GITHUB_REPO),
                    download_url: None,
                    status_message: "Risposta GitHub non valida.".to_string(),
                }
            }
        }
        Err(ureq::Error::Status(404, _)) => {
            // Nessuna release ancora pubblicata sulla repo: siamo sulla v1.00 corrente
            UpdateCheckResult {
                current_version: APP_VERSION.to_string(),
                latest_version: APP_VERSION.to_string(),
                update_available: false,
                release_notes: String::new(),
                html_url: format!("https://github.com/{}/releases", GITHUB_REPO),
                download_url: None,
                status_message: format!(
                    "Sei all'ultima versione disponibile (v{})",
                    APP_VERSION
                ),
            }
        }
        Err(e) => UpdateCheckResult {
            current_version: APP_VERSION.to_string(),
            latest_version: APP_VERSION.to_string(),
            update_available: false,
            release_notes: String::new(),
            html_url: format!("https://github.com/{}/releases", GITHUB_REPO),
            download_url: None,
            status_message: format!("Impossibile verificare aggiornamenti: {}", e),
        },
    }
}

/// Scarica il nuovo eseguibile o installer da GitHub Releases, sostituisce il binario o avvia il setup ed esce in sicurezza
pub fn perform_self_update(download_url: &str) -> Result<(), String> {
    let current_exe = env::current_exe().map_err(|e| format!("Percorso exe non trovato: {}", e))?;
    let is_setup = download_url.to_lowercase().contains("setup");

    let resp = ureq::get(download_url)
        .set("User-Agent", "Monitor-Manager-Updater/1.05")
        .call()
        .map_err(|e| format!("Errore download aggiornamento: {}", e))?;

    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Errore lettura stream download: {}", e))?;

    if bytes.len() < 1024 {
        return Err("Il file scaricato non sembra un eseguibile valido.".to_string());
    }

    if is_setup {
        // Se l'asset scaricato è un installer setup, lo salviamo in %TEMP% con nome dedicato
        let temp_setup = env::temp_dir().join(format!("Monitor-Manager-Setup-v{}.exe", APP_VERSION));
        fs::write(&temp_setup, &bytes).map_err(|e| format!("Errore scrittura installer temporaneo: {}", e))?;

        // Avviamo l'installer e terminiamo l'istanza corrente dell'app così che i file possano essere sostituiti
        let _ = Command::new(&temp_setup).spawn();
        std::process::exit(0);
    } else {
        // Aggiornamento diretto in-place dell'eseguibile standalone
        let temp_exe = current_exe.with_extension("new.exe");
        let old_exe = current_exe.with_extension("old.exe");

        fs::write(&temp_exe, &bytes).map_err(|e| format!("Errore scrittura nuovo exe: {}", e))?;

        if old_exe.exists() {
            let _ = fs::remove_file(&old_exe);
        }

        fs::rename(&current_exe, &old_exe)
            .map_err(|e| format!("Impossibile rinominare l'eseguibile in uso: {}", e))?;

        if let Err(e) = fs::rename(&temp_exe, &current_exe) {
            // Rollback in caso di errore imprevisto
            let _ = fs::rename(&old_exe, &current_exe);
            return Err(format!("Impossibile attivare il nuovo eseguibile: {}", e));
        }

        // Avvia la nuova versione e termina l'istanza corrente
        let _ = Command::new(&current_exe).spawn();
        std::process::exit(0);
    }
}
