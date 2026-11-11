# Monitor Manager

Applicazione nativa per Windows scritta in **Rust** per il rilevamento automatico e il controllo hardware universale in tempo reale dei monitor collegati (es. **ASUS TUF Gaming**, **AOC**, **KTC** e qualsiasi monitor compatibile **DDC/CI VESA MCCS**), con interfaccia ispirata ad **Apple iOS 26 Liquid Glass**.

## Caratteristiche Principali (`v1.00`)
- **Rilevamento Automatico Multi-Monitor**: Identificazione in tempo reale tramite `EnumDisplayMonitors`, `QueryDisplayConfig` ed **EDID** hardware (marca, modello, risoluzione, stato DDC/CI).
- **Zero-Lag Hardware Control**: Coda di comandi atomica con coalescenza su worker thread dedicati per regolare i parametri del monitor senza alcun blocco dell'interfaccia a 120 FPS.
- **Pacchetto Controlli Base**:
  - **Immagine**: Luminosità (`0x10`), Contrasto (`0x12`), Saturazione (`0x8A`), Nitidezza / Sharpness (`0x87`).
  - **Colore**: Guadagno Rosso (`0x16`), Verde (`0x18`), Blu (`0x1A`) e Temperatura Colore (`0x14`).
  - **Audio & Sorgente**: Volume Altoparlanti (`0x62`) e Sorgente Input Video (`0x60` — DisplayPort 1/2, HDMI 1/2, USB-C).
- **Doppia Modalità di Visualizzazione**:
  - **Vista Schede Affiancate (Multi-Card)** con funzione **Sync Tutti**.
  - **Vista Selettore Singolo (Monitor Switcher)**.
- **Design Apple iOS 26 Liquid Glass**:
  - Sfondo con sfumatura delicata e macchie di colore fluttuanti, schede e controlli in vetro satinato traslucido (`backdrop-filter: blur`), bordi luminosi speculari e animazioni a molla.
  - Supporto completo per tema **Chiaro**, **Scuro** e **Sincronizzato con Windows**.
- **System Tray & Auto-Updater**:
  - Riduzione nella System Tray di Windows e avvio automatico opzionale con Windows.
  - Sistema di aggiornamento automatico collegato alle **GitHub Releases** di questo repository.

## Installazione
Scarica ed esegui **`Monitor-Manager-Setup-v1.00.exe`** dalla sezione [Releases](https://github.com/riccardo-bettini/Monitor-Manager/releases).
