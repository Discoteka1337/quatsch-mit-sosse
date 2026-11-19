# 0003: Datenschutz-Wächter und gemeinsame CI-Prüfung

Datum: 17.11.2026 · Bauschritt 0.2

## Prüfbereich

`scripts/privacy-guard.ps1` ist eine lokale, statische Prüfung ohne zusätzliche Pakete oder Netzwerkzugriff. Standardmäßig prüft sie das Repository oberhalb des Skripts. `-RepositoryRoot` erlaubt die Prüfung eines anderen, eigenständigen Git-Repositories und wird für isolierte Regressionstests verwendet. Ein Verstoß oder ein nicht vollständig lesbarer Prüfbestand endet mit Exitcode 1.

Verbotene Dateipfade werden am tatsächlichen Git-Index geprüft, auch wenn eine Datei ignoriert oder bereits aus der Arbeitskopie gelöscht wurde. Die WAV-Ausnahme gilt nur unmittelbar in `testdata/public/`, nicht in Unterordnern. Private Dateien und Audio-/Modellinhalte werden dafür nicht gelesen.

Quellcode, Skripte und Konfigurationen werden sowohl aus den Git-Blobs im Index als auch aus der Arbeitskopie gelesen. Noch nicht versionierte, nicht ignorierte Quellen zählen ebenfalls dazu. Dadurch kann ein gestagter Verstoß nicht durch eine saubere Arbeitskopie verdeckt werden. Ignorierte, nicht versionierte Build-Ausgaben und Abhängigkeiten werden nicht durchsucht; eingecheckte Quellen werden auch bei später hinzugefügten Ignore-Regeln geprüft. Verknüpfte Codepfade werden abgewiesen, statt ihnen zu folgen.

Die Prüfung umfasst Rust, TypeScript/JavaScript, Svelte/Vue/HTML, PowerShell, übliche weitere Quellformate sowie TOML, JSON und YAML. Cargo- und npm-Manifeste samt Lockdateien werden auf Telemetrie geprüft. Die Beispiele `sentry`, `posthog`, `segment` und `mixpanel` sind konkrete Paketregeln; der normale Bezeichner `segment` ist weiterhin zulässig. Windows-Capture-Namensräume werden auch in Rust-Schreibweise und als `Graphics_Capture`-Feature erkannt.

HTTP-Abhängigkeitsdeklarationen in Manifesten und Lockdateien sind zulässig: Sie sind Voraussetzung für die späteren Netzwerkmodule, aber noch keine Verwendung im Anwendungscode. Verwendungen von `reqwest`, `ureq` und `hyper` sind ausschließlich in den exakt benannten Dateien `crates/models/src/net.rs` und `crates/llm/src/net.rs` zulässig. Cargo-Paketaliase werden aus Inline-Abhängigkeiten und Abhängigkeitstabellen ebenfalls berücksichtigt, einschließlich gleichwertiger gepunkteter und zitierter TOML-Schlüssel. Regressionen sichern diese Schreibweisen auch bei einem ausschließlich im Git-Index vorhandenen Verstoß ab. Diese Ausnahme gilt nicht für Telemetrie oder Bildschirmaufnahme.

Die beiden Dateien `scripts/privacy-guard.ps1` und `scripts/privacy-guard.Tests.ps1` enthalten die Verbotsmuster beziehungsweise synthetische Negativbeispiele und sind deshalb von der Inhaltsprüfung ausgenommen. Andere Skripte sind nicht pauschal ausgenommen. Dokumentation ist kein Anwendungscode. Änderungen an den Regeln und ihren Ausnahmen bleiben Teil des Code-Reviews.

## Aussagekraft

Die Prüfung meldet Regelkennung, Dateipfad, Zeilennummer und Herkunft (Index/Arbeitskopie), niemals die gefundene Quellzeile. Sie ist eine zusätzliche Sperre gegen bekannte verbotene Abhängigkeiten und Aufrufe, keine vollständige Programmanalyse oder Laufzeit-Netzwerküberwachung. Auch Kommentare und Zeichenketten mit den konkreten Verbotsmustern können einen Fund auslösen. SHA256-Prüfung von Modelldownloads und die ausdrückliche Aktivierung von KI-Anbietern müssen beim Bau der jeweiligen Netzwerkmodule zusätzlich umgesetzt und getestet werden.

## CI

Der GitHub-Workflow verwendet denselben vollständigen `scripts/check.ps1`-Prüflauf wie die lokale Entwicklung. Dazu gehören die neuen Datenschutz- und CI-Regressionstests; fehlende Pflichtdateien sind Fehler. `npm ci` läuft zuvor in `app/ui`. Die Caches enthalten npm-Downloads, Cargo-Abhängigkeiten und die Rust-Build-Ausgabe unter `C:\qms-target`.

Die [Windows-Runner](https://github.com/actions/runner-images#available-images) verwenden eine vollständige Visual-Studio-Edition; das aktuelle [Windows-2025-Image mit VS 2026](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-VS2026-Readme.md) enthält Enterprise. Die MSVC-Erkennung akzeptiert deshalb die passenden C++-Werkzeuge von VS 2022 und VS 2026 unabhängig von der Edition; die vorhandenen lokalen Build Tools bleiben unterstützt. Der reale GitHub-Lauf wird nach dem Push unter **Actions** kontrolliert. Die lokale Prüfung allein bestätigt keinen bereits ausgeführten GitHub-Lauf.
