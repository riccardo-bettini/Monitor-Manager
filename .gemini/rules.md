# Regole del Progetto Monitor Manager

Questo progetto segue tassativamente le regole definite in `RULES.md` e `DESIGN_SPECS.md`:

1. **Versionamento**:
   - Parte da **`v1.00`**.
   - Avanza di **`+0.01`** alla volta (`v1.00`, `v1.01`, `v1.02`...).
2. **Release su GitHub**:
   - Ogni aggiornamento deve essere caricato come GitHub Release su `https://github.com/riccardo-bettini/Monitor-Manager`.
   - La pubblicazione di una release NON deve mai avvenire in automatico: **avviene SOLO ed ESCLUSIVAMENTE su comando esplicito dell'utente**.
3. **UI e Design**:
   - Stile rigoroso **Apple iOS 26 Liquid Glass** (sfondo sfumato con bolle di colore fluttuanti, frosted glass con forte blur, bordi speculari luminosi, angoli arrotondati, animazioni a molla).
   - Temi: Chiaro, Scuro, e Sincronizzato con il Sistema Windows.
4. **Performance & Architettura**:
   - Linguaggio: Rust. Leggero, veloce e nativo.
   - Tutti i comandi DDC/CI eseguiti rigorosamente su thread asincroni per garantire 0 lag sulla UI.
