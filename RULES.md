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
- **Controllo Totale dell'Utente**: **NESSUNA** release su GitHub deve mai essere creata o pubblicata in autonomia dall'agente/assistente.
- **Autorizzazione Esplicita**: La creazione di tag Git e la pubblicazione di una nuova release avverrà **esclusivamente su comando esplicito dell'utente**.
- **Canale di Distribuzione Ufficiale**:
  - Ogni aggiornamento approvato deve essere caricato come **GitHub Release** nella repository ufficiale:
    `https://github.com/riccardo-bettini/Monitor-Manager`
  - Ogni release deve contenere l'eseguibile compilato per Windows (es. `.exe` o archivio `.zip` compresso) e un sintetico changelog delle novità/fix.

---

## 3. Qualità del Codice & Prestazioni
- **Nessun Blocco UI**: Tutte le operazioni di comunicazione hardware (DDC/CI, scansioni I²C/VCP) devono risiedere su thread di background disaccoppiati per garantire che l'interfaccia resti costantemente a 60/120+ FPS.
- **Binario Leggero**: Mantenere al minimo le dipendenze esterne non necessarie; privilegiare compilazioni ottimizzate native per Windows.

---

## 4. Integrità del Design
- Tutte le modifiche all'interfaccia devono rispettare rigorosamente il design system **Apple iOS 26 Liquid Glass** (sfumature fluide, sfere di colore fluttuanti, frosted glass con forte sfocatura, bordi speculari luminosi, angoli arrotondati e animazioni a molla).
- Supporto obbligatorio ai 3 temi: **Chiaro**, **Scuro**, e **Sincronizzato col Sistema**.
