# 0001: Prüfpflicht nach Bauschritt

Datum: 17.11.2026

## Ursache

Der Masterplan verlangt nach V5 einen Commit, legt den Cargo-Workspace und die UI aber erst in Schritt 0.1 sowie den Datenschutz-Wächter erst in Schritt 0.2 an. Die bisher uneingeschränkte Prüfpflicht in AGENTS.md und im Commit-Prompt blockierte deshalb den vorgesehenen Abschluss der Vorbereitung.

## Entscheidung

Die Prüfpflicht folgt dem im Masterplan vorgesehenen Bauschritt. Anwendungsteile werden in ihren vorgesehenen Schritten angelegt.

| Stand | Prüflauf | Pflicht |
| --- | --- | --- |
| V4 | Datei-, Ignore- und Git-Index-Prüfung | Projektdateien und Ausschluss privater Daten |
| V5 | `powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/check.ps1" -Stage V5` | Prüfregel- und Pfad-Regressionstests, Git-Whitespace und verbotene Dateien im Index |
| Schritt 0.1 | Derselbe Prüflauf mit `-Stage 0.1` | Zusätzlich Cargo fmt, Clippy, Cargo-Tests, UI-Check und UI-Tests |
| Ab Schritt 0.2 | Derselbe Prüflauf ohne `-Stage` | Zusätzlich der vollständige Datenschutz-Wächter |

Ohne Stufenangabe gilt immer die Vollprüfung. Noch nicht vorgesehene Prüfungen erhalten diesen Status und werden nicht als bestanden dargestellt. Eine fehlende Pflichtdatei ist weiterhin ein Fehler.

Die reduzierten Prüfmodi sind an V5 beziehungsweise Schritt 0.1 gebunden. Der Prüflauf sperrt sie, sobald Artefakte eines späteren Schritts im Arbeitsverzeichnis, im Git-Index oder in der Git-Historie auftauchen. Das Löschen eines Manifests kann somit keinen früheren Prüfmodus freigeben. Der Git-Index wird auch während der Vorbereitung auf private Testdaten, Modelle und nicht freigegebene WAV-Dateien geprüft; dies ersetzt nicht den vollständigen Datenschutz-Wächter ab 0.2.

## Nachweis

`scripts/check.Tests.ps1` prüft in einem isolierten temporären Git-Repository die Stufengrenzen, fehlende Pflichtdateien, verbotene Indexdateien einschließlich Dateinamen mit ß und gelöschte Manifeste. Die synthetischen Cargo-/npm-Dateien werden nur zur Prüfung der Voraussetzungen verwendet; mit `-PlanOnly` werden keine vermeintlichen Builds oder Erfolgsskripte ausgeführt. Der echte Rust-/npm-Pfadtest läuft zusätzlich über `scripts/path-check.Tests.ps1`.

Die Vorbereitungsprüfungen bestätigen noch keinen vollständigen Tauri-Build und keine vollständige Datenschutzprüfung des späteren Anwendungscodes.
