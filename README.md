# Monitor Manager

Applicazione nativa ultra-leggera per Windows scritta in **Rust** (con UI nativa Slint, VESA MCCS `ddc-hi`/`ddc-winapi`/`mccs-db`, Windows `DisplayConfig` e calibrazione GDI LUT) per il rilevamento automatico e il controllo hardware/software completo dei monitor collegati (**ASUS TUF Gaming**, **AOC**, **Philips**, **KTC**, monitor generici DDC/CI e schermi **portatili Windows**).

## Caratteristiche Principali (`v1.15`)
- **Architettura Ultra-Leggera Nata da Zero**: Consumo di RAM minimo con rendering accelerato nativo e zero overhead in System Tray.
- **Scansione Hardware a 2 Fasi (`ddc-hi` + `Dxva2` + `mccs-caps`/`mccs-db`)**: Avvio immediato con i controlli principali, seguito dalla lettura asincrona della stringa MCCS Capabilities e dei registri VCP supportati.
- **Profili Brand & Gaming Specifici**: Supporto dedicato e mappatura VCP avanzata per **ASUS TUF**, **AOC**, **Philips** (inclusa decodifica EDID `PHL`) e **KTC** (Overdrive, Shadow Boost, Adaptive-Sync, GameVisual, Low Blue Light, Power Mode, ecc.) con filtri per categoria e protezione di conferma sui registri critici.
- **Display Windows Nativi & Calibrazione Software GDI LUT (Portatili + Esterni)**: Selettore frequenze di aggiornamento (`Hz`), toggle **HDR Windows** nativo e calibrazione colore per singolo schermo via `SetDeviceGammaRamp` (Gamma, Filtro Luce Blu, Contrasto, RGB Gain).
- **Auto-Updater & Installer con Modale di Avanzamento**: Finestra dedicata accessibile da qualsiasi schermata con barra di avanzamento reale, MB scaricati/totali, velocità (`MB/s`), fasi esplicite e bottoni dinamici (`Aggiorna Ora` → `Annulla` → `Riavvia Ora`), oltre alla progress bar integrata nell'installer `SetupWindow`.

## Installazione
Scarica ed esegui l'installer dalla sezione [Releases](https://github.com/riccardo-bettini/Monitor-Manager/releases).
