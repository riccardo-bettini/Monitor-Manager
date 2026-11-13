# Regole del Progetto Monitor Manager

Questo progetto segue tassativamente le regole definite in `RULES.md` e `DESIGN_SPECS.md`:

1. **Versionamento**:
   - Inizio da **`v1.00`** (rilasciata).
   - **OGNI SINGOLA MODIFICA** (anche minima correzione o feature) fa avanzare la versione di **`+0.01`** (`v1.01`, `v1.02`, `v1.03`...).
   - La nuova versione viene subito sincronizzata in `Cargo.toml`, `src/updater.rs` e `src/bin/setup.rs`.
2. **Release su GitHub**:
   - Ogni aggiornamento deve essere caricato come GitHub Release su `https://github.com/riccardo-bettini/Monitor-Manager`.
   - La pubblicazione di una release NON deve mai avvenire in automatico: **avviene SOLO ed ESCLUSIVAMENTE su comando esplicito dell'utente**.
3. **UI e Design**:
   - Stile rigoroso **Apple iOS 26 Liquid Glass** (sfondo sfumato con bolle di colore fluttuanti, frosted glass con forte blur, bordi speculari luminosi, angoli arrotondati, animazioni a molla).
   - Temi: Chiaro, Scuro, e Sincronizzato con il Sistema Windows.
4. **Performance & Architettura**:
   - Linguaggio: Rust. Leggero, veloce e nativo.
   - Tutti i comandi DDC/CI eseguiti rigorosamente su thread asincroni per garantire 0 lag sulla UI.
5. **Build & Test Cargo (Affidabilità su Windows)**:
   - Disabilitare colori e progress bar interattive di Cargo (configurato in `.cargo/config.toml` con `term.color = "never"` e `term.progress.when = "never"`).
   - Eseguire i test con `-- --nocapture` (es. `cargo test -- --nocapture`) per evitare buffering bloccante dell'output.
   - **Prevenzione blocco shell in background (`Wait / UserRequest`)**:
     - Terminare **sempre** i comandi `cargo` (check, test, build) con `; exit $LASTEXITCODE` (es. `cargo test -- --nocapture; exit $LASTEXITCODE`) oppure eseguirli tramite `cmd /c "<comando> < NUL"` per forzare la chiusura immediata di `powershell.exe` al termine ed evitare che resti appeso sullo stdin.
     - Non concatenare comandi `cargo` pesanti nella stessa invocazione (eseguire `cargo check` e `cargo test` in chiamate separate).
     - Usare `WaitMsBeforeAsync: 10000` per i comandi `cargo` rapidi così da completarli in modalità sincrona senza creare background task.
   - Prima di una build `--release`, verificare/chiudere eventuali istanze attive di `monitor-manager.exe` per evitare lock su Windows durante il linking.
   - Le build `--release` usano `lto = true` e `codegen-units = 1` in `Cargo.toml`, quindi richiedono più di 10 secondi e passano in background: attendere il completamento senza interromperle prematuramente.
