# Offene Aufgaben

## Review Phase 0 · 17.11.2026

- [ ] **Niedrig – Fehlerrückmeldung ohne Konsole:** Fehler beim Start und Öffnen der Einstellungen werden derzeit ausschließlich nach stderr geschrieben (`app/src-tauri/src/main.rs`, `app/src-tauri/src/lib.rs`). Die Release-App besitzt keine Konsole; Betroffene erhalten daher keine sichtbare Erklärung. Einen lokalen, deutschen Fehlerdialog aus dem Sprachkatalog ergänzen und den Fehlerpfad testen, ohne technische Details oder private Inhalte auszugeben.
- [x] **Niedrig – Speicherziel des Gerüsts:** Durch die Datenschutzkorrektur aus [Entscheidung 0006](decisions/0006-native-fenster-ohne-webview2.md) entfallen die WebViews. Die erneute [Messung vom 17.11.2026](benchmarks/idle-2026-11-17.json) erreicht maximal 2,84 MB Private Bytes und 18,49 MB Working Set, damit liegt das aktuelle Gerüst unter 100 MB. Die spätere Leistungsabnahme der vollständigen App in Schritt 8.2 bleibt vorgesehen.
