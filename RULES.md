# Regole Base di Sviluppo e Release — Monitor Manager

Questo file definisce le regole operative, il ciclo di vita delle versioni e le modalità di rilascio del progetto **Monitor Manager**.

---

## 1. Schema di Versionamento
- **Versione di partenza**: Si parte da **`v1.00`** (rilasciata).
- **Incremento incrementale obbligatorio**: **OGNI** singola modifica, correzione, aggiunta di feature o ritocco (anche minimo) comporta tassativamente l'avanzamento della versione di **`+0.01`** (es. `v1.00` → `v1.01` → `v1.02` → `v1.03` ...).
- **Allineamento Versioni**: A ogni modifica, il nuovo numero di versione deve essere aggiornato sincronizzatamente in:
  - `Cargo.toml` (`display_version` e `version`)
  - `src/updater.rs` (`APP_VERSION`)
  - `src/bin/setup.rs` (`APP_VERSION`)
  - Eventuali riferimenti UI e documentazione


---

## 2. Politica di Pubblicazione & Release
- **Pubblicazione Automatica a Ogni Modifica**: Salvo indicazioni diverse dell'utente, **OGNI SINGOLA MODIFICA** (bugfix, miglioramento, nuova feature) approvata o implementata nell'applicazione comporta automaticamente l'incremento di versione (`+0.01`) e la pubblicazione immediata della nuova release su GitHub.
- **Canale di Distribuzione Ufficiale**:
  - Ogni aggiornamento deve essere caricato come **GitHub Release** nella repository ufficiale:
    `https://github.com/riccardo-bettini/Monitor-Manager`
  - Ogni release deve tassativamente contenere **entrambi** i seguenti file:
    1. `Monitor-Manager-Setup-vX.XX.exe` (l'installer completo per nuove installazioni o download manuale).
    2. `monitor-manager.exe` (l'eseguibile nativo puro per l'auto-updater in-app silenzioso a 0 click).
  - Un sintetico changelog delle novità/fix nelle note di rilascio.

---

## 3. Qualità del Codice & Prestazioni
- **Nessun Blocco UI**: Tutte le operazioni di comunicazione hardware (DDC/CI, scansioni I²C/VCP) devono risiedere su thread di background disaccoppiati per garantire che l'interfaccia resti costantemente a 60/120+ FPS.
- **Binario Leggero**: Mantenere al minimo le dipendenze esterne non necessarie; privilegiare compilazioni ottimizzate native per Windows.

---

## 4. Integrità del Design
- Tutte le modifiche all'interfaccia devono rispettare rigorosamente il design system **Apple iOS 26 Liquid Glass** (sfumature fluide, sfere di colore fluttuanti, frosted glass con forte sfocatura, bordi speculari luminosi, angoli arrotondati e animazioni a molla).
- Supporto obbligatorio ai 3 temi: **Chiaro**, **Scuro**, e **Sincronizzato col Sistema**.
