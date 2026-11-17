# Quatsch mit Soße – Regeln für Coding-Agenten
## Projekt
Deutsche, offline-first Diktier-App für Windows 11 (Tauri 2, Rust-Workspace, Svelte 5 + TypeScript). Der Plan steht in docs/masterplan.md. Bei Widersprüchen gilt der Masterplan; begründete Abweichungen kommen nach docs/decisions/.
## Sprache
UI-Texte, Doku und Commit-Nachrichten auf Deutsch. Code, Bezeichner und Kommentare auf Englisch.
## Datenschutz (nicht verhandelbar)
- Keine Telemetrie, keine Analytics, keine Absturzberichte nach außen.
- Keine Screenshots und keine Aufrufe von Bildschirmaufnahme-APIs.
- Netzwerk nur für Modell-Downloads mit SHA256-Prüfung und für ausdrücklich aktivierte KI-Anbieter.
- Diktierten Text, Audio und Tokens nie loggen.
- Nie committen: testdata/private/, Modelldateien, Zugangsdaten.
## Arbeitsweise
- Genau eine Aufgabe pro Lauf, nichts darüber hinaus.
- Erst Tests schreiben, dann implementieren.
- Vor Abschluss müssen grün sein: cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, und in app/ui: npm run check und npm test.
- Bei Unklarheiten die Empfehlung aus dem Masterplan nehmen und in docs/decisions/ notieren.
- Zum Schluss auf Deutsch zusammenfassen: was geändert, wie getestet, was offen ist, was der Mensch prüfen soll.
## Technik
- Spracherkennung über sherpa-onnx (Crate sherpa-rs) hinter dem Trait in crates/stt.
- Windows-APIs über die Crate windows.
- Einfügen in dieser Reihenfolge: UI Automation, SendInput mit KEYEVENTF_UNICODE, Zwischenablage mit vollständiger Wiederherstellung.
- Anzeigename "Quatsch mit Soße", technischer Name quatsch-mit-sosse (in Dateinamen, Crates, Adressen und Bezeichnern nur ASCII). Tauri-Bezeichner de.quatschmitsosse.app.
- Der Projektordner H:\Quatsch mit Soße enthält Leerzeichen und ß: Pfade in Skripten und Befehlen immer in Anführungszeichen setzen. Rust-Build-Ausgabe über CARGO_TARGET_DIR=C:\qms-target.
- Modelle unter %LOCALAPPDATA%\QuatschMitSosse\models, Daten unter %LOCALAPPDATA%\QuatschMitSosse\data.
- Leistungsziele: Leerlauf unter 100 MB RAM ohne geladenes Modell, unter 1 % CPU, Start unter 2 Sekunden.
