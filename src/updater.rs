use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub const APP_VERSION: &str = "1.15";
pub const GITHUB_REPO: &str = "riccardo-bettini/Monitor-Manager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    Downloading,
    Installing,
}

#[derive(Debug, Clone)]
pub struct UpdateProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub fraction: f32,
    pub speed_bps: f64,
    pub phase: UpdatePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_notes: String,
    pub html_url: String,
    pub download_url: Option<String>,
    pub download_size: u64,
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
    #[serde(default)]
    size: u64,
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
                    .map(|a| (a.browser_download_url.clone(), a.size));

                let (download_url, download_size) = match exe_asset {
                    Some((u, s)) => (Some(u), s),
                    None => (None, 0),
                };

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
                    download_url,
                    download_size,
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
                    download_size: 0,
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
                download_size: 0,
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
            download_size: 0,
            status_message: format!("Impossibile verificare aggiornamenti: {}", e),
        },
    }
}

/// Formatta percentuale, MB scaricati e velocità per la UI della barra di progressione
pub fn format_progress_detail(downloaded: u64, total: u64, speed_bps: f64) -> String {
    let dl_mb = downloaded as f64 / (1024.0 * 1024.0);
    let speed_str = if speed_bps > 1024.0 {
        format!(" · {:.1} MB/s", speed_bps / (1024.0 * 1024.0))
    } else {
        String::new()
    };
    if total > 0 {
        let tot_mb = total as f64 / (1024.0 * 1024.0);
        let pct = ((downloaded as f64 / total as f64) * 100.0)
            .clamp(0.0, 100.0)
            .round() as u32;
        format!("{}% · {:.1} / {:.1} MB{}", pct, dl_mb, tot_mb, speed_str)
    } else {
        format!("{:.1} MB{}", dl_mb, speed_str)
    }
}

/// Scarica l'aggiornamento in streaming notificando il progresso e restituisce il percorso dell'eseguibile da avviare al riavvio
pub fn perform_self_update_with_progress<F>(
    download_url: &str,
    target_version: &str,
    fallback_total_bytes: u64,
    cancel_flag: &AtomicBool,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(UpdateProgress),
{
    if cancel_flag.load(Ordering::SeqCst) {
        return Err("Aggiornamento annullato.".to_string());
    }

    let current_exe = env::current_exe().map_err(|e| format!("Percorso exe non trovato: {}", e))?;
    let is_setup = download_url.to_lowercase().contains("setup");

    let resp = ureq::get(download_url)
        .set("User-Agent", "Monitor-Manager-Updater/1.15")
        .call()
        .map_err(|e| format!("Errore download aggiornamento: {}", e))?;

    let header_len = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let total_bytes = if header_len > 0 {
        header_len
    } else {
        fallback_total_bytes
    };

    let mut bytes = Vec::with_capacity(total_bytes.min(64 * 1024 * 1024) as usize);
    let mut reader = resp.into_reader();
    let mut chunk = [0u8; 32768];
    let mut last_reported_bytes: u64 = 0;
    let start_time = Instant::now();

    on_progress(UpdateProgress {
        downloaded_bytes: 0,
        total_bytes,
        fraction: 0.0,
        speed_bps: 0.0,
        phase: UpdatePhase::Downloading,
    });

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err("Aggiornamento annullato.".to_string());
        }
        let read_n = reader
            .read(&mut chunk)
            .map_err(|e| format!("Errore lettura stream download: {}", e))?;
        if read_n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read_n]);
        let downloaded = bytes.len() as u64;
        if downloaded.saturating_sub(last_reported_bytes) >= 65_536
            || (total_bytes > 0 && downloaded >= total_bytes)
        {
            last_reported_bytes = downloaded;
            let elapsed = start_time.elapsed().as_secs_f64().max(0.05);
            let speed_bps = downloaded as f64 / elapsed;
            let fraction = if total_bytes > 0 {
                (downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            on_progress(UpdateProgress {
                downloaded_bytes: downloaded,
                total_bytes,
                fraction,
                speed_bps,
                phase: UpdatePhase::Downloading,
            });
        }
    }

    if cancel_flag.load(Ordering::SeqCst) {
        return Err("Aggiornamento annullato.".to_string());
    }

    if bytes.len() < 1024 {
        return Err("Il file scaricato non sembra un eseguibile valido.".to_string());
    }

    let final_dl = bytes.len() as u64;
    let elapsed = start_time.elapsed().as_secs_f64().max(0.05);
    on_progress(UpdateProgress {
        downloaded_bytes: final_dl,
        total_bytes: if total_bytes > 0 { total_bytes } else { final_dl },
        fraction: 1.0,
        speed_bps: final_dl as f64 / elapsed,
        phase: UpdatePhase::Installing,
    });

    if is_setup {
        let ver_clean = if target_version.trim().is_empty() {
            APP_VERSION
        } else {
            target_version.trim()
        };
        let temp_setup = env::temp_dir().join(format!("Monitor-Manager-Setup-v{}.exe", ver_clean));
        fs::write(&temp_setup, &bytes)
            .map_err(|e| format!("Errore scrittura installer temporaneo: {}", e))?;
        Ok(temp_setup)
    } else {
        let temp_exe = current_exe.with_extension("new.exe");
        let old_exe = current_exe.with_extension("old.exe");

        fs::write(&temp_exe, &bytes).map_err(|e| format!("Errore scrittura nuovo exe: {}", e))?;

        if old_exe.exists() {
            let _ = fs::remove_file(&old_exe);
        }

        fs::rename(&current_exe, &old_exe)
            .map_err(|e| format!("Impossibile rinominare l'eseguibile in uso: {}", e))?;

        if let Err(e) = fs::rename(&temp_exe, &current_exe) {
            let _ = fs::rename(&old_exe, &current_exe);
            return Err(format!("Impossibile attivare il nuovo eseguibile: {}", e));
        }

        Ok(current_exe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_detail_formats_percent_and_megabytes() {
        let full = format_progress_detail(5 * 1024 * 1024, 10 * 1024 * 1024, 2.5 * 1024.0 * 1024.0);
        assert_eq!(full, "50% · 5.0 / 10.0 MB · 2.5 MB/s");

        let unknown = format_progress_detail(2 * 1024 * 1024, 0, 0.0);
        assert_eq!(unknown, "2.0 MB");
    }

    #[test]
    fn cancel_flag_aborts_before_network_request() {
        let cancel = AtomicBool::new(true);
        let mut updates = Vec::new();
        let res = perform_self_update_with_progress(
            "https://example.invalid/monitor-manager.exe",
            "1.16",
            1000,
            &cancel,
            |p| updates.push(p),
        );
        assert!(res.is_err());
        assert!(updates.is_empty());
    }
}
