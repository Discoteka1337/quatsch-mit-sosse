# Review zu Phase 1 · Audio und Spracherkennung

Geprüft wurden die Schritte 1.1 bis 1.6 auf Basis von Commit `ad323ad`. Die Schwere bezeichnet die Auswirkung vor der Korrektur.

## Hoch

**Modellpfade mit Umlauten oder ß konnten die Erkennung vollständig verhindern.** Bei einem gültigen Silero-Modell im Ordner „Modellordner mit Soße“ scheiterte das Laden in der nativen Windows-Bibliothek. Ein Windows-Benutzerprofil mit solchen Zeichen betrifft auch den Standard-Modellpfad. Die Windows-Inferenzprogramme deklarieren UTF-8 für ihre nativen Dateizugriffe. Der Integrationstest lädt echtes Silero aus einem solchen Ordner und verarbeitet synthetische Stille. [Integrationstest](../crates/stt/tests/installed_models.rs)

## Mittel

| Befund | Korrektur und Prüfung |
| --- | --- |
| Bei gleichzeitigem Treiberfehler und Aufnahmestopp konnte ein unterbrochenes Diktat zur Bestätigung gelangen. | Der Audioworker verarbeitet ausstehende Fehlermeldungen vor der Bestätigung. Ein Test steuert die Reihenfolge von Treiberereignis und Stopp. [Audiotests](../crates/audio/src/tests.rs) |
| Ein unterbrochener Versuch konnte die gesamte Recorder-Sitzung beenden. Bei einem Fehler der Konsolenausgabe nach dem Start konnte eine Aufnahme weiterlaufen. | Der Recorder beendet den laufenden Versuch auch bei einem Ausgabefehler. Nach einer Unterbrechung funktionieren Wiederholen, Überspringen und Beenden. Die Tests simulieren alle Bedienwege. [Recordertests](../tools/corpus-recorder/tests/corpus.rs) |
| Die Beispiele konnten bei einem Schreibfehler die letzte WAV oder das letzte Transkript verlieren. Beim Mikrofontest konnte sogar der Vorlauf allein die vorherige Zehnsekundenaufnahme ersetzen. | Die Beispiele schreiben zuerst eine temporäre Datei. Erst nach erfolgreichem Abschluss ersetzen sie das Ziel. Der Mikrofontest prüft zusätzlich die Audiodauer. Die Tests simulieren Schreibfehler und eine zu kurze Aufnahme. [WAV-Beispiel](../crates/audio/examples/record_test.rs) · [Transkript-Beispiel](../crates/stt/examples/transcribe_file.rs) |
| Die Installation öffnete einen Download nach der SHA256-Prüfung erneut. Beim nativen Modellstart konnte parallel eine andere CLI die benötigten Dateien entfernen. | Prüfung und Entpacken verwenden unter Windows dasselbe gegen Änderungen geschützte Dateihandle. Eine gemeinsame Lesesperre schützt die Modellinstallation während des Ladens. Tests prüfen gemeinsame Leser, konkurrierende Änderungen und die Freigabe nach dem letzten Leser. [Modelltests](../crates/models/src/tests.rs) · [STT-Tests](../crates/stt/src/engine.rs) |
| Bei einem Abbruch unmittelbar nach Veröffentlichung des Modells blieben große Downloads auf der Platte. Ein erneuter Installationsaufruf übersprang die Bereinigung. | Der Wiederanlauf räumt den Cache auch bei gültiger Installation auf. Ein Test prüft diesen Zustand ohne erneuten Download. [Modelltests](../crates/models/src/tests.rs) |
| Nach reiner Stille meldeten die Engines ein geladenes ASR-Modell, obwohl nur Silero im Speicher war. | Die Statusabfrage prüft den tatsächlichen Erkenner. Ein Integrationstest verwendet Silero bei fehlenden ASR-Dateien und fragt beide Engine-Typen ab. [Integrationstest](../crates/stt/tests/installed_models.rs) |
| Die Modell-CLI wiederholte unbekannte Eingaben in der Fehlerausgabe. Nicht als Unicode darstellbare Argumente konnten einen Panic auslösen. | Ungültige Eingaben erzeugen eine feste Fehlermeldung. CLI-Tests prüfen Exitstatus und Ausgaben mit synthetischen Eingaben. [CLI-Tests](../tools/models-cli/tests/cli.rs) |
| Git-Ignore-Regeln und Indexprüfungen erfassten externe ONNX-Gewichte, Modellarchive und die mitgelieferten Tokenlisten unvollständig. Ignorierte Zugangsdaten-Dateien konnten bei erzwungener Aufnahme die Indexprüfung passieren. | Die Regeln decken die Phase-1-Modellformate und die bereits ausgeschlossenen Namen für Zugangsdaten ab. Die Tests prüfen normale und erzwungene Git-Aufnahme an synthetischen Dateien sowie die Ausnahmen für öffentliche WAV-Testdaten und die Umgebungsvorlage. [Prüfregeltests](../scripts/check.Tests.ps1) · [Datenschutztests](../scripts/privacy-guard.Tests.ps1) |
| Eine zwischen zwei Modellläufen geänderte Referenz oder WAV konnte unbemerkt in den Modellvergleich eingehen. | Jeder Messprozess vergleicht seine Eingaben vor der Inferenz mit dem Korpusstand beim Start. Änderungen brechen den Lauf vor der Berichtsveröffentlichung ab. Der CLI-Test ändert Referenz und Audio getrennt bei gleicher Länge. [Benchmark-Tests](../tools/bench/tests/cli.rs) |

## Niedrig

Die offenen Punkte mit konkreten Prüfkriterien stehen in der [Aufgabenliste für Phase 1](todo.md#review-phase-1--20112026).

## Abdeckung und Grenzen

Für alle sechs Schritte liegen Implementierung und Tests vor. Der private Korpus enthält 40 Sprachreferenzen und zehn Aufnahmen mit leerer Referenz. Die Aufnahmen 002, 004 und 005 liegen unter den vorgesehenen zehn Sekunden. Alle drei zählen in der Messung mit. Die bekannte Abweichung beim ersten Wort wurde nach der Hörprüfung für den Stand von Schritt 1.6 akzeptiert. Das Ziel bleibt bei 20 von 20. [Benchmark mit Hörbefund](benchmarks/asr-2026-11-20.md#schritt-16--stille-und-satzanfänge)

In den geprüften Phase-1-Pfaden gab es keinen Befund für ausgehende Diktatinhalte, Bildschirmaufnahmen oder Telemetrie. Native Modelle werden bei Leerlauf freigegeben. Die einmal initialisierte ONNX-Laufzeit bleibt absichtlich bis zum Prozessende erhalten. Die Desktop-App bindet die Phase-1-Verarbeitung bisher nicht ein. Damit läuft aktuell keine Modellinferenz und kein Modelldownload auf ihrem UI-Thread. Ein echter Mikrofonwechsel bleibt ein manueller Hardwaretest.

## Nachprüfung vom 20.11.2026

Die zentrale Vollprüfung mit `scripts/check.ps1` war erfolgreich. Sie umfasste Rust-Formatierung, Clippy und Workspace-Tests, die Svelte- und UI-Tests, den Datenschutz-Wächter und die nativen Windows-Prüfungen.

Der Release-Benchmark verarbeitete alle 50 Paare mit zwei ASR-Threads. Die Zeitspalte zeigt je Modell den höchsten Median der Zehnsekundenproben.

| Modell | Wortfehlerrate | 10 s Audio | Phantomtexte | Korrektes erstes Wort |
| --- | ---: | ---: | ---: | ---: |
| primeLine Deutsch | 21,81 % | 0,976 s | 0/10 | 15/20 |
| Parakeet v3 | 28,46 % | 0,974 s | 0/10 | 11/20 |

Beide Modelle erfüllen die Zeitgrenze von 1,5 Sekunden. Die niedrigere Wortfehlerrate bestätigt primeLine als Standardmodell. Die gesonderte Korpusabnahme mit `--check-protection` meldete wegen der Satzanfänge Exitcode 1. In einem temporären Windows-Profil mit Umlauten und ß erkannten beide Modelle alle acht Wörter einer lokal erzeugten Sprachprobe. Whisper war für die Nachprüfung nicht installiert.
