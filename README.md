# Monitor Manager

Applicazione nativa ultra-leggera per Windows scritta in **Rust** (con Slint toolkit nativo e DDC/CI VESA MCCS) per il rilevamento automatico e il controllo hardware in tempo reale dei monitor collegati (es. **ASUS TUF Gaming**, **AOC**, **KTC** e qualsiasi monitor compatibile DDC/CI).

## Caratteristiche Principali (`v1.04`)
- **Architettura Ultra-Leggera Nata da Zero**: Consumo di RAM minimo (< 25-30 MB) con rendering accelerato nativo.
- **Rilevamento Automatico Multi-Monitor**: Identificazione in tempo reale tramite `EnumDisplayMonitors`, `QueryDisplayConfig` ed **EDID** hardware (marca, modello, risoluzione, stato DDC/CI).
- **Zero-Lag Hardware Control**: Coda di comandi atomica con coalescenza su worker thread dedicati per regolare i parametri del monitor a 144Hz+ senza alcun blocco dell'interfaccia.
- **Pacchetto Controlli Hardware**:
  - **Luminosità** (`0x10`), **Contrasto** (`0x12`), **Nitidezza** (`0x87`), **Saturazione** (`0x8A`).
  - **RGB Gain**: Guadagno Rosso (`0x16`), Verde (`0x18`), Blu (`0x1A`).
  - **Audio**: Volume (`0x62`).
- **Interfaccia Cyberpunk Slate / Dark Mode**:
  - Sidebar scura con glow al neon reattivo sul monitor selezionato.
  - Indicatore circolare luminoso di percentuale.
  - Slider interattivi reattivi con feedback visivo immediato.
- **System Tray & Auto-Updater**:
  - Riduzione nella barra delle applicazioni / System Tray di Windows.
  - Sistema di aggiornamento automatico silenzioso integrato con le **GitHub Releases**.

## Installazione
Scarica ed esegui l'installer dalla sezione [Releases](https://github.com/riccardo-bettini/Monitor-Manager/releases).
