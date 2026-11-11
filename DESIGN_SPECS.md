# Monitor Manager — Specifiche Complete di Progetto (Master Doc)

Questo documento racchiude **tutte le informazioni, i requisiti tecnici, l'hardware di riferimento e le direttive di design** concordate finora per lo sviluppo dell'applicazione.

---

## 1. Identità e Obiettivi Generali
- **Nome Progetto**: `Monitor Manager`
- **Piattaforma target**: Windows (10 e 11).
- **Linguaggio di sviluppo**: **Rust**.
- **Requisiti non funzionali chiave**:
  - **Leggerezza estrema e velocità**: Binario nativo senza runtime pesanti (niente Electron/browser overhead ingombranti), avvio rapido e consumo RAM minimo.
  - **Modifiche istantanee**: Nessun lag percepito durante la regolazione dei parametri (comandi DDC/CI inviati su thread separati asincroni/background per non bloccare mai la UI).
  - **Rilevamento automatico e universale**: L'app deve riconoscere in automatico ogni monitor collegato al PC; in configurazioni multi-monitor deve mostrarli tutti contemporaneamente o con switch immediato.

---

## 2. Hardware e Monitor Target Specifici
L'app è pensata per funzionare in modo universale con qualsiasi marca e modello, con focus sui monitor attualmente a disposizione:
1. **ASUS TUF Gaming**:
   - Sostituirà la suite proprietaria ASUS (notoriamente pesante e lenta).
   - Controllo standard DDC/CI + eventuale mappatura di codici VCP custom (es. GameVisual, Shadow Boost, overdrive) una volta rilevate le capabilities.
2. **AOC**:
   - Sostituirà l'app ufficiale AOC (G-Menu, poco ottimizzata).
   - Controllo DDC/CI standard + VCP per filtri/impostazioni colore.
3. **KTC**:
   - Fornirà un'interfaccia di gestione software completa per un monitor che non dispone di alcuna app ufficiale dedicata.

---

## 3. Architettura Tecnica & Protocollo Hardware
- **Protocollo**: **DDC/CI (VESA MCCS)** attraverso il cavo video (DisplayPort, HDMI, DVI).
- **Integrazione Windows**:
  - `EnumDisplayMonitors` + `GetPhysicalMonitorsFromHMONITOR` per enumerare i display fisici.
  - Lettura EDID per recuperare modello, produttore e numero di serie univoco.
  - API `Dxva2` (`GetVCPFeatureAndVCPFeatureReply`, `SetVCPFeature`) o crate Rust dedicati (`ddc-winapi`, `windows-rs`).
  - Thread worker DDC dedicato: comunicazione I²C/DDC disaccoppiata dalla grafica a 60/120+ FPS.

---

## 4. Direttive di Design & Stile UI (Mandatorie)
Ispirazione estetica: **Apple iOS 26 Liquid Glass**

### Sfondo ed Atmosfera
- Sfondo con una **sfumatura delicata** arricchita da **sottili macchie/sfere di colore fluttuanti** (floating glowing color blobs).
- Effetto di profondità trasparente/vetrosa coerente con l'ambiente desktop moderno.

### Componenti (Schede, Card, Pulsanti, Campi di Input, Slider)
- **Effetto Vetro Satinato Traslucido**: Frosted glass con forte sfocatura di sfondo (*heavy backdrop blur*).
- **Bordi e Riflessi**: Bordi sottili, luminosi ed evidenziati (*specular highlights* / riflessi di luce sui bordi) che simulano lo spessore e la brillantezza del vetro liquido.
- **Ombre**: Ombre morbide, diffuse e multistrato.
- **Geometria**: Angoli generosamente arrotondati (*continuous rounded corners / superellipse*).

### 4. Gestione Temi & Personalizzazione
- **Opzioni Tema**:
  - **Scuro (Dark Mode)**: Vetro fumé satinato con accenti e riflessi luminescenti, sfumature profonde e bolle di colore ad alto contrasto.
  - **Chiaro (Light Mode)**: Vetro cristallino satinato ad alta luminosità, ombre soffuse, riflessi perlacei e gradienti ariosi.
  - **Automatico / Sistema (Sync with Windows)**: Rilevamento automatico delle preferenze di sistema di Windows (lettura chiavi di registro `AppsUseLightTheme` / notifiche di cambio tema live) con transizione fluida istantanea.
- **Selettore dedicato**: Switch toggle/segmented control in pieno stile Liquid Glass accessibile dalle impostazioni dell'app.

### Tipografia e Micro-interazioni
- **Tipografia**: Font di sistema pulito, nitido e moderno.
- **Interazioni**: **Fluide animazioni a molla (*spring physics*)** su hover, click, scorrimento slider, aperture menu e transizioni tra monitor.

---

## 5. Sistema di Aggiornamento Software (Auto-Updater via GitHub)
- **Meccanismo**:
  - Controllo automatico della versione all'avvio dell'applicazione (in background, non bloccante).
  - Possibilità di effettuare il controllo e l'aggiornamento manuale in qualsiasi momento tramite pulsante nelle impostazioni.
- **Provider / Backend**: Repository GitHub tramite le **GitHub Releases**.
- **Dati Repository GitHub**:
  - **Owner**: `riccardo-bettini`
  - **Repository**: `Monitor-Manager`
  - **URL Remoto**: `https://github.com/riccardo-bettini/Monitor-Manager.git`
  - **Visibilità**: Pubblica
  - **Branch principale**: `main`
- **Funzionamento**:
  - Endpoint API per check versione: `https://api.github.com/repos/riccardo-bettini/Monitor-Manager/releases/latest`
  - Confronto semver con la versione interna compilata (`env!("CARGO_PKG_VERSION")`).
  - Notifica non invasiva in stile Liquid Glass e download/sostituzione automatica del binario.

---

## 6. Scelte Architetturali & Funzionalità v1.00 Approvate
- **Stack**: Rust nativo + WebView2 di sistema (`tao` + `wry` + `tray-icon`), singolo eseguibile `.exe` con asset UI incorporati (`include_str!`).
- **Finestra & System Tray**: Finestra desktop frameless con barra del titolo custom in stile Liquid Glass + icona nella System Tray per ridurla in background e riaprirla rapidamente.
- **Doppia Modalità di Visualizzazione Monitor**:
  - **Multi-Card View (Schede Affiancate)**: Tutti i monitor visibili e regolabili contemporaneamente + opzione "Sincronizza tutti".
  - **Monitor Switcher (Selettore Superiore)**: Barra superiore con i monitor collegati e vista singola dettagliata.
  - Selettore dedicato nelle Impostazioni e toggle rapido nell'header.
- **Controlli Base v1.00**:
  - Immagine: Luminosità (`0x10`), Contrasto (`0x12`), Saturazione (`0x8A`), Nitidezza / Sharpness (`0x87`).
  - Colore: Guadagno RGB (`0x16`, `0x18`, `0x1A`) e Temperatura Colore (`0x14`).
  - Audio & Sorgente: Volume Audio (`0x62`) e Sorgente Input (`0x60`).
  - Mostra tutti i controlli base (indicando se un parametro non è supportato dal firmware del singolo monitor) + toggle nelle Impostazioni *"Nascondi controlli non supportati"*.
- **Auto-Updater & Avvio con Windows**:
  - Notifica Liquid Glass con pulsante *"Aggiorna e Riavvia ora"* (download `.exe` da GitHub Releases, sostituzione eseguibile e riavvio automatico).
  - Opzione *"Avvia automaticamente con Windows"* nelle Impostazioni.

---

## 8. Nuove Direttive di Design "Dark Matte Titanium" & Architettura Nativa Slint (v1.03+)

Con il passaggio all'architettura nativa pura **Slint** (senza Chromium/WebView2 per garantire un consumo RAM di soli **~25-35 MB** attiva e **~2-8 MB** in background durante il gaming), le vecchie direttive basate su filtri CSS ad alto impatto di compositing (`backdrop-filter: blur`, particelle pesanti in DOM) sono sostituite dalle **nuove direttive stilistiche "Dark Matte Titanium"**, basate direttamente sull'anteprima nativa approvata.

### 8.1. Filosofia Visiva: "Dark Matte Titanium"
- **Sfondo Finestra**: Tinta unita profonda antracite scura opaca (`#16191f` / `#1c1f26`), pulita e priva di filtri blur a livello software per azzerare il carico GPU/CPU.
- **Card e Pannelli**: Finitura solida opaca satinata (`#21252d` con sfumatura impercettibile verso `#272b35`), bordi netti e geometrici ma ingentiliti con raggio di curvatura sottile e moderno (`border-radius: 10px - 14px`).
- **Glow & Bordi Attivi (Neon Glow)**: L'elemento attivo (es. card del monitor selezionato nella sidebar o slider in trascinamento) è evidenziato da un bordo ad alto contrasto con bagliore neon colorato d'accento (`#ff5733` default Coral Neon).
- **Barra Superiore Borderless Integrata**:
  - Nessuna barra standard Windows vistosa: barra del titolo integrata seamless a tutta larghezza con drag della finestra.
  - Tasti finestra (Riduci a icona, Ingrandisci, Chiudi) collocati in un blocco compatto ad accento cromatico in alto a destra, come nell'anteprima.

### 8.2. Sistema di Personalizzazione a Zero Impatto RAM
Tutte le personalizzazioni sono gestite tramite proprietà reattive native in Slint (`slint::Color`, enum ed interi), senza allocazione di texture o buffer di memoria aggiuntivi:
1. **Palette Colore d'Accento (5 Preset)**:
   - **Coral Neon** (Predefinito, arancione/rosso gaming vivace `#ff5733`)
   - **Cyber Cyan** (Azzurro elettrico `#00e5ff`)
   - **Emerald Green** (Verde smeraldo `#00e676`)
   - **Electric Violet** (Viola dinamico `#bd00ff`)
   - **Amber Gold** (Giallo ambra intenso `#ffb300`)
   - *Effetto*: Colora istantaneamente l'anello gauge circolare, gli indicatori degli slider, i badge di stato e il glow dei monitor selezionati.
2. **Stile Glow dei Bordi (Bordo ed Enfasi)**:
   - *Minimal / Spento*: Bordo chiaro semplice da 1px senza bagliore diffuso.
   - *Sottile (Predefinito)*: Bordo d'accento con alone luminoso discreto.
   - *Marcato (Gaming Glow)*: Bordo ad alta saturazione con contorno doppio per massima leggibilità durante il gioco.
3. **Densità Layout**:
   - *Standard* (Ideale per monitor 1440p / 4K, spaziatura ariosa).
   - *Compatta* (Margini e altezze ridotti, ideale per schermi 1080p o finestre affiancate).
4. **Localizzazione Bilingue (Lingua Interfaccia)**:
   - Selettore istantaneo **Italiano** / **English**:
     - *IT*: Monitor Manager, Luminosità, Contrasto, Nitidezza, Saturazione, Guadagno RGB, Impostazioni.
     - *EN*: Monitor Manager, Brightness, Contrast, Sharpness, Saturation, RGB Gain, Settings.

### 8.3. Layout e Componenti Chiave
1. **Sidebar Sinistra**:
   - Logo e nome `Monitor Manager` in testa.
   - **Lista Monitor Hardware**: Pill dedicate per ciascun monitor rilevato (**ASUS TUF Gaming VG27A**, **AOC Q27G2**, **KTC H27T22**) con icona display, nome EDID e indicatore LED / glow sul monitor attualmente attivo.
   - **Navigazione Inferiore**: *Dashboard*, *Profili Colore / Tutti i Monitor*, *Impostazioni*.
2. **Hero Header Card**:
   - Nome del monitor a caratteri grandi.
   - Badge di stato verde brillante `● DDC/CI Active`.
   - Scheda tecnica compatta: `Risoluzione | Frequenza Hz | Porta d'ingresso` (es. `2560×1440 | 165Hz | DisplayPort 1`).
   - **Circular Gauge 75%**: Anello circolare ad alto spessore che visualizza la luminosità del display con percentuale numerica al centro.
3. **Quartetto Stat Cards**:
   - 4 card affiancate: *Brightness* (0x10), *Contrast* (0x12), *Sharpness* (0x87), *Saturation* (0x8A).
   - Valore numerico grande ad alto contrasto (75%, 50%, 60%, 50%).
   - Mini-slider orizzontale incassato direttamente nella parte inferiore di ciascuna card per regolazione a colpo d'occhio.
4. **Pannello Guadagno RGB (RGB Gain)**:
   - 3 slider orizzontali colorati individualmente (*Red*, *Green*, *Blue*) per calibrazione accurata dei singoli canali colore del monitor.




