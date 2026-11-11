// Stato globale di Monitor Manager v1.01 (Glassy Dashboard V24)
const state = {
    monitors: [],
    selectedMonitorId: null,
    activeView: "dashboard", // "dashboard" | "multi" | "color" | "io" | "settings"
    activeSettingsCat: "theme", // "theme" | "display" | "system" | "updates"
    searchQuery: "",
    settings: {
        theme: "system",
        layout_mode: "single",
        hide_unsupported: false,
        sync_monitors: false,
        start_with_windows: false,
        auto_check_updates: true,
        close_to_tray: true,
        accent_palette: "coral",
        custom_accent_hex: "#e15b48",
        glass_blur: 44,
        blob_intensity: 85,
        animate_blobs: true,
    },
    latestUpdate: null,
};

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

document.addEventListener("DOMContentLoaded", () => {
    // Trascinamento finestra dall'header principale o dall'header sidebar
    ["titlebar-drag", "sidebar-drag"].forEach((id) => {
        const el = document.getElementById(id);
        if (el) {
            el.addEventListener("mousedown", (e) => {
                if (
                    e.button === 0 &&
                    !e.target.closest("button") &&
                    !e.target.closest("input")
                ) {
                    sendIpc({ cmd: "drag_window" });
                }
            });
            el.addEventListener("dblclick", (e) => {
                if (!e.target.closest("button")) {
                    sendIpc({ cmd: "window_action", action: "maximize" });
                }
            });
        }
    });

    // Scorciatoia Ctrl+K / Cmd+K per la ricerca rapida nella Sidebar
    document.addEventListener("keydown", (e) => {
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
            e.preventDefault();
            const sb = document.getElementById("sidebar");
            if (sb && sb.classList.contains("collapsed")) {
                sb.classList.remove("collapsed");
            }
            const inp = document.getElementById("quick-search-input");
            if (inp) inp.focus();
        }
    });

    // Sincronizzazione live col tema di Windows
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    mediaQuery.addEventListener("change", () => {
        if (state.settings.theme === "system") {
            applyVisualThemeAndPalette();
        }
    });

    updateGreetingHeader();
    sendIpc({ cmd: "init" });
});

function windowAction(action) {
    sendIpc({ cmd: "window_action", action });
}

function toggleSidebarCollapse() {
    const sb = document.getElementById("sidebar");
    if (sb) {
        sb.classList.toggle("collapsed");
    }
}

function updateGreetingHeader() {
    const now = new Date();
    const hour = now.getHours();
    let greet = "Buon pomeriggio";
    if (hour >= 5 && hour < 12) greet = "Buongiorno";
    else if (hour >= 18 || hour < 5) greet = "Buonasera";

    const activeMon = getSelectedMonitor();
    const titleEl = document.getElementById("topbar-greeting");
    const subEl = document.getElementById("topbar-subtitle");

    if (state.activeView === "settings") {
        if (titleEl) titleEl.textContent = "Impostazioni & Personalizzazione";
        if (subEl) subEl.textContent = "Configura Tema e Colori Liquid Glass, visualizzazione monitor e aggiornamenti";
        return;
    }

    if (titleEl) {
        titleEl.textContent = activeMon
            ? `${greet}, ${activeMon.name}`
            : `${greet}, Monitor Studio`;
    }

    if (subEl) {
        const giorni = ["Domenica", "Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato"];
        const mesi = ["Gennaio", "Febbraio", "Marzo", "Aprile", "Maggio", "Giugno", "Luglio", "Agosto", "Settembre", "Ottobre", "Novembre", "Dicembre"];
        const dateStr = `${giorni[now.getDay()]}, ${now.getDate()} ${mesi[now.getMonth()]}`;
        const count = state.monitors.length;
        subEl.textContent = `${dateStr} • ${count} monitor collegat${count === 1 ? "o" : "i"} e pront${count === 1 ? "o" : "i"} al controllo DDC/CI`;
    }
}

function getSelectedMonitor() {
    if (state.monitors.length === 0) return null;
    return (
        state.monitors.find((m) => m.id === state.selectedMonitorId) ||
        state.monitors[0]
    );
}

// ============================================================================
// GESTIONE TEMA, PALETTE CROMATICHE & PERSONALIZZAZIONE LIQUID GLASS
// ============================================================================
function applyVisualThemeAndPalette() {
    const s = state.settings;

    // 1. Tema Chiaro / Scuro / Sistema
    let resolvedTheme = s.theme || "system";
    if (resolvedTheme === "system") {
        resolvedTheme = window.matchMedia("(prefers-color-scheme: dark)").matches
            ? "dark"
            : "light";
    }
    document.documentElement.setAttribute("data-theme", resolvedTheme);

    // 2. Palette Colore Accento
    const palette = s.accent_palette || "coral";
    document.documentElement.setAttribute("data-palette", palette);

    const rootStyle = document.documentElement.style;
    if (palette === "custom" && s.custom_accent_hex) {
        const hex = s.custom_accent_hex;
        rootStyle.setProperty("--accent-main", hex);
        rootStyle.setProperty(
            "--hero-gradient",
            `linear-gradient(135deg, ${hex}e0 0%, ${hex}b8 100%)`
        );
        rootStyle.setProperty(
            "--stat-accent-gradient",
            `linear-gradient(145deg, ${hex}d8 0%, ${hex}b0 100%)`
        );
        rootStyle.setProperty("--blob-c1", `${hex}66`);
    } else {
        rootStyle.removeProperty("--accent-main");
        rootStyle.removeProperty("--hero-gradient");
        rootStyle.removeProperty("--stat-accent-gradient");
        rootStyle.removeProperty("--blob-c1");
    }

    // 3. Intensità Blur Vetro e Macchie di Colore
    const blurPx = Math.max(16, Math.min(64, s.glass_blur ?? 44));
    const blobOp = Math.max(0, Math.min(100, s.blob_intensity ?? 85)) / 100;
    rootStyle.setProperty("--glass-blur-px", `${blurPx}px`);
    rootStyle.setProperty("--blob-opacity", String(blobOp));

    const canvas = document.getElementById("ambient-canvas");
    if (canvas) {
        canvas.classList.toggle("paused", !s.animate_blobs);
    }

    // Aggiorna controlli attivi nella sezione Tema e Colori
    document.querySelectorAll(".theme-mode-option").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-theme-opt") === s.theme);
    });
    document.querySelectorAll(".palette-swatch-card").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-palette-opt") === palette);
    });
}

function setThemePreference(themePref) {
    state.settings.theme = themePref;
    applyVisualThemeAndPalette();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function setAccentPalette(palette) {
    state.settings.accent_palette = palette;
    applyVisualThemeAndPalette();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function setCustomAccentHex(hexValue) {
    state.settings.accent_palette = "custom";
    state.settings.custom_accent_hex = hexValue;
    const lbl = document.getElementById("custom-hex-label");
    if (lbl) lbl.textContent = hexValue.toUpperCase();
    applyVisualThemeAndPalette();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function updateVisualSlider(key, rawValue) {
    const val = parseInt(rawValue, 10) || 0;
    state.settings[key] = val;
    if (key === "glass_blur") {
        const b = document.getElementById("blur-val-badge");
        if (b) b.textContent = `${val} px`;
    } else if (key === "blob_intensity") {
        const b = document.getElementById("blob-val-badge");
        if (b) b.textContent = `${val}%`;
    }
    applyVisualThemeAndPalette();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function setLayoutMode(layoutMode) {
    state.settings.layout_mode = layoutMode;
    document.querySelectorAll("#modal-layout-segmented .seg-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-layout-opt") === layoutMode);
    });
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function toggleSyncAll() {
    state.settings.sync_monitors = !state.settings.sync_monitors;
    syncSettingsControls();
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function updateBoolSetting(key, value) {
    state.settings[key] = !!value;
    syncSettingsControls();
    if (key === "hide_unsupported") {
        renderActiveView();
    }
    sendIpc({ cmd: "save_settings", settings: state.settings });
}

function syncSettingsControls() {
    applyVisualThemeAndPalette();

    const s = state.settings;
    const syncBtn = document.getElementById("sync-all-btn");
    if (syncBtn) {
        syncBtn.classList.toggle("active-sync", !!s.sync_monitors);
        syncBtn.innerHTML = s.sync_monitors
            ? "<span>✓ Sync Tutti Attivo</span>"
            : "<span>+ Sync Tutti i Monitor</span>";
    }

    const setCheck = (id, val) => {
        const el = document.getElementById(id);
        if (el) el.checked = !!val;
    };
    setCheck("toggle-animate-blobs", s.animate_blobs);
    setCheck("toggle-sync-monitors", s.sync_monitors);
    setCheck("toggle-hide-unsupported", s.hide_unsupported);
    setCheck("toggle-close-to-tray", s.close_to_tray);
    setCheck("toggle-autostart", s.start_with_windows);
    setCheck("toggle-auto-updates", s.auto_check_updates);

    const blurSlider = document.getElementById("slider-glass-blur");
    if (blurSlider) blurSlider.value = s.glass_blur ?? 44;
    const blurBadge = document.getElementById("blur-val-badge");
    if (blurBadge) blurBadge.textContent = `${s.glass_blur ?? 44} px`;

    const blobSlider = document.getElementById("slider-blob-intensity");
    if (blobSlider) blobSlider.value = s.blob_intensity ?? 85;
    const blobBadge = document.getElementById("blob-val-badge");
    if (blobBadge) blobBadge.textContent = `${s.blob_intensity ?? 85}%`;

    const colorInp = document.getElementById("custom-color-input");
    if (colorInp && s.custom_accent_hex) colorInp.value = s.custom_accent_hex;
    const hexLbl = document.getElementById("custom-hex-label");
    if (hexLbl && s.custom_accent_hex) hexLbl.textContent = s.custom_accent_hex.toUpperCase();

    document.querySelectorAll("#modal-layout-segmented .seg-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.getAttribute("data-layout-opt") === s.layout_mode);
    });
}

// ============================================================================
// NAVIGAZIONE TRA VISTE E MONITOR NELLA SIDEBAR
// ============================================================================
function switchMainView(viewName, settingsCat) {
    state.activeView = viewName;
    if (settingsCat) {
        state.activeSettingsCat = settingsCat;
    }

    document.querySelectorAll(".sidebar-nav .nav-item[data-nav]").forEach((btn) => {
        const nav = btn.getAttribute("data-nav");
        const isActive =
            nav === viewName ||
            (viewName === "settings" &&
                ((state.activeSettingsCat === "updates" && nav === "updates") ||
                    (state.activeSettingsCat !== "updates" && nav === "settings")));
        btn.classList.toggle("active", isActive);
    });

    updateGreetingHeader();
    renderActiveView();
}

function switchSettingsCategory(cat) {
    state.activeSettingsCat = cat;
    document.querySelectorAll(".settings-cat-tab").forEach((tab) => {
        tab.classList.toggle("active", tab.getAttribute("data-scat") === cat);
    });
    ["theme", "display", "system", "updates"].forEach((c) => {
        const panel = document.getElementById(`scat-panel-${c}`);
        if (panel) panel.classList.toggle("hidden", c !== cat);
    });
}

function selectMonitorFromSidebar(monitorId) {
    state.selectedMonitorId = monitorId;
    if (state.activeView === "settings" || state.activeView === "multi") {
        state.activeView = "dashboard";
    }
    renderSidebarMonitors();
    updateGreetingHeader();
    renderActiveView();
}

function onQuickSearch(query) {
    state.searchQuery = (query || "").trim().toLowerCase();
    renderSidebarMonitors();
}

function renderSidebarMonitors() {
    const listEl = document.getElementById("sidebar-monitors-list");
    if (!listEl) return;

    const filtered = state.monitors.filter((m) => {
        if (!state.searchQuery) return true;
        return (
            m.name.toLowerCase().includes(state.searchQuery) ||
            m.brand.toLowerCase().includes(state.searchQuery)
        );
    });

    listEl.innerHTML = filtered
        .map((m) => {
            const isSel =
                m.id === state.selectedMonitorId &&
                state.activeView !== "multi" &&
                state.activeView !== "settings";
            return `
                <button class="nav-item ${isSel ? "active" : ""}" onclick="selectMonitorFromSidebar(${m.id})" title="${escapeHtml(m.name)} (${escapeHtml(m.resolution)})">
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="3"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
                    <span class="sidebar-text">${escapeHtml(m.name)}</span>
                    <span class="nav-badge">${m.brightness?.current ?? 50}%</span>
                </button>
            `;
        })
        .join("");
}

// ============================================================================
// RICEZIONE EVENTI DAL MOTORE RUST
// ============================================================================
window.onBackendEvent = function (event) {
    if (!event || !event.type) return;

    switch (event.type) {
        case "init_data": {
            state.settings = Object.assign(state.settings, event.settings || {});
            state.activeView =
                state.settings.layout_mode === "multi" ? "multi" : "dashboard";

            if (event.version) {
                const sv = document.getElementById("sidebar-ver");
                if (sv) sv.textContent = "v" + event.version;
                const scv = document.getElementById("settings-current-ver");
                if (scv) scv.textContent = "v" + event.version;
            }

            syncSettingsControls();
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

function setMonitorsList(monitors) {
    state.monitors = monitors;
    if (state.monitors.length > 0) {
        const exists = state.monitors.some((m) => m.id === state.selectedMonitorId);
        if (!exists) {
            state.selectedMonitorId = state.monitors[0].id;
        }
    }

    const loadingEl = document.getElementById("loading-state");
    if (loadingEl) loadingEl.classList.add("hidden");

    renderSidebarMonitors();
    updateGreetingHeader();
    renderActiveView();
}

function triggerRescan() {
    const btn = document.getElementById("rescan-btn");
    if (btn) btn.classList.add("spinning");
    sendIpc({ cmd: "rescan_monitors" });
}

// ============================================================================
// RENDERING DELLA VISTA ATTIVA
// ============================================================================
function renderActiveView() {
    const viewDash = document.getElementById("view-dashboard");
    const viewMulti = document.getElementById("view-multi");
    const viewSettings = document.getElementById("view-settings");

    if (!viewDash || !viewMulti || !viewSettings) return;

    viewDash.classList.add("hidden");
    viewMulti.classList.add("hidden");
    viewSettings.classList.add("hidden");

    if (state.activeView === "settings") {
        viewSettings.classList.remove("hidden");
        switchSettingsCategory(state.activeSettingsCat || "theme");
        return;
    }

    if (state.monitors.length === 0) {
        viewDash.classList.remove("hidden");
        viewDash.innerHTML = `
            <div class="loading-glass-state">
                <h2>Nessun monitor DDC/CI rilevato</h2>
                <p>Assicurati che il cavo DisplayPort/HDMI sia collegato e che il supporto DDC/CI sia attivo nell'OSD del monitor.</p>
                <button class="pill-btn light-pill-btn" onclick="triggerRescan()">Riprova scansione</button>
            </div>
        `;
        return;
    }

    if (state.activeView === "multi") {
        viewMulti.classList.remove("hidden");
        renderMultiMonitorsView(viewMulti);
    } else {
        viewDash.classList.remove("hidden");
        renderGlassyDashboardV24(viewDash);
    }
}

// ============================================================================
// RENDERING GLASSY DASHBOARD V24 (Hero Card + Side Widget + 4 Stat Cards + Quick Pills)
// ============================================================================
function renderGlassyDashboardV24(container) {
    const m = getSelectedMonitor();
    if (!m) return;

    const hideUnsup = !!state.settings.hide_unsupported;
    const lum = m.brightness?.current ?? 50;
    const con = m.contrast?.current ?? 50;
    const shp = m.sharpness?.current ?? 50;
    const sat = m.saturation?.current ?? 50;

    // Circonferenza per r=56 è 2 * PI * 56 = 351.86
    const circumference = 351.86;
    const dashOffset = circumference - (Math.min(100, Math.max(0, lum)) / 100) * circumference;

    // Costruzione calendario settimanale nel Side Widget
    const now = new Date();
    const dayNum = now.getDate();
    const dayNamesFull = ["Domenica", "Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato"];
    const monthNames = ["Gennaio", "Febbraio", "Marzo", "Aprile", "Maggio", "Giugno", "Luglio", "Agosto", "Settembre", "Ottobre", "Novembre", "Dicembre"];
    const shortDays = ["L", "M", "M", "G", "V", "S", "D"];
    const currentDowMonday0 = (now.getDay() + 6) % 7;

    let weekStripHtml = "";
    for (let i = 0; i < 7; i++) {
        const d = new Date(now);
        d.setDate(now.getDate() - currentDowMonday0 + i);
        const isToday = i === currentDowMonday0;
        weekStripHtml += `
            <div class="cal-day-pill ${isToday ? "today" : ""}">
                <span>${shortDays[i]}</span>
                <b>${d.getDate()}</b>
            </div>
        `;
    }

    const activeInputLabel =
        INPUT_SOURCES.find((s) => s.val === m.input_source?.current)?.label ||
        "Ingresso Attivo";
    const activeTempLabel =
        COLOR_TEMP_PRESETS.find((t) => t.val === m.color_temp?.current)?.label ||
        "Calibrazione Attiva";

    const showColorFocus = state.activeView === "color";
    const showIoFocus = state.activeView === "io";

    const heroAndStatsHtml =
        showColorFocus || showIoFocus
            ? ""
            : `
        <!-- RIGA SUPERIORE: HERO CARD + SIDE WIDGET -->
        <div class="hero-and-widget-grid">
            <div class="hero-glass-card">
                <div class="hero-left">
                    <div class="hero-status-pill">
                        <span>● ${m.ddc_supported ? "DDC/CI Hardware Attivo" : "Modalità Standard"} • ${escapeHtml(m.brand)}</span>
                    </div>
                    <h2>Il tuo <em>${escapeHtml(m.name)}</em> è calibrato in tempo reale</h2>
                    <p class="hero-desc">
                        Controllo diretto a zero latenza sul bus I²C del monitor. Tutte le regolazioni vengono applicate istantaneamente all'hardware.
                    </p>
                    <div class="hero-mini-metrics">
                        <div class="hero-metric-col">
                            <strong id="hero-metric-lum">${lum}%</strong>
                            <span>Luminosità</span>
                        </div>
                        <div class="hero-metric-col">
                            <strong id="hero-metric-con">${con}%</strong>
                            <span>Contrasto</span>
                        </div>
                        <div class="hero-metric-col">
                            <strong>${escapeHtml(m.resolution)}</strong>
                            <span>Risoluzione</span>
                        </div>
                    </div>
                    <div class="hero-actions-row">
                        <button class="hero-btn-white" onclick="applyQuickPreset(${m.id}, 75, 75)">
                            Preset Brillante ↗
                        </button>
                        <button class="hero-btn-outline" onclick="applyQuickPreset(${m.id}, 45, 70)">
                            + Comfort Visivo
                        </button>
                    </div>
                </div>

                <!-- Gauge Circolare Animato con Nodi Orbitali -->
                <div class="hero-gauge-wrap">
                    <svg class="hero-gauge-svg" viewBox="0 0 140 140">
                        <circle class="gauge-bg-ring" cx="70" cy="70" r="56"></circle>
                        <circle
                            class="gauge-fg-ring"
                            id="hero-gauge-circle"
                            cx="70"
                            cy="70"
                            r="56"
                            stroke-dasharray="${circumference}"
                            stroke-dashoffset="${dashOffset}"
                        ></circle>
                    </svg>
                    <div class="gauge-center-label">
                        <strong id="hero-gauge-val">${lum}%</strong>
                        <span>luminosità</span>
                    </div>
                    <div class="orbit-node orbit-top" title="Marca Monitor">${escapeHtml(m.brand.slice(0, 3).toUpperCase())}</div>
                    <div class="orbit-node orbit-right" title="Contrasto">C:${con}</div>
                    <div class="orbit-node orbit-bottom" title="RGB Gain">RGB</div>
                    <div class="orbit-node orbit-left" title="DDC/CI">VCP</div>
                </div>
            </div>

            <!-- SIDE WIDGET DESTRO (Stile Calendario & Stato Hardware) -->
            <div class="side-calendar-widget">
                <div class="widget-cal-header">
                    <div class="cal-day-block">
                        <span class="cal-big-num">${String(dayNum).padStart(2, "0")}</span>
                        <div class="cal-day-titles">
                            <strong>${dayNamesFull[now.getDay()]}</strong>
                            <span>${monthNames[now.getMonth()]} ${now.getFullYear()} • ${state.monitors.length} display</span>
                        </div>
                    </div>
                    <div class="stat-icon-box" title="Monitor Principale">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
                    </div>
                </div>

                <div class="cal-week-strip">
                    ${weekStripHtml}
                </div>

                <div class="widget-hw-events">
                    <div class="hw-event-row">
                        <span class="hw-event-time">EDID</span>
                        <div class="hw-event-info">
                            <strong>${escapeHtml(m.name)}</strong>
                            <span>${escapeHtml(m.resolution)} • ${m.is_primary ? "Display Principale" : "Display Esteso"}</span>
                        </div>
                        <span class="hw-event-tag">Live</span>
                    </div>
                    <div class="hw-event-row">
                        <span class="hw-event-time">I/O</span>
                        <div class="hw-event-info">
                            <strong>${escapeHtml(activeInputLabel)}</strong>
                            <span>Profilo: ${escapeHtml(activeTempLabel)}</span>
                        </div>
                        <span class="hw-event-tag">VCP</span>
                    </div>
                </div>
            </div>
        </div>

        <!-- RIGA CENTRALE: 4 STAT CARDS INTERATTIVE -->
        <div class="stats-cards-grid">
            ${buildInteractiveStatCard(
                m.id,
                "Luminosità",
                "Livello retroilluminazione",
                m.brightness,
                VCP.BRIGHTNESS,
                iconSun(),
                false,
                "M2 20 Q 16 8, 32 16 T 64 6"
            )}
            ${buildInteractiveStatCard(
                m.id,
                "Contrasto",
                "Gamma dinamica pannello",
                m.contrast,
                VCP.CONTRAST,
                iconContrast(),
                false,
                "M2 18 Q 18 22, 34 12 T 64 8"
            )}
            ${buildInteractiveStatCard(
                m.id,
                "Nitidezza",
                "Definizione bordi VCP",
                m.sharpness,
                VCP.SHARPNESS,
                iconSparkle(),
                true,
                "M2 10 Q 20 6, 36 16 T 64 20"
            )}
            ${buildInteractiveStatCard(
                m.id,
                "Saturazione",
                "Intensità cromatica",
                m.saturation,
                VCP.SATURATION,
                iconDroplet(),
                false,
                "M2 22 Q 22 14, 40 16 T 64 6"
            )}
        </div>

        <!-- RIGA QUICK ACTION PILLS (6 pillole rapide su 2 righe x 3 colonne) -->
        <div class="quick-pills-grid">
            <button class="quick-action-pill-card ${m.input_source?.current === 15 ? "active-pill" : ""}" onclick="onChipSelect(${m.id}, ${VCP.INPUT_SOURCE}, 15)">
                <div class="qpill-icon">${iconCable()}</div>
                <div class="qpill-text">
                    <strong>Ingresso DisplayPort 1</strong>
                    <span>Switch immediato VCP 0x60</span>
                </div>
            </button>
            <button class="quick-action-pill-card ${m.input_source?.current === 17 ? "active-pill" : ""}" onclick="onChipSelect(${m.id}, ${VCP.INPUT_SOURCE}, 17)">
                <div class="qpill-icon">${iconCable()}</div>
                <div class="qpill-text">
                    <strong>Ingresso HDMI 1</strong>
                    <span>Switch immediato VCP 0x60</span>
                </div>
            </button>
            <button class="quick-action-pill-card ${m.color_temp?.current === 5 ? "active-pill" : ""}" onclick="onChipSelect(${m.id}, ${VCP.COLOR_TEMP}, 5)">
                <div class="qpill-icon">${iconThermometer()}</div>
                <div class="qpill-text">
                    <strong>Temperatura 6500K Standard</strong>
                    <span>Bilanciamento bianco sRGB</span>
                </div>
            </button>
            <button class="quick-action-pill-card ${m.color_temp?.current === 4 ? "active-pill" : ""}" onclick="onChipSelect(${m.id}, ${VCP.COLOR_TEMP}, 4)">
                <div class="qpill-icon">${iconSun()}</div>
                <div class="qpill-text">
                    <strong>Temperatura 5000K Caldo</strong>
                    <span>Ideale per lettura e sera</span>
                </div>
            </button>
            <button class="quick-action-pill-card ${m.color_temp?.current === 8 ? "active-pill" : ""}" onclick="onChipSelect(${m.id}, ${VCP.COLOR_TEMP}, 8)">
                <div class="qpill-icon">${iconSparkle()}</div>
                <div class="qpill-text">
                    <strong>Temperatura 9300K Freddo</strong>
                    <span>Massimo contrasto gaming</span>
                </div>
            </button>
            <button class="quick-action-pill-card" onclick="resetRgbGain(${m.id})">
                <div class="qpill-icon">${iconDroplet()}</div>
                <div class="qpill-text">
                    <strong>Bilancia Canali RGB (50%)</strong>
                    <span>Allinea Rosso, Verde e Blu</span>
                </div>
            </button>
        </div>
    `;

    // Sezioni Controlli Estesi (RGB + Audio/Input)
    const colorPanelHtml = `
        <div class="glass-detail-panel">
            <div class="detail-panel-header">
                <span>Calibrazione Guadagno Colore RGB & Temperatura</span>
                <span>${escapeHtml(m.name)}</span>
            </div>
            ${renderDetailSliderRow(m.id, "Guadagno Rosso (R)", m.red_gain, VCP.RED_GAIN, iconColorDot("#ef4444"), hideUnsup)}
            ${renderDetailSliderRow(m.id, "Guadagno Verde (G)", m.green_gain, VCP.GREEN_GAIN, iconColorDot("#10b981"), hideUnsup)}
            ${renderDetailSliderRow(m.id, "Guadagno Blu (B)", m.blue_gain, VCP.BLUE_GAIN, iconColorDot("#3b82f6"), hideUnsup)}
            ${renderChipGroupRow(m.id, "Preset Temperatura Colore", m.color_temp, VCP.COLOR_TEMP, COLOR_TEMP_PRESETS, hideUnsup)}
        </div>
    `;

    const ioPanelHtml = `
        <div class="glass-detail-panel">
            <div class="detail-panel-header">
                <span>Volume Audio & Sorgenti Input Video</span>
                <span>Hardware I/O</span>
            </div>
            ${renderDetailSliderRow(m.id, "Volume Altoparlanti Monitor", m.volume, VCP.VOLUME, iconVolume(), hideUnsup)}
            ${renderChipGroupRow(m.id, "Selettore Sorgente Input Video", m.input_source, VCP.INPUT_SOURCE, INPUT_SOURCES, hideUnsup)}
            ${
                showIoFocus
                    ? renderDetailSliderRow(m.id, "Luminosità Rapida", m.brightness, VCP.BRIGHTNESS, iconSun(), hideUnsup)
                    : ""
            }
        </div>
    `;

    let bottomPanels = `${colorPanelHtml}${ioPanelHtml}`;
    if (showColorFocus) bottomPanels = colorPanelHtml;
    if (showIoFocus) bottomPanels = ioPanelHtml;

    container.innerHTML = `
        ${heroAndStatsHtml}
        <div class="extended-controls-grid">
            ${bottomPanels}
        </div>
    `;
}

function buildInteractiveStatCard(
    monitorId,
    title,
    subtitle,
    feature,
    vcpCode,
    svgIcon,
    isAccentCard,
    sparkPath
) {
    const maxVal = feature?.max && feature.max > 0 ? feature.max : 100;
    const curVal = Math.min(feature?.current ?? 50, maxVal);
    const supported = feature?.supported !== false;
    const displayVal = maxVal === 100 ? `${curVal}%` : `${curVal}/${maxVal}`;
    const pillText = supported ? `VCP 0x${vcpCode.toString(16).toUpperCase()}` : "OSD Lock";
    const strokeColor = isAccentCard ? "#ffffff" : "#10b981";

    return `
        <div class="stat-glass-card ${isAccentCard ? "accent-stat-card" : ""}" data-stat-monitor="${monitorId}">
            <div class="stat-card-top">
                <div class="stat-icon-box">${svgIcon}</div>
                <span class="stat-trend-pill">${pillText}</span>
            </div>

            <div class="stat-card-middle">
                <div class="stat-title-block">
                    <div class="stat-big-value" id="stat-val-${monitorId}-${vcpCode}">${displayVal}</div>
                    <strong>${title}</strong>
                    <span>${subtitle}</span>
                </div>
                <svg class="stat-sparkline" viewBox="0 0 68 28" fill="none">
                    <path d="${sparkPath}" stroke="${strokeColor}" stroke-width="2.2" stroke-linecap="round"/>
                </svg>
            </div>

            <div class="stat-slider-wrap">
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

function renderDetailSliderRow(monitorId, label, feature, vcpCode, iconHtml, hideUnsupported) {
    if (!feature) return "";
    if (!feature.supported && hideUnsupported) return "";

    const maxVal = feature.max && feature.max > 0 ? feature.max : 100;
    const curVal = Math.min(feature.current ?? 50, maxVal);
    const unsupClass = feature.supported ? "" : "unsupported-feature";

    return `
        <div class="vcp-control-row ${unsupClass}">
            <div class="vcp-row-top">
                <div class="vcp-label-wrap">
                    ${iconHtml}
                    <span>${label}</span>
                </div>
                <span class="vcp-value-badge" id="val-${monitorId}-${vcpCode}">
                    ${curVal}${maxVal === 100 ? "%" : " / " + maxVal}
                </span>
            </div>
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
    `;
}

function renderChipGroupRow(monitorId, label, feature, vcpCode, optionsList, hideUnsupported) {
    if (!feature) return "";
    if (!feature.supported && hideUnsupported) return "";

    const chips = optionsList
        .map((opt) => {
            const active = feature.current === opt.val ? "active" : "";
            return `<button class="glass-chip ${active}" data-chip-vcp="${vcpCode}" data-chip-val="${opt.val}" onclick="onChipSelect(${monitorId}, ${vcpCode}, ${opt.val})">${opt.label}</button>`;
        })
        .join("");

    return `
        <div class="vcp-control-row ${feature.supported ? "" : "unsupported-feature"}">
            <div class="vcp-row-top">
                <div class="vcp-label-wrap">
                    <span>${label}</span>
                </div>
            </div>
            <div class="chip-selector-group">
                ${chips}
            </div>
        </div>
    `;
}

// ============================================================================
// VISTA MULTI-MONITOR AFFIANCATI
// ============================================================================
function renderMultiMonitorsView(container) {
    const hideUnsup = !!state.settings.hide_unsupported;
    container.innerHTML = state.monitors
        .map((m) => {
            return `
                <div class="glass-detail-panel" data-monitor-id="${m.id}">
                    <div class="detail-panel-header">
                        <span>${escapeHtml(m.name)} (${escapeHtml(m.brand)})</span>
                        <span>${escapeHtml(m.resolution)}</span>
                    </div>
                    ${renderDetailSliderRow(m.id, "Luminosità", m.brightness, VCP.BRIGHTNESS, iconSun(), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Contrasto", m.contrast, VCP.CONTRAST, iconContrast(), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Nitidezza", m.sharpness, VCP.SHARPNESS, iconSparkle(), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Saturazione", m.saturation, VCP.SATURATION, iconDroplet(), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Guadagno Rosso (R)", m.red_gain, VCP.RED_GAIN, iconColorDot("#ef4444"), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Guadagno Verde (G)", m.green_gain, VCP.GREEN_GAIN, iconColorDot("#10b981"), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Guadagno Blu (B)", m.blue_gain, VCP.BLUE_GAIN, iconColorDot("#3b82f6"), hideUnsup)}
                    ${renderDetailSliderRow(m.id, "Volume Audio", m.volume, VCP.VOLUME, iconVolume(), hideUnsup)}
                    ${renderChipGroupRow(m.id, "Temperatura Colore", m.color_temp, VCP.COLOR_TEMP, COLOR_TEMP_PRESETS, hideUnsup)}
                    ${renderChipGroupRow(m.id, "Sorgente Input", m.input_source, VCP.INPUT_SOURCE, INPUT_SOURCES, hideUnsup)}
                </div>
            `;
        })
        .join("");
}

// ============================================================================
// INTERAZIONE ISTANTANEA CON SLIDER, PRESET E CHIP DDC/CI
// ============================================================================
function onSliderInput(monitorId, vcpCode, rawValue, maxVal) {
    const val = parseInt(rawValue, 10) || 0;
    const syncAll = !!state.settings.sync_monitors;
    const mMax = maxVal || 100;

    state.monitors.forEach((m) => {
        if (syncAll || m.id === monitorId) {
            updateLocalMonitorFeature(m, vcpCode, val);

            // Aggiorna badge dettaglio
            const badge = document.getElementById(`val-${m.id}-${vcpCode}`);
            if (badge) {
                badge.textContent = `${val}${mMax === 100 ? "%" : " / " + mMax}`;
            }

            // Aggiorna valore grande nella Stat Card
            const statBig = document.getElementById(`stat-val-${m.id}-${vcpCode}`);
            if (statBig) {
                statBig.textContent = mMax === 100 ? `${val}%` : `${val}/${mMax}`;
            }

            // Se è Luminosità o Contrasto sul monitor selezionato, aggiorna anche la Hero Card e il Gauge circolare!
            if (m.id === state.selectedMonitorId) {
                if (vcpCode === VCP.BRIGHTNESS) {
                    const hLum = document.getElementById("hero-metric-lum");
                    if (hLum) hLum.textContent = `${val}%`;
                    const gVal = document.getElementById("hero-gauge-val");
                    if (gVal) gVal.textContent = `${val}%`;
                    const gCirc = document.getElementById("hero-gauge-circle");
                    if (gCirc) {
                        const circumference = 351.86;
                        const offset =
                            circumference - (Math.min(100, Math.max(0, val)) / 100) * circumference;
                        gCirc.setAttribute("stroke-dashoffset", String(offset));
                    }
                } else if (vcpCode === VCP.CONTRAST) {
                    const hCon = document.getElementById("hero-metric-con");
                    if (hCon) hCon.textContent = `${val}%`;
                }
            }
        }
    });

    renderSidebarMonitors();

    sendIpc({
        cmd: "set_vcp",
        monitor_id: monitorId,
        vcp_code: vcpCode,
        value: val,
        sync_all: syncAll,
    });
}

function applyQuickPreset(monitorId, targetLum, targetCon) {
    onSliderInput(monitorId, VCP.BRIGHTNESS, targetLum, 100);
    onSliderInput(monitorId, VCP.CONTRAST, targetCon, 100);
    renderActiveView();
}

function resetRgbGain(monitorId) {
    onSliderInput(monitorId, VCP.RED_GAIN, 50, 100);
    onSliderInput(monitorId, VCP.GREEN_GAIN, 50, 100);
    onSliderInput(monitorId, VCP.BLUE_GAIN, 50, 100);
    renderActiveView();
}

function onChipSelect(monitorId, vcpCode, value) {
    const syncAll = !!state.settings.sync_monitors;
    state.monitors.forEach((m) => {
        if (syncAll || m.id === monitorId) {
            updateLocalMonitorFeature(m, vcpCode, value);
        }
    });

    sendIpc({
        cmd: "set_vcp",
        monitor_id: monitorId,
        vcp_code: vcpCode,
        value,
        sync_all: syncAll,
    });

    renderActiveView();
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

// ============================================================================
// AGGIORNAMENTI GITHUB RELEASES
// ============================================================================
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
    const sidebarBadge = document.getElementById("sidebar-update-badge");

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
        if (sidebarBadge) sidebarBadge.classList.remove("hidden");
    } else {
        if (modalInstallBtn) modalInstallBtn.classList.add("hidden");
        if (sidebarBadge) sidebarBadge.classList.add("hidden");
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

// Icone SVG vettoriali
function iconSun() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"></path></svg>`;
}
function iconContrast() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"></circle><path d="M12 3v18a9 9 0 0 0 0-18z" fill="currentColor"></path></svg>`;
}
function iconDroplet() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2.69l5.66 5.66a8 8 0 1 1-11.31 0z"></path></svg>`;
}
function iconSparkle() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`;
}
function iconColorDot(hex) {
    return `<span style="display:inline-block;width:11px;height:11px;border-radius:50%;background:${hex};box-shadow:0 0 8px ${hex};"></span>`;
}
function iconThermometer() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 14.76V3.5a2.5 2.5 0 0 0-5 0v11.26a4.5 4.5 0 1 0 5 0z"></path></svg>`;
}
function iconVolume() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14"></path></svg>`;
}
function iconCable() {
    return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="7" width="20" height="10" rx="2"></rect><line x1="6" y1="12" x2="6.01" y2="12"></line><line x1="10" y1="12" x2="10.01" y2="12"></line><line x1="14" y1="12" x2="18" y2="12"></line></svg>`;
}
