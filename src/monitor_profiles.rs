use mccs::{Capabilities, FeatureCode, ValueNames, Version};
use mccs_db::{Access, Database, ValueType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcpValueOption {
    pub value: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredVcp {
    pub code: u8,
    pub name_it: String,
    pub name_en: String,
    /// "image" | "color" | "gaming" | "audio_input" | "system"
    pub category: String,
    pub is_continuous: bool,
    pub current: u32,
    pub max: u32,
    pub is_read_only: bool,
    pub is_critical: bool,
    pub from_capabilities: bool,
    pub options: Vec<VcpValueOption>,
}

pub struct VcpMeta {
    pub name_it: String,
    pub name_en: String,
    pub category: String,
    pub is_continuous: bool,
    pub is_read_only: bool,
    pub is_critical: bool,
    pub options: Vec<VcpValueOption>,
}

/// Parsifica in modo resiliente la stringa MCCS Capabilities restituita dal monitor.
pub fn parse_capabilities_resilient(raw: &str) -> (Option<Capabilities>, Database) {
    let trimmed = raw.trim().trim_end_matches('\0').trim();
    let parsed = mccs_caps::parse_capabilities(trimmed).ok().or_else(|| {
        // Alcuni firmware inviano byte sporchi dopo l'ultima ')' o prima della prima '('
        let start = trimmed.find('(')?;
        let end = trimmed.rfind(')')?;
        if end > start {
            mccs_caps::parse_capabilities(&trimmed[start..=end]).ok()
        } else {
            None
        }
    });

    if let Some(ref caps) = parsed {
        let ver = caps
            .mccs_version
            .clone()
            .unwrap_or_else(|| Version::new(2, 2));
        let mut db = Database::from_version(&ver);
        db.apply_capabilities(caps);
        (parsed, db)
    } else {
        let db = Database::from_version(&Version::new(2, 2));
        (None, db)
    }
}

/// Restituisce l'insieme ordinato di codici VCP da interrogare in base a Capabilities + Brand.
pub fn collect_probe_codes(
    caps: Option<&Capabilities>,
    brand: &str,
    model_code: &str,
) -> Vec<u8> {
    let mut set = BTreeSet::new();

    // 1. Tutti i codici dichiarati esplicitamente nella stringa Capabilities MCCS del monitor
    if let Some(c) = caps {
        for &code in c.vcp_features.keys() {
            set.insert(code);
        }
    }

    // 2. Codici MCCS standard utili (anche su monitor con Capabilities troncate o assenti)
    const UNIVERSAL_EXTRA: &[u8] = &[
        0x0B, // Color Temp Increment
        0x0C, // Color Temp Request
        0x10, // Brightness
        0x12, // Contrast
        0x13, // Backlight Control
        0x14, // Select Color Preset
        0x16, // Red Gain
        0x18, // Green Gain
        0x1A, // Blue Gain
        0x52, // Active Control
        0x60, // Input Source
        0x62, // Audio Speaker Volume
        0x6C, // Red Black Level
        0x6E, // Green Black Level
        0x70, // Blue Black Level
        0x72, // Gamma
        0x86, // Display Scaling
        0x87, // Sharpness
        0x8A, // Color Saturation
        0x8D, // Audio Mute
        0x90, // Hue
        0x92, // Black Level / Luminance
        0xAC, // Horizontal Frequency
        0xAE, // Vertical Frequency
        0xCA, // OSD Control / Lock
        0xCC, // OSD Language
        0xD6, // Power Mode
        0xDC, // Display Application / Preset Mode (GameVisual / SmartImage / Game Mode)
        0xDF, // VCP Version
    ];
    for &c in UNIVERSAL_EXTRA {
        set.insert(c);
    }

    // 3. Sonde mirate per i brand prioritari (ASUS TUF, AOC, Philips, KTC)
    let b = brand.to_uppercase();
    let m = model_code.to_uppercase();
    if b.contains("ASUS") || matches!(m.as_str(), "AUS" | "ACI" | "ASU") {
        // ASUS TUF / ROG: GameVisual (0xDC), VividPixel / Overdrive / Blue Light / Shadow Boost (0xE0..0xF1)
        for c in [0xE0u8, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE8, 0xEA, 0xEB, 0xEC, 0xED, 0xF0, 0xF1] {
            set.insert(c);
        }
    } else if b.contains("AOC") || m == "AOC" {
        // AOC Gaming: Game Mode (0xDC), LowBlue / Shadow Control / Game Color / Overdrive / MBR
        for c in [0xE0u8, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE8, 0xEC, 0xEE, 0xF0] {
            set.insert(c);
        }
    } else if b.contains("PHILIPS") || m == "PHL" {
        // Philips Evnia / Momentum: SmartImage (0xDC), SmartResponse, SmartContrast, LowBlue
        for c in [0xE0u8, 0xE1, 0xE2, 0xE3, 0xE4, 0xE6, 0xEA, 0xEB, 0xEC, 0xED] {
            set.insert(c);
        }
    } else if b.contains("KTC") || m == "KTC" {
        // KTC: Picture/Gaming Mode (0xDC), Black Equalizer, Overdrive, Low Blue Light, MPRT
        for c in [0xE0u8, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE8, 0xEA, 0xEB, 0xEC] {
            set.insert(c);
        }
    }

    // Escludiamo codici puramente operativi one-shot senza senso come parametri live,
    // mantenendo comunque i reset critici SOLO se dichiarati dal monitor o mappati in sicurezza.
    set.into_iter().collect()
}

/// Restituisce true per i codici VCP potenzialmente distruttivi (richiedono conferma UI)
pub fn is_critical_vcp(code: u8) -> bool {
    matches!(code, 0x04 | 0x05 | 0x06 | 0x08 | 0xD6)
}

/// Codici one-shot / interni di sola lettura o non adatti a slider continui in UI
pub fn is_write_only_reset(code: u8) -> bool {
    matches!(code, 0x04 | 0x05 | 0x06 | 0x08 | 0x0A)
}

fn friendly_category_and_names(code: u8, brand: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let b = brand.to_uppercase();
    let is_asus = b.contains("ASUS");
    let is_aoc = b.contains("AOC");
    let is_philips = b.contains("PHILIPS");
    let is_ktc = b.contains("KTC");

    Some(match code {
        // Immagine
        0x10 => ("image", "Luminosità", "Brightness"),
        0x12 => ("image", "Contrasto", "Contrast"),
        0x13 => ("image", "Controllo Retroilluminazione", "Backlight Control"),
        0x87 => ("image", "Nitidezza", "Sharpness"),
        0x8A => ("image", "Saturazione Colore", "Color Saturation"),
        0x90 => ("image", "Tonalità (Hue)", "Hue"),
        0x92 => ("image", "Livello Nero / Luminanza", "Black Level"),
        0x86 => ("image", "Proporzioni / Scaling", "Display Scaling"),

        // Colore & Gamma
        0x14 => ("color", "Preset Temperatura Colore", "Color Temp Preset"),
        0x0B => ("color", "Incremento Kelvin", "Color Temp Increment"),
        0x0C => ("color", "Temperatura Kelvin", "Color Temp (Kelvin)"),
        0x16 => ("color", "Guadagno Rosso (R)", "Red Gain"),
        0x18 => ("color", "Guadagno Verde (G)", "Green Gain"),
        0x1A => ("color", "Guadagno Blu (B)", "Blue Gain"),
        0x6C => ("color", "Livello Nero Rosso", "Red Black Level"),
        0x6E => ("color", "Livello Nero Verde", "Green Black Level"),
        0x70 => ("color", "Livello Nero Blu", "Blue Black Level"),
        0x72 => ("color", "Curva Gamma", "Gamma"),
        0x59 => ("color", "Saturazione 6-Assi Rosso", "6-Axis Red Sat"),
        0x5A => ("color", "Saturazione 6-Assi Giallo", "6-Axis Yellow Sat"),
        0x5B => ("color", "Saturazione 6-Assi Verde", "6-Axis Green Sat"),
        0x5C => ("color", "Saturazione 6-Assi Ciano", "6-Axis Cyan Sat"),
        0x5D => ("color", "Saturazione 6-Assi Blu", "6-Axis Blue Sat"),
        0x5E => ("color", "Saturazione 6-Assi Magenta", "6-Axis Magenta Sat"),

        // Gaming & Brand
        0xDC => {
            if is_asus {
                ("gaming", "ASUS GameVisual (Modalità)", "ASUS GameVisual Mode")
            } else if is_aoc {
                ("gaming", "AOC Game Mode (Modalità)", "AOC Game Mode")
            } else if is_philips {
                ("gaming", "Philips SmartImage", "Philips SmartImage")
            } else if is_ktc {
                ("gaming", "KTC Modalità Immagine/Gaming", "KTC Picture Mode")
            } else {
                ("gaming", "Modalità Display / Applicazione", "Display Mode")
            }
        }
        0xE0..=0xFF => {
            if is_asus {
                match code {
                    0xE1 => ("gaming", "ASUS Controllo Custom 0xE1", "ASUS Custom Control 0xE1"),
                    0xE2 => ("gaming", "ASUS Overdrive / TraceFree", "ASUS Overdrive / TraceFree"),
                    0xE8 => ("gaming", "ASUS Shadow Boost", "ASUS Shadow Boost"),
                    0xEA => ("gaming", "ASUS Blue Light Filter", "ASUS Blue Light Filter"),
                    _ => return None,
                }
            } else if is_aoc {
                match code {
                    0xE1 => ("gaming", "AOC LowBlue Mode", "AOC LowBlue Mode"),
                    0xE2 => ("gaming", "AOC Overdrive", "AOC Overdrive"),
                    0xE6 => ("gaming", "AOC Shadow Control", "AOC Shadow Control"),
                    _ => return None,
                }
            } else if is_philips {
                match code {
                    0xE1 => ("gaming", "Philips LowBlue Mode", "Philips LowBlue Mode"),
                    0xE2 => ("gaming", "Philips SmartResponse", "Philips SmartResponse"),
                    0xE3 => ("gaming", "Philips SmartContrast", "Philips SmartContrast"),
                    _ => return None,
                }
            } else if is_ktc {
                match code {
                    0xE1 => ("gaming", "KTC Low Blue Light", "KTC Low Blue Light"),
                    0xE2 => ("gaming", "KTC Overdrive / Risposta", "KTC Overdrive"),
                    0xE4 => ("gaming", "KTC Black Equalizer", "KTC Black Equalizer"),
                    _ => return None,
                }
            } else {
                return None;
            }
        }

        // Audio & Input
        0x60 => ("audio_input", "Sorgente Input Video", "Input Source"),
        0x62 => ("audio_input", "Volume Altoparlanti", "Speaker Volume"),
        0x8D => ("audio_input", "Muto Audio", "Audio Mute"),
        0x8F => ("audio_input", "Alti Audio (Treble)", "Audio Treble"),
        0x91 => ("audio_input", "Bassi Audio (Bass)", "Audio Bass"),

        // Sistema & Energia
        0xD6 => ("system", "Stato Alimentazione (DPMS)", "Power Mode (DPMS)"),
        0xCA => ("system", "Blocco / Controllo OSD", "OSD Control / Lock"),
        0xCC => ("system", "Lingua OSD", "OSD Language"),
        0xDF => ("system", "Versione VCP / MCCS", "MCCS Version"),
        0xAC => ("system", "Frequenza Orizzontale", "Horizontal Frequency"),
        0xAE => ("system", "Frequenza Verticale", "Vertical Frequency"),
        0x04 => ("system", "Ripristino di Fabbrica", "Factory Reset"),
        0x05 => ("system", "Reset Luminosità & Contrasto", "Reset Brightness/Contrast"),
        0x06 => ("system", "Reset Geometria", "Reset Geometry"),
        0x08 => ("system", "Reset Colori", "Reset Color"),
        _ => return None,
    })
}

fn default_options_for_code(code: u8, brand: &str) -> Vec<VcpValueOption> {
    let b = brand.to_uppercase();
    match code {
        0x14 => vec![
            VcpValueOption { value: 1, label: "sRGB".to_string() },
            VcpValueOption { value: 4, label: "5000K".to_string() },
            VcpValueOption { value: 5, label: "6500K".to_string() },
            VcpValueOption { value: 6, label: "7500K".to_string() },
            VcpValueOption { value: 8, label: "9300K".to_string() },
            VcpValueOption { value: 11, label: "User".to_string() },
        ],
        0x60 => vec![
            VcpValueOption { value: 15, label: "DisplayPort 1".to_string() },
            VcpValueOption { value: 16, label: "DisplayPort 2".to_string() },
            VcpValueOption { value: 17, label: "HDMI 1".to_string() },
            VcpValueOption { value: 18, label: "HDMI 2".to_string() },
            VcpValueOption { value: 27, label: "USB-C".to_string() },
        ],
        0x8D => vec![
            VcpValueOption { value: 1, label: "Muto (Mute)".to_string() },
            VcpValueOption { value: 2, label: "Audio Attivo".to_string() },
        ],
        0xD6 => vec![
            VcpValueOption { value: 1, label: "Acceso (On)".to_string() },
            VcpValueOption { value: 2, label: "Standby".to_string() },
            VcpValueOption { value: 4, label: "Spento (Off)".to_string() },
        ],
        0xCA => vec![
            VcpValueOption { value: 1, label: "OSD Bloccato".to_string() },
            VcpValueOption { value: 2, label: "OSD Sbloccato".to_string() },
        ],
        0xDC => {
            if b.contains("ASUS") {
                vec![
                    VcpValueOption { value: 0, label: "Standard".to_string() },
                    VcpValueOption { value: 1, label: "Productivity".to_string() },
                    VcpValueOption { value: 2, label: "Mixed / Scenery".to_string() },
                    VcpValueOption { value: 3, label: "Movie / Cinema".to_string() },
                    VcpValueOption { value: 4, label: "User / Racing".to_string() },
                    VcpValueOption { value: 5, label: "Games / FPS".to_string() },
                ]
            } else if b.contains("PHILIPS") {
                vec![
                    VcpValueOption { value: 0, label: "Standard".to_string() },
                    VcpValueOption { value: 1, label: "Office".to_string() },
                    VcpValueOption { value: 2, label: "Photo".to_string() },
                    VcpValueOption { value: 3, label: "Movie".to_string() },
                    VcpValueOption { value: 5, label: "Game".to_string() },
                ]
            } else {
                vec![
                    VcpValueOption { value: 0, label: "Standard".to_string() },
                    VcpValueOption { value: 1, label: "Productivity".to_string() },
                    VcpValueOption { value: 3, label: "Movie".to_string() },
                    VcpValueOption { value: 5, label: "Gaming".to_string() },
                ]
            }
        }
        _ => Vec::new(),
    }
}

pub fn describe_vcp(
    code: FeatureCode,
    brand: &str,
    db: &Database,
    cap_values: Option<&ValueNames>,
) -> VcpMeta {
    let db_desc = db.get(code);
    let friendly = friendly_category_and_names(code, brand);

    let (category, name_it, name_en) = if let Some((cat, it, en)) = friendly {
        (cat.to_string(), it.to_string(), en.to_string())
    } else {
        let db_name = db_desc
            .and_then(|d| d.name.clone())
            .unwrap_or_else(|| format!("VCP 0x{:02X}", code));
        let cat = if (0xE0..=0xFF).contains(&code) {
            "gaming"
        } else if let Some(grp) = db_desc.and_then(|d| d.group.as_deref()) {
            let gl = grp.to_lowercase();
            if gl.contains("audio") {
                "audio_input"
            } else if gl.contains("color") {
                "color"
            } else if gl.contains("image") || gl.contains("geometry") {
                "image"
            } else {
                "system"
            }
        } else {
            "system"
        };
        (cat.to_string(), db_name.clone(), db_name)
    };

    let is_read_only = db_desc
        .map(|d| matches!(d.access, Access::ReadOnly))
        .unwrap_or(false)
        || matches!(code, 0xAC | 0xAE | 0xDF);

    let mut options: Vec<VcpValueOption> = Vec::new();
    let mut is_continuous = true;

    if let Some(vals) = cap_values {
        if !vals.is_empty() {
            is_continuous = false;
            for (&val, opt_name) in vals {
                let fallback_opts = default_options_for_code(code, brand);
                let label = opt_name
                    .clone()
                    .or_else(|| {
                        fallback_opts
                            .into_iter()
                            .find(|o| o.value == val as u32)
                            .map(|o| o.label)
                    })
                    .unwrap_or_else(|| format!("0x{:02X} ({})", val, val));
                options.push(VcpValueOption {
                    value: val as u32,
                    label,
                });
            }
        }
    }

    if options.is_empty() {
        if let Some(desc) = db_desc {
            match &desc.ty {
                ValueType::Continuous { .. } => {
                    is_continuous = true;
                }
                ValueType::NonContinuous { values, .. } => {
                    is_continuous = false;
                    let fallback_opts = default_options_for_code(code, brand);
                    if !values.is_empty() && values.len() <= 24 {
                        for (&val, opt_name) in values {
                            let label = fallback_opts
                                .iter()
                                .find(|o| o.value == val as u32)
                                .map(|o| o.label.clone())
                                .or_else(|| opt_name.clone())
                                .unwrap_or_else(|| format!("{}", val));
                            options.push(VcpValueOption {
                                value: val as u32,
                                label,
                            });
                        }
                    } else if !fallback_opts.is_empty() {
                        options = fallback_opts;
                    }
                }
                ValueType::Table { .. } | ValueType::Unknown => {
                    is_continuous = false;
                }
            }
        }
    }

    if options.is_empty() && matches!(code, 0x14 | 0x60 | 0x8D | 0xD6 | 0xCA | 0xDC) {
        is_continuous = false;
        options = default_options_for_code(code, brand);
    }

    // Override sicuro per codici notoriamente continui
    if matches!(
        code,
        0x10 | 0x12 | 0x13 | 0x16 | 0x18 | 0x1A | 0x62 | 0x6C | 0x6E | 0x70 | 0x87 | 0x8A | 0x90 | 0x92
    ) {
        is_continuous = true;
        options.clear();
    }

    VcpMeta {
        name_it,
        name_en,
        category,
        is_continuous,
        is_read_only,
        is_critical: is_critical_vcp(code),
        options,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_mccs_capabilities_and_discovers_features() {
        let sample = "(prot(monitor)type(lcd)model(VG27AQ)cmds(01 02 03 0C E3 F3)vcp(10 12 14(05 08 0B) 16 18 1A 60(0F 11 12) 62 72 87 8A 8D(01 02) D6(01 04) DC(00 02 03 05) E8 EA)mccs_ver(2.1))";
        let (caps, db) = parse_capabilities_resilient(sample);
        assert!(caps.is_some());
        let caps = caps.unwrap();
        assert!(caps.vcp_features.contains_key(&0x10));
        assert!(caps.vcp_features.contains_key(&0xDC));
        assert!(caps.vcp_features.contains_key(&0xE8));

        let meta_dc = describe_vcp(
            0xDC,
            "ASUS",
            &db,
            caps.vcp_features.get(&0xDC).map(|d| &d.values),
        );
        assert_eq!(meta_dc.category, "gaming");
        assert!(!meta_dc.is_continuous);
        assert_eq!(meta_dc.options.len(), 4);

        let meta_e8 = describe_vcp(0xE8, "ASUS", &db, None);
        assert_eq!(meta_e8.name_en, "ASUS Shadow Boost");
        assert!(is_critical_vcp(0x04));
        assert!(is_critical_vcp(0xD6));
    }

    #[test]
    fn brand_probes_include_vendor_codes_for_priority_brands() {
        for brand in ["ASUS", "AOC", "Philips", "KTC"] {
            let codes = collect_probe_codes(None, brand, brand);
            assert!(codes.contains(&0xDC));
            assert!(codes.contains(&0xE1));
            assert!(codes.contains(&0xE2));
        }
    }
}
