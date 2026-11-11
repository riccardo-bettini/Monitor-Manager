// Stato globale della UI di Monitor Manager
const state = {
    monitors: [],
    selectedMonitorId: null,
    settings: {
        theme: "system",
        layout_mode: "multi",
        hide_unsupported: false,
        sync_monitors: false,
        start_with_windows: false,
        auto_check_updates: true,
        close_to_tray: true,
    },
    latestUpdate: null,
};

// Codici VCP standard VESA MCCS
const VCP = {
    BRIGHTNESS: 0x10,
    CONTRAST: 0x12,
    COLOR_TEMP: 0x14,
    RED_GAIN: 0x16,
    GREEN_GAIN: 0x18,
    BLUE_GAIN: 0x1A,
    INPUT_SOURCE: 0x60,
    VOLUME: 0x62,
    SHARPNESS: 0x87,
    SATURATION: 0x8A,
};

const COLOR_TEMP_PRESETS = [
    { label: "5000K Caldo", val: 4 },
    { label: "6500K Standard", val: 5 },
    { label: "7500K", val: 6 },
    { label: "9300K Freddo", val: 8 },
    { label: "Utente (RGB)", val: 11 },
];

const INPUT_SOURCES = [
    { label: "DisplayPort 1", val: 15 },
    { label: "DisplayPort 2", val: 16 },
    { label: "HDMI 1", val: 17 },
    { label: "HDMI 2", val: 18 },
    { label: "USB-C", val: 27 },
];

function sendIpc(payload) {
    if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify(payload));
    }
}

// Trascinamento nativo della finestra dalla barra del titolo Liquid Glass
document.addEventListener("DOMContentLoaded", () => {
    const dragRegion = document.getElementById("titlebar-drag");
    if (dragRegion) {
        dragRegion.addEventListener("mousedown", (e) => {
            if (e.button === 0) {
                sendIpc({ cmd: "drag_window" });
            }
        });
        dragRegion.addEventListener("dblclick", () => {
            sendIpc({ cmd: "window_action", action: "maximize" });
        });
    }

    // Ascolta il cambio tema di Windows in tempo reale
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    mediaQuery.addEventListener("change", () => {
        if (state.settings.theme === "system") {
            applyEffectiveTheme("system");
        }
    });

    // Richiedi stato iniziale al motore Rust
    sendIpc({ cmd: "init" });
});

function windowAction(action) {
    sendIpc({ cmd: "window_action", action });
}

function applyEffectiveTheme(themePref) {
    let resolved = themePref;
    if (themePref === "system") {
        const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
        resolved = prefersDark ? "dark" : "light";
    }
    document.documentElement.setAttribute("data-theme", resolved);

    // Aggiorna i pulsanti segmented nel modale impostazioni
    document.querySelectorAll("#theme-segmented .seg-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-theme-opt") === themePref);
    });
}

function setThemePreference(themePref) {
    state.settings.theme = themePref;
    applyEffectiveTheme(themePref);
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function setLayoutMode(layoutMode) {
    state.settings.layout_mode = layoutMode;
    updateLayoutButtons();
    renderMonitors();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function updateLayoutButtons() {
    const mode = state.settings.layout_mode;
    document.querySelectorAll("#quick-layout-toggle .seg-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-layout") === mode);
    });
    document.querySelectorAll("#modal-layout-segmented .seg-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-layout-opt") === mode);
    });
}

function toggleSyncAll() {
    state.settings.sync_monitors = !state.settings.sync_monitors;
    updateSyncButton();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function updateSyncButton() {
    const btn = document.getElementById("sync-all-btn");
    if (btn) {
        btn.classList.toggle("active-sync", !!state.settings.sync_monitors);
    }
}

function updateBoolSetting(key, value) {
    state.settings[key] = !!value;
    if (key === "hide_unsupported") {
        renderMonitors();
    }
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function triggerRescan() {
    const btn = document.getElementById("rescan-btn");
    if (btn) btn.classList.add("spinning");
    sendIpc({ cmd: "rescan_monitors" });
}

// Funzioni ricezione dati dal backend Rust
window.onBackendEvent = function (event) {
    if (!event || !event.type) return;

    switch (event.type) {
        case "init_data": {
            state.settings = event.settings || state.settings;
            if (event.version) {
                document.getElementById("version-badge").textContent = "v" + event.version;
                document.getElementById("settings-current-ver").textContent = "v" + event.version;
            }
            syncSettingsUI();
            if (event.monitors) {
                setMonitorsList(event.monitors);
            }
            break;
        }
        case "monitors_updated": {
            const btn = document.getElementById("rescan-btn");
            if (btn) btn.classList.remove("spinning");
            setMonitorsList(event.monitors || []);
            break;
        }
        case "update_check_result": {
            handleUpdateResult(event.result);
            break;
        }
        case "update_error": {
            const statusEl = document.getElementById("update-status-text");
            if (statusEl) statusEl.textContent = event.message || "Errore durante l'aggiornamento.";
            const installBtn = document.getElementById("install-update-btn");
            if (installBtn) {
                installBtn.disabled = false;
                installBtn.textContent = "Aggiorna e Riavvia ora";
            }
            break;
        }
    }
};

function syncSettingsUI() {
    applyEffectiveTheme(state.settings.theme);
    updateLayoutButtons();
    updateSyncButton();

    const hideToggle = document.getElementById("toggle-hide-unsupported");
    if (hideToggle) hideToggle.checked = !!state.settings.hide_unsupported;

    const closeTrayToggle = document.getElementById("toggle-close-to-tray");
    if (closeTrayToggle) closeTrayToggle.checked = !!state.settings.close_to_tray;

    const autostartToggle = document.getElementById("toggle-autostart");
    if (autostartToggle) autostartToggle.checked = !!state.settings.start_with_windows;

    const autoUpdatesToggle = document.getElementById("toggle-auto-updates");
    if (autoUpdatesToggle) autoUpdatesToggle.checked = !!state.settings.auto_check_updates;
}

function setMonitorsList(monitors) {
    state.monitors = monitors;
    if (state.monitors.length > 0) {
        const exists = state.monitors.some((m) => m.id === state.selectedMonitorId);
        if (!exists) {
            state.selectedMonitorId = state.monitors[0].id;
        }
    }
    const loadingEl = document.getElementById("loading-state");
    const containerEl = document.getElementById("monitors-container");
    if (loadingEl) loadingEl.classList.add("hidden");
    if (containerEl) containerEl.classList.remove("hidden");

    renderMonitors();
}

function selectMonitorTab(monitorId) {
    state.selectedMonitorId = monitorId;
    renderMonitors();
}

function renderMonitors() {
    const switcherBar = document.getElementById("monitor-switcher-bar");
    const container = document.getElementById("monitors-container");
    if (!container || !switcherBar) return;

    const isSingleMode = state.settings.layout_mode === "single";
    container.className = "monitors-container " + (isSingleMode ? "layout-single" : "layout-multi");

    if (state.monitors.length === 0) {
        switcherBar.classList.add("hidden");
        container.innerHTML = `
            <div class="loading-glass-card">
                <h2>Nessun monitor esterno DDC/CI rilevato</h2>
                <p>Verifica che il cavo DisplayPort/HDMI sia collegato e che l'opzione DDC/CI sia attiva nell'OSD del monitor.</p>
                <button class="glass-btn primary-glass-btn" onclick="triggerRescan()">Riprova scansione</button>
            </div>
        `;
        return;
    }

    // Barra selettore superiore in modalità Single
    if (isSingleMode) {
        switcherBar.classList.remove("hidden");
        switcherBar.innerHTML = state.monitors
            .map((m) => {
                const active = m.id === state.selectedMonitorId ? "active" : "";
                return `
                    <button class="switcher-tab ${active}" onclick="selectMonitorTab(${m.id})">
                        <div class="monitor-avatar" style="width:34px;height:34px;border-radius:11px;">
                            ${getMonitorSvgIcon()}
                        </div>
                        <div>
                            <span class="switcher-tab-title">${escapeHtml(m.name)}</span>
                            <span class="switcher-tab-sub">${escapeHtml(m.brand)} • ${escapeHtml(m.resolution)}</span>
                        </div>
                    </button>
                `;
            })
            .join("");
    } else {
        switcherBar.classList.add("hidden");
    }

    const visibleMonitors = isSingleMode
        ? state.monitors.filter((m) => m.id === state.selectedMonitorId)
        : state.monitors;

    container.innerHTML = visibleMonitors.map((m) => buildMonitorCardHtml(m)).join("");
}

function buildMonitorCardHtml(m) {
    const hideUnsup = !!state.settings.hide_unsupported;

    const imageSliders = [
        renderSliderRow(m.id, "Luminosità", m.brightness, VCP.BRIGHTNESS, iconSun(), hideUnsup),
        renderSliderRow(m.id, "Contrasto", m.contrast, VCP.CONTRAST, iconContrast(), hideUnsup),
        renderSliderRow(m.id, "Saturazione", m.saturation, VCP.SATURATION, iconDroplet(), hideUnsup),
        renderSliderRow(m.id, "Nitidezza (Sharpness)", m.sharpness, VCP.SHARPNESS, iconSparkle(), hideUnsup),
    ].join("");

    const colorSliders = [
        renderSliderRow(m.id, "Guadagno Rosso (R)", m.red_gain, VCP.RED_GAIN, iconColorDot("#f87171"), hideUnsup),
        renderSliderRow(m.id, "Guadagno Verde (G)", m.green_gain, VCP.GREEN_GAIN, iconColorDot("#4ade80"), hideUnsup),
        renderSliderRow(m.id, "Guadagno Blu (B)", m.blue_gain, VCP.BLUE_GAIN, iconColorDot("#60a5fa"), hideUnsup),
        renderColorTempRow(m.id, m.color_temp, hideUnsup),
    ].join("");

    const audioInputBlock = [
        renderSliderRow(m.id, "Volume Altoparlanti", m.volume, VCP.VOLUME, iconVolume(), hideUnsup),
        renderInputSourceRow(m.id, m.input_source, hideUnsup),
    ].join("");

    return `
        <section class="monitor-glass-card" data-monitor-id="${m.id}">
            <div class="monitor-card-header">
                <div class="monitor-identity">
                    <div class="monitor-avatar">
                        ${getMonitorSvgIcon()}
                    </div>
                    <div class="monitor-title-block">
                        <h3>${escapeHtml(m.name)}</h3>
                        <div class="monitor-meta-pills">
                            <span class="meta-pill brand-pill">${escapeHtml(m.brand)}</span>
                            <span class="meta-pill">${escapeHtml(m.resolution)}</span>
                            ${m.is_primary ? '<span class="meta-pill primary-pill">Principale</span>' : ""}
                            <span class="meta-pill ${m.ddc_supported ? "ddc-ok" : ""}">
                                ${m.ddc_supported ? "● DDC/CI Attivo" : "○ DDC Limitato"}
                            </span>
                        </div>
                    </div>
                </div>
            </div>

            <div class="controls-sections-grid">
                <div class="control-section-box">
                    <div class="section-header-title">
                        <span>Immagine & Nitidezza</span>
                        <span>VCP Base</span>
                    </div>
                    ${imageSliders}
                </div>

                <div class="control-section-box">
                    <div class="section-header-title">
                        <span>Bilanciamento Colore RGB & Temperatura</span>
                        <span>Calibrazione</span>
                    </div>
                    ${colorSliders}
                </div>

                <div class="control-section-box">
                    <div class="section-header-title">
                        <span>Audio & Sorgente Video</span>
                        <span>Hardware I/O</span>
                    </div>
                    ${audioInputBlock}
                </div>
            </div>
        </section>
    `;
}

function renderSliderRow(monitorId, label, feature, vcpCode, svgIcon, hideUnsupported) {
    if (!feature) return "";
    if (!feature.supported && hideUnsupported) return "";

    const maxVal = feature.max && feature.max > 0 ? feature.max : 100;
    const curVal = Math.min(feature.current ?? 50, maxVal);
    const unsupClass = feature.supported ? "" : "unsupported-feature";
    const unsupBadge = feature.supported
        ? ""
        : `<span class="unsupported-tag" title="Il firmware di questo monitor non ha dichiarato supporto attivo per questo codice VCP">Non esposto da OSD</span>`;

    return `
        <div class="vcp-control-row ${unsupClass}">
            <div class="vcp-row-top">
                <div class="vcp-label-wrap">
                    ${svgIcon}
                    <span>${label}</span>
                    ${unsupBadge}
                </div>
                <span class="vcp-value-badge" id="val-${monitorId}-${vcpCode}">${curVal}${maxVal === 100 ? "%" : " / " + maxVal}</span>
            </div>
            <div class="slider-track-container">
                <input
                    type="range"
                    class="liquid-slider"
                    min="0"
                    max="${maxVal}"
                    value="${curVal}"
                    data-vcp="${vcpCode}"
                    data-max="${maxVal}"
                    oninput="onSliderInput(${monitorId}, ${vcpCode}, this.value, ${maxVal})"
                />
            </div>
        </div>
    `;
}

function renderColorTempRow(monitorId, feature, hideUnsupported) {
    if (!feature) return "";
    if (!feature.supported && hideUnsupported) return "";

    const unsupClass = feature.supported ? "" : "unsupported-feature";
    const chips = COLOR_TEMP_PRESETS.map((p) => {
        const active = feature.current === p.val ? "active" : "";
        return `<button class="glass-chip ${active}" data-chip-vcp="${VCP.COLOR_TEMP}" data-chip-val="${p.val}" onclick="onChipSelect(${monitorId}, ${VCP.COLOR_TEMP}, ${p.val})">${p.label}</button>`;
    }).join("");

    return `
        <div class="vcp-control-row ${unsupClass}">
            <div class="vcp-row-top">
                <div class="vcp-label-wrap">
                    ${iconThermometer()}
                    <span>Temperatura Colore</span>
                    ${feature.supported ? "" : '<span class="unsupported-tag">Non esposto da OSD</span>'}
                </div>
            </div>
            <div class="chip-selector-group">
                ${chips}
            </div>
        </div>
    `;
}

function renderInputSourceRow(monitorId, feature, hideUnsupported) {
    if (!feature) return "";
    if (!feature.supported && hideUnsupported) return "";

    const unsupClass = feature.supported ? "" : "unsupported-feature";
    const chips = INPUT_SOURCES.map((src) => {
        const active = feature.current === src.val ? "active" : "";
        return `<button class="glass-chip ${active}" data-chip-vcp="${VCP.INPUT_SOURCE}" data-chip-val="${src.val}" onclick="onChipSelect(${monitorId}, ${VCP.INPUT_SOURCE}, ${src.val})">${src.label}</button>`;
    }).join("");

    return `
        <div class="vcp-control-row ${unsupClass}">
            <div class="vcp-row-top">
                <div class="vcp-label-wrap">
                    ${iconCable()}
                    <span>Sorgente Input Video</span>
                    ${feature.supported ? "" : '<span class="unsupported-tag">Non esposto da OSD</span>'}
                </div>
            </div>
            <div class="chip-selector-group">
                ${chips}
            </div>
        </div>
    `;
}

function onSliderInput(monitorId, vcpCode, rawValue, maxVal) {
    const val = parseInt(rawValue, 10) || 0;
    const syncAll = !!state.settings.sync_monitors;

    // Aggiorna lo stato locale e i badge numerici all'istante (zero-lag UI)
    state.monitors.forEach((m) => {
        if (syncAll || m.id === monitorId) {
            updateLocalMonitorFeature(m, vcpCode, val);
            const badge = document.getElementById(`val-${m.id}-${vcpCode}`);
            if (badge) {
                const mMax = maxVal || 100;
                badge.textContent = `${val}${mMax === 100 ? "%" : " / " + mMax}`;
            }
            if (syncAll && m.id !== monitorId) {
                const card = document.querySelector(`.monitor-glass-card[data-monitor-id="${m.id}"]`);
                if (card) {
                    const slider = card.querySelector(`input.liquid-slider[data-vcp="${vcpCode}"]`);
                    if (slider) slider.value = val;
                }
            }
        }
    });

    // Invia al worker DDC/CI con coalescing atomico
    sendIpc({
        cmd: "set_vcp",
        monitor_id: monitorId,
        vcp_code: vcpCode,
        value: val,
        sync_all: syncAll,
    });
}

function onChipSelect(monitorId, vcpCode, value) {
    const syncAll = !!state.settings.sync_monitors;
    state.monitors.forEach((m) => {
        if (syncAll || m.id === monitorId) {
            updateLocalMonitorFeature(m, vcpCode, value);
            const card = document.querySelector(`.monitor-glass-card[data-monitor-id="${m.id}"]`);
            if (card) {
                card.querySelectorAll(`button.glass-chip[data-chip-vcp="${vcpCode}"]`).forEach((btn) => {
                    btn.classList.toggle("active", parseInt(btn.getAttribute("data-chip-val"), 10) === value);
                });
            }
        }
    });

    sendIpc({
        cmd: "set_vcp",
        monitor_id: monitorId,
        vcp_code: vcpCode,
        value,
        sync_all: syncAll,
    });
}

function updateLocalMonitorFeature(monitor, vcpCode, val) {
    switch (vcpCode) {
        case VCP.BRIGHTNESS: monitor.brightness.current = val; break;
        case VCP.CONTRAST: monitor.contrast.current = val; break;
        case VCP.SATURATION: monitor.saturation.current = val; break;
        case VCP.SHARPNESS: monitor.sharpness.current = val; break;
        case VCP.RED_GAIN: monitor.red_gain.current = val; break;
        case VCP.GREEN_GAIN: monitor.green_gain.current = val; break;
        case VCP.BLUE_GAIN: monitor.blue_gain.current = val; break;
        case VCP.COLOR_TEMP: monitor.color_temp.current = val; break;
        case VCP.VOLUME: monitor.volume.current = val; break;
        case VCP.INPUT_SOURCE: monitor.input_source.current = val; break;
    }
}

// Gestione Modale Impostazioni
function openSettingsModal() {
    document.getElementById("settings-modal").classList.remove("hidden");
}

function closeSettingsModal() {
    document.getElementById("settings-modal").classList.add("hidden");
}

function handleBackdropClick(e) {
    if (e.target.id === "settings-modal") {
        closeSettingsModal();
    }
}

// Gestione Aggiornamenti GitHub
function checkUpdatesManual() {
    const statusText = document.getElementById("update-status-text");
    const btn = document.getElementById("manual-check-btn");
    if (statusText) statusText.textContent = "Controllo release su GitHub in corso...";
    if (btn) btn.disabled = true;
    sendIpc({ cmd: "check_updates" });
}

function handleUpdateResult(res) {
    state.latestUpdate = res;
    const statusText = document.getElementById("update-status-text");
    const checkBtn = document.getElementById("manual-check-btn");
    const modalInstallBtn = document.getElementById("modal-install-btn");

    if (checkBtn) checkBtn.disabled = false;
    if (statusText && res.status_message) {
        statusText.textContent = res.status_message;
    }

    if (res.update_available) {
        const banner = document.getElementById("update-banner");
        const title = document.getElementById("update-banner-title");
        const desc = document.getElementById("update-banner-desc");
        if (title) title.textContent = `Nuovo aggiornamento disponibile: v${res.latest_version}`;
        if (desc) {
            desc.textContent = res.release_notes
                ? res.release_notes.slice(0, 120)
                : "Nuova release pronta per il download automatico da GitHub.";
        }
        if (banner) banner.classList.remove("hidden");
        if (modalInstallBtn) modalInstallBtn.classList.remove("hidden");
    } else {
        if (modalInstallBtn) modalInstallBtn.classList.add("hidden");
    }
}

function dismissUpdateBanner() {
    document.getElementById("update-banner").classList.add("hidden");
}

function installUpdateNow() {
    if (!state.latestUpdate) return;
    const installBtn = document.getElementById("install-update-btn");
    if (installBtn) {
        installBtn.disabled = true;
        installBtn.textContent = "Download e aggiornamento in corso...";
    }
    sendIpc({
        cmd: "install_update",
        download_url: state.latestUpdate.download_url,
        html_url: state.latestUpdate.html_url,
    });
}

function escapeHtml(str) {
    return String(str ?? "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

// Icone SVG vettoriali nitide
function getMonitorSvgIcon() {
    return `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="3"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>`;
}
function iconSun() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"></path></svg>`;
}
function iconContrast() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"></circle><path d="M12 3v18a9 9 0 0 0 0-18z" fill="currentColor"></path></svg>`;
}
function iconDroplet() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2.69l5.66 5.66a8 8 0 1 1-11.31 0z"></path></svg>`;
}
function iconSparkle() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`;
}
function iconColorDot(hex) {
    return `<span style="display:inline-block;width:11px;height:11px;border-radius:50%;background:${hex};box-shadow:0 0 8px ${hex};"></span>`;
}
function iconThermometer() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 14.76V3.5a2.5 2.5 0 0 0-5 0v11.26a4.5 4.5 0 1 0 5 0z"></path></svg>`;
}
function iconVolume() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14"></path></svg>`;
}
function iconCable() {
    return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="7" width="20" height="10" rx="2"></rect><line x1="6" y1="12" x2="6.01" y2="12"></line><line x1="10" y1="12" x2="10.01" y2="12"></line><line x1="14" y1="12" x2="18" y2="12"></line></svg>`;
}
