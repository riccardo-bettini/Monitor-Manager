# Monitor Manager

Applicazione nativa ultra-leggera per Windows scritta in **Rust** (con Slint toolkit nativo e DDC/CI VESA MCCS) per il rilevamento automatico e il controllo hardware in tempo reale dei monitor collegati (es. **ASUS TUF Gaming**, **AOC**, **KTC** e qualsiasi monitor compatibile DDC/CI).

## Caratteristiche Principali (`v1.07`)
- **Architettura Ultra-Leggera Nata da Zero**: Consumo di RAM minimo (< 25-30 MB) con rendering accelerato nativo.
- **Rilevamento Automatico Multi-Monitor**: Identificazione in tempo reale tramite `EnumDisplayMonitors`, `QueryDisplayConfig` ed **EDID** hardware (marca, modello, risoluzione, stato DDC/CI).
- **Nuova Sezione Home con Vista a Blocchi**: Panoramica affiancata e regolazione rapida istantanea di luminosità e contrasto su tutti i monitor con switch immediato ai dettagli completi.
- **Navigazione Sidebar ad Accordion**: Menu contestuale con sotto-elenco (Dashboard e Profili Colore) direttamente sotto il display selezionato.
- **Gestione & Riordino Sequenza Monitor**: Sezione "Ordina Monitor" con pulsanti Su/Giù per personalizzare la priorità e la disposizione nella barra laterale e nella Home, salvata in modo persistente.
- **Sezione Impostazioni a 4 Card Tematiche**: Organizzazione modulare pulita per Sistema & Comportamento, Aspetto & Personalizzazione Cyberpunk, Hardware & Lingua, Software & Aggiornamenti.
- **Zero-Lag Hardware Control**: Coda di comandi atomica con coalescenza su worker thread dedicati per regolare i parametri del monitor a 144Hz+ senza alcun blocco dell'interfaccia.
- **System Tray & Auto-Updater Blindato**:
  - Riduzione nella barra delle applicazioni / System Tray di Windows.
  - Sistema di aggiornamento automatico silenzioso integrato con le **GitHub Releases** (self-update a 0 click e installer protetto con isolamento PID).

## Installazione
Scarica ed esegui l'installer dalla sezione [Releases](https://github.com/riccardo-bettini/Monitor-Manager/releases).
