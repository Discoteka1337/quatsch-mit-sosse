# Quatsch mit Soße

Deutsche, offline-first Diktier-App für Windows 11. Geplant sind lokale Spracherkennung und Textbereinigung sowie optionale, ausdrücklich aktivierte KI-Anbieter. Die Desktop-App verwendet Tauri 2, Rust und native Windows-Steuerelemente. Das Svelte-/TypeScript-Gerüst bleibt separat erhalten.

**Status: Schritt 1.2 – Modell-Manager.** Die Audio-Crate bietet WASAPI-Aufnahme mit 16 kHz mono, 500 ms Vorlauf, RMS-Pegel und Rückfall bei Geräteausfällen. Der Modell-Manager installiert die vier geprüften ASR-/VAD-Modelle mit SHA256-Prüfung und fortsetzbaren Downloads. Aufnahme und Modellverwaltung sind über Kommandozeilenprogramme nutzbar; die Verbindung mit Tray, Hotkey und Spracherkennung folgt in späteren Schritten.

**WebView2-Diagnostikpfad entfernt.** Einstellungen und Overlay sind native Fenster; die App startet weder WebView2 noch dessen SDK-Versionsabfrage. Dafür wird eine eng begrenzte, geprüfte Tauri-Abhängigkeitskorrektur verwendet. Entscheidung, Tests und Wartung stehen in [Entscheidung 0006](docs/decisions/0006-native-fenster-ohne-webview2.md). Systemweite Windows-Diagnostik wird dadurch nicht verändert.

Der technische Name lautet `quatsch-mit-sosse`. Architektur und nächste Schritte stehen im [Masterplan](docs/masterplan.md); die [Regeln für Coding-Agenten](AGENTS.md) gelten für die Entwicklung.

## Entwicklung starten

Voraussetzungen: Windows 11 x64, Rust mit MSVC-Toolchain, Visual Studio 2022 Build Tools mit Desktop-C++ und Windows SDK sowie Node.js LTS mit npm (siehe Vorbereitung im Masterplan). Die native App benötigt keine installierte WebView2-Laufzeit.

Im Projektordner einmal die npm-Abhängigkeiten installieren:

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
npm --prefix "app/ui" ci --no-audit --no-fund
```

Danach die App im Entwicklungsmodus starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/dev.ps1"
```

Das Skript lädt die MSVC-Umgebung, setzt `CARGO_TARGET_DIR` auf `C:\qms-target` und startet Vite mit der Tauri-App. Der Entwicklungsserver für das separate Svelte-Gerüst ist nur unter `127.0.0.1:1420` erreichbar; die Desktop-App zeigt native Steuerelemente. Rust- und npm-Abhängigkeiten benötigen beim ersten Einrichten einen Download. Der Modell-Manager lädt ausschließlich auf ausdrücklichen Installationsaufruf von den freigegebenen Modellhosts. In einer bereits vorbereiteten Entwickler-Shell funktioniert auch `npm --prefix "app/ui" run tauri -- dev`.

Beide Fenster starten unsichtbar. Das Tray-Menü **Einstellungen** öffnet den deutschen Platzhalter. Das Schließen zerstört das Einstellungsfenster samt nativen Steuerelementen; beim nächsten Öffnen wird es neu erstellt. **Beenden** beendet die App. Ein weiterer App-Start öffnet die Einstellungen der vorhandenen Instanz.

## Prüfungen ab Schritt 0.2

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/check.ps1"
```

Der Prüflauf umfasst die Pfad-, Ignore-, Prüfregel-, CI-, Datenschutz- und Messskript-Regressionen, Git-Whitespace/Index, `cargo fmt --all --check`, Clippy ohne Warnungen, alle Workspace-Tests sowie `npm run check` und `npm test`. Hinzu kommen der exakte Tauri-Patchvergleich, der UI-Produktionsbuild und die native Fenster-/Prozessprüfung mit einem frisch gebauten App-Programm. Frühere Prüfmodi sind ab diesem Stand gesperrt.

GitHub Actions führt bei Push und Pull Request sowie auf manuellen Start denselben vollständigen Prüflauf auf `windows-latest` aus. Zuvor installiert `npm ci` die gesperrten UI-Abhängigkeiten; Cargo- und npm-Caches beschleunigen Folgeläufe. Nach dem Push muss der Workflow unter **Actions** grün sein.

Den Datenschutz-Wächter und seine isolierten Negativtests kann man auch einzeln starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/privacy-guard.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/privacy-guard.Tests.ps1"
```

Der Wächter blockiert private Testdaten, Modellgewichte und nicht freigegebene WAV-Dateien im Git-Index. Er prüft Quell- und Abhängigkeitsdateien im Index und in der Arbeitskopie auf Telemetrie, Bildschirmaufnahme und HTTP-Clients außerhalb der beiden vorgesehenen `net.rs`-Module. Die Negativtests verwenden ausschließlich synthetische Inhalte in temporären Git-Repositories. Prüfbereich und Grenzen stehen in [Entscheidung 0003](docs/decisions/0003-datenschutz-waechter-und-ci.md).

Manuell prüfen:

- Tray-Symbol und Tooltip „Quatsch mit Soße“ erscheinen; beim Start öffnet sich kein Fenster.
- **Einstellungen** zeigt deutschen Text. Schließen und erneutes Öffnen funktioniert mehrfach; ein minimiertes Fenster wird wiederhergestellt.
- Ein zweiter Start erzeugt keine zweite App-Instanz.
- **Beenden** entfernt das Tray-Symbol und beendet den Prozess.

## Mikrofonaufnahme prüfen

Im Projektordner die Entwicklerumgebung laden und das Beispiel starten:

```powershell
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
$env:CARGO_TARGET_DIR = "C:\qms-target"
cargo run --package quatsch-mit-sosse-audio --example record_test
```

Das Beispiel aktiviert sichtbar die Mikrofonbereitschaft und füllt den Vorlauf. Sobald es bereit ist, Enter drücken und direkt sprechen. Nach zehn Sekunden endet die Aufnahme und das Mikrofon wird freigegeben. Die Datei `testdata/private/record_test.wav` enthält 16 kHz mono als 32-Bit-Float, mit Vorlauf ungefähr 10,5 Sekunden. Ein vorheriger Test wird überschrieben. Die Datei bleibt privat und wird von Git ignoriert.

Geräte ohne Aufnahme auflisten oder ein Mikrofon über seinen exakten Namen wählen:

```powershell
cargo run --package quatsch-mit-sosse-audio --example record_test -- --list
cargo run --package quatsch-mit-sosse-audio --example record_test -- --device "Exakter Mikrofonname"
```

Manuell prüfen: WAV anhören, insbesondere den ersten Laut direkt beim Start. Außerdem während einer Aufnahme das gewählte USB-Mikrofon abziehen beziehungsweise das Windows-Standardmikrofon wechseln: Das Beispiel meldet den Wechsel und verwendet das verfügbare Standardgerät weiter. Ohne Ersatzgerät meldet es den Ausfall und versucht erneut eine Verbindung. Die Ausfallzeit kann nicht aufgenommen werden. Nach Programmende muss Windows das Mikrofon als freigegeben anzeigen.

Die API bietet `input_devices`, `default_input_device`, `AudioCapture::start`, `select_device`, `start_recording`, `finish_recording` und `stop_ready`. `finish_recording` lässt den Vorlauf aktiv; erst `stop_ready` oder `Drop` beendet sämtliche Audio-Threads. Details zu Kaltstart, Filterlatenz und Rückfall stehen in [Entscheidung 0007](docs/decisions/0007-mikrofonaufnahme-und-vorlauf.md). Die automatischen Tests benötigen kein Mikrofon und erzeugen keine privaten Aufnahmen.

## Modelle verwalten

Im Projektordner die Entwicklerumgebung laden und die verfügbaren Modelle anzeigen:

```powershell
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
$env:CARGO_TARGET_DIR = "C:\qms-target"
cargo run --package models-cli -- list
cargo run --package models-cli -- install parakeet-v3-int8
cargo run --package models-cli -- install parakeet-primeline-de-int8
cargo run --package models-cli -- install silero-vad
```

Optional steht `cargo run --package models-cli -- install whisper-large-v3-turbo` bereit. Die Dateien liegen nach erfolgreicher Prüfung unter `%LOCALAPPDATA%\QuatschMitSosse\models\<id>`. Die CLI zeigt Downloadfortschritt und Prüfphasen an. Strg+C bricht ab; denselben Installationsbefehl erneut ausführen, um fortzusetzen. Auch nach einem Verbindungsabbruch bleiben Teildownloads erhalten. Unterstützt ein Server keine Bereichsanfragen, startet der betroffene Download erneut. Eine falsche SHA256-Prüfsumme entfernt sämtliche Downloads und die unvollständige Installation dieses Modells.

Während einer blockierten Netzwerkoperation kann ein Abbruch bis zu 30 Sekunden dauern. Für die Gesamtdauer großer Downloads besteht keine feste Zeitgrenze.

`cargo run --package models-cli -- remove <id>` entfernt das angegebene Modell einschließlich seiner Teildownloads. `list` arbeitet ohne Netzwerk. Die Bibliothek bietet `Manager::new`, `list`, `install`, `remove`, `is_installed` und `path_of`; Dateipfade werden erst für vollständig installierte Modelle zurückgegeben. Die synchrone Installation gehört bei einer späteren UI-Anbindung in einen Arbeits-Thread.

Downloadadressen, exakte Dateigrößen, SHA256-Werte, enthaltene Laufzeitdateien, Lizenzen und Namensnennungen stehen in [manifest.toml](crates/models/manifest.toml). Auch Weiterleitungen werden gegen die exakte Hostliste geprüft; neue CDN-Hosts benötigen eine geprüfte Manifeständerung. Herkunft und Hash-Nachweis beschreibt [Entscheidung 0008](docs/decisions/0008-modellquellen-und-installation.md); die Namensnennungen stehen zusätzlich in [NOTICE](NOTICE).

Manuell prüfen: ein gewünschtes Modell installieren, während des Downloads Strg+C drücken und denselben Befehl erneut starten. Danach muss `list` den Zustand „installiert“ anzeigen. Spracherkennung und ein Qualitätsvergleich der Modelle sind erst ab Schritt 1.3 beziehungsweise 1.5 vorgesehen.

## Leerlauf und Startzeit messen

Die laufende App zuerst über **Beenden** schließen. Während der Messung die Einstellungen geschlossen lassen und keinen parallelen Build starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/measure-idle.ps1"
```

Das Skript baut die Release-App ohne Installer nach `C:\qms-target`, startet sie, wartet auf `QMS_READY`, lässt sie 60 Sekunden aufwärmen und misst anschließend mindestens 30 Sekunden. Bereits installierte Build-Abhängigkeiten werden offline verwendet. Mit `-SkipBuild` wird der vorhandene Release-Build verwendet; ein abweichender Release-Pfad lässt sich zusätzlich mit `-ExecutablePath "C:\Pfad\quatsch-mit-sosse.exe"` angeben.

Die Tabelle enthält Private Bytes, Working Set und CPU des gesamten erkannten App-Prozessbaums einschließlich aller erfassten Kindprozesse. MB bedeutet 1.000.000 Bytes; CPU ist auf alle logischen Prozessoren normiert. Speicherziele werden gegen das Maximum, das CPU-Ziel gegen den Mittelwert geprüft. Die App beendet sich danach regulär; erst nach bestätigtem Ende wird `docs/benchmarks/idle-JJJJ-MM-TT.json` geschrieben. Eine erneute Messung am selben Tag ersetzt diesen Tagesbericht.

`QMS_READY` bestätigt derzeit Tray und Ereignisschleife. Der Hotkey ist erst für Schritt 2.1 vorgesehen; diese Messung ist deshalb eine Basislinie für das Gerüst. Details zu Prozesszuordnung, Messgrenzen und Bereitschaft stehen in [Entscheidung 0004](docs/decisions/0004-leerlaufmessung.md). Die Leistungsoptimierung folgt in Schritt 8.2; Werte über dem Ziel sind jetzt kein fehlgeschlagener Funktionstest. Der vollständige Installer-Test ist noch nicht vorgesehen. Das Overlay bleibt unsichtbar und hat noch keine Bedienfunktion.

Lizenz: [Apache-2.0](LICENSE), Copyright 2026 Denis R. Hinweise zu Drittanbietern und Modellen werden in [NOTICE](NOTICE) gesammelt.
