# Quatsch mit Soße

Deutsche, offline-first Diktier-App für Windows 11. Geplant sind lokale Spracherkennung und Textbereinigung sowie optionale, ausdrücklich aktivierte KI-Anbieter. Die technische Grundlage bilden Tauri 2, Rust und Svelte 5 mit TypeScript.

**Status: Schritt 0.1 – technisches Gerüst.** Der Cargo-Workspace enthält die zwölf Library-Crates und die Tauri-App. Tray, verstecktes Einstellungsfenster und Overlay-Platzhalter sind vorhanden. Diktieren und alle weiteren Fachfunktionen folgen in späteren Schritten.

Der technische Name lautet `quatsch-mit-sosse`. Architektur und nächste Schritte stehen im [Masterplan](docs/masterplan.md); die [Regeln für Coding-Agenten](AGENTS.md) gelten für die Entwicklung.

## Entwicklung starten

Voraussetzungen: Windows 11 x64, Rust mit MSVC-Toolchain, Visual Studio 2022 Build Tools mit Desktop-C++ und Windows SDK, Node.js LTS mit npm sowie WebView2 (siehe Vorbereitung im Masterplan).

Im Projektordner einmal die npm-Abhängigkeiten installieren:

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
npm --prefix "app/ui" ci --no-audit --no-fund
```

Danach die App im Entwicklungsmodus starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/dev.ps1"
```

Das Skript lädt die MSVC-Umgebung, setzt `CARGO_TARGET_DIR` auf `C:\qms-target` und startet Vite mit der Tauri-App. Der Entwicklungsserver ist nur unter `127.0.0.1:1420` erreichbar. Rust- und npm-Abhängigkeiten benötigen beim ersten Einrichten einen Download; die Gerüst-App verwendet keinen externen Dienst. In einer bereits vorbereiteten Entwickler-Shell funktioniert auch `npm --prefix "app/ui" run tauri -- dev`.

Beide Fenster starten unsichtbar. Das Tray-Menü **Einstellungen** öffnet den deutschen Platzhalter. Das Schließen zerstört das Einstellungsfenster samt WebView; beim nächsten Öffnen wird es neu erstellt. **Beenden** beendet die App. Ein weiterer App-Start öffnet die Einstellungen der vorhandenen Instanz.

## Prüfungen für Schritt 0.1

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/check.ps1" -Stage 0.1
```

Der Prüflauf umfasst die Pfad-, Ignore- und Prüfregel-Regressionen, Git-Whitespace/Index, `cargo fmt --all --check`, Clippy ohne Warnungen, alle Workspace-Tests sowie `npm run check` und `npm test`. Den UI-Produktionsbuild prüft `npm --prefix "app/ui" run build`.

Manuell prüfen:

- Tray-Symbol und Tooltip „Quatsch mit Soße“ erscheinen; beim Start öffnet sich kein Fenster.
- **Einstellungen** zeigt deutschen Text. Schließen und erneutes Öffnen funktioniert mehrfach; ein minimiertes Fenster wird wiederhergestellt.
- Ein zweiter Start erzeugt keine zweite App-Instanz.
- **Beenden** entfernt das Tray-Symbol und beendet den Prozess.

Der Datenschutz-Wächter und CI sind erst ab Schritt 0.2 vorgesehen. Leistungsziele und der vollständige Installer werden in den späteren Schritten gemessen beziehungsweise abgenommen. Das Overlay bleibt in Schritt 0.1 unsichtbar und hat noch keine Bedienfunktion.

Lizenz: [Apache-2.0](LICENSE), Copyright 2026 Denis R. Hinweise zu Drittanbietern und Modellen werden in [NOTICE](NOTICE) gesammelt.
