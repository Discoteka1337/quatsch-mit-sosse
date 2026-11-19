# Quatsch mit Soße

Deutsche, offline-first Diktier-App für Windows 11. Geplant sind lokale Spracherkennung und Textbereinigung sowie optionale, ausdrücklich aktivierte KI-Anbieter. Die Desktop-App verwendet Tauri 2, Rust und native Windows-Steuerelemente. Das Svelte-/TypeScript-Gerüst bleibt separat erhalten.

**Status: Schritt 1.3 – Spracherkennung und Sprachaktivität.** WASAPI-Aufnahmen mit 16 kHz mono können lokal mit beiden Parakeet-Modellen oder Whisper erkannt werden. Silero erkennt Sprachabschnitte; lange Aufnahmen werden an Pausen in höchstens 20 Sekunden lange Stücke geteilt. Modelle laden erst bei Bedarf und werden nach standardmäßig fünf Minuten Leerlauf automatisch entladen. Aufnahme, Modellverwaltung und Erkennung sind über Kommandozeilenprogramme nutzbar; die Verbindung mit Tray und Hotkey folgt in späteren Schritten.

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
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
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
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --package models-cli -- list
cargo run --package models-cli -- install parakeet-v3-int8
cargo run --package models-cli -- install parakeet-primeline-de-int8
cargo run --package models-cli -- install silero-vad
```

Optional steht `cargo run --package models-cli -- install whisper-large-v3-turbo` bereit. Die Dateien liegen nach erfolgreicher Prüfung unter `%LOCALAPPDATA%\QuatschMitSosse\models\<id>`. Die CLI zeigt Downloadfortschritt und Prüfphasen an. Strg+C bricht ab; denselben Installationsbefehl erneut ausführen, um fortzusetzen. Auch nach einem Verbindungsabbruch bleiben Teildownloads erhalten. Unterstützt ein Server keine Bereichsanfragen, startet der betroffene Download erneut. Eine falsche SHA256-Prüfsumme entfernt sämtliche Downloads und die unvollständige Installation dieses Modells.

Während einer blockierten Netzwerkoperation kann ein Abbruch bis zu 30 Sekunden dauern. Für die Gesamtdauer großer Downloads besteht keine feste Zeitgrenze.

`cargo run --package models-cli -- remove <id>` entfernt das angegebene Modell einschließlich seiner Teildownloads. `list` arbeitet ohne Netzwerk. Die Bibliothek bietet `Manager::new`, `list`, `install`, `remove`, `is_installed` und `path_of`; Dateipfade werden erst für vollständig installierte Modelle zurückgegeben. Die synchrone Installation gehört bei einer späteren UI-Anbindung in einen Arbeits-Thread.

Downloadadressen, exakte Dateigrößen, SHA256-Werte, enthaltene Laufzeitdateien, Lizenzen und Namensnennungen stehen in [manifest.toml](crates/models/manifest.toml). Auch Weiterleitungen werden gegen die exakte Hostliste geprüft; neue CDN-Hosts benötigen eine geprüfte Manifeständerung. Herkunft und Hash-Nachweis beschreibt [Entscheidung 0008](docs/decisions/0008-modellquellen-und-installation.md); die Namensnennungen stehen zusätzlich in [NOTICE](NOTICE).

Manuell prüfen: ein gewünschtes Modell installieren, während des Downloads Strg+C drücken und denselben Befehl erneut starten. Danach muss `list` den Zustand „installiert“ anzeigen. Der systematische Qualitätsvergleich der Modelle ist erst ab Schritt 1.5 vorgesehen.

## Spracherkennung prüfen

Voraussetzung: Das jeweilige ASR-Modell und `silero-vad` sind mit `models-cli` installiert. Die Erkennung lädt nichts aus dem Netz. Die Entwicklerumgebung auch in einer bereits eingerichteten MSVC-Shell laden: Sie bereitet die von `sherpa-rs` benötigten DLL-Ausgabeordner vor. LLVM/Clang wird für die Rust-Bindings benötigt; beim ersten Build lädt die Abhängigkeit die SHA256-geprüfte native CPU-Laufzeit.

```powershell
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --package quatsch-mit-sosse-stt --example transcribe_file -- "testdata/private/record_test.wav" parakeet-v3-int8
```

Das Beispiel akzeptiert WAV mit **16 kHz mono**, als Float32 oder PCM16/24/32. Es schreibt das Ergebnis als UTF-8 nach `testdata/private/transcribe_file.txt` im Projektordner; eine vorhandene Ergebnisdatei wird ersetzt. Transkript und Audio erscheinen nie im Terminal oder in Logs. Im Terminal stehen nur Status und Verarbeitungsdauer einschließlich Laden und VAD. Die Ergebnisdatei bleibt wie die Aufnahme von Git ausgeschlossen.

Danach denselben Befehl mit `parakeet-primeline-de-int8` beziehungsweise optional `whisper-large-v3-turbo` starten und jeweils die Ergebnisdatei lesen. Manuell prüfen: Der gesprochene deutsche Text stimmt erkennbar, insbesondere der erste Laut und längere Sätze über die Stückgrenzen. Ein Qualitätsbenchmark ist noch nicht vorgesehen.

Die Bibliothek bietet `SpeechEngine::transcribe`, `Transcript { text, processing_duration }`, `TransducerEngine`, `WhisperEngine` und `create_engine`. `EngineConfig` stellt ASR-Threads (Standard 2) und `idle_timeout` (Standard 300 Sekunden) ein; `Duration::ZERO` entlädt nach jedem Aufruf. `with_manager` erlaubt einen eigenen Modellordner. Ein schlafender Helfer entlädt VAD und ASR ohne weiteren Aufruf; laufende Erkennung wird nicht unterbrochen. Stille lädt nur VAD, kein ASR-Modell. Die synchronen Aufrufe gehören später in einen Arbeits-Thread.

`language_hint: None` lässt die Sprache automatisch erkennen. Whisper übernimmt einen unterstützten Sprachcode (z. B. `de`); bei geändertem Hinweis wird dessen ASR-Modell neu geladen. Parakeet erkennt die Sprache selbst und akzeptiert unterstützte Hinweise ohne Zwangsumschaltung. Ungültige Hinweise werden vor dem nativen Aufruf abgewiesen. `SileroVad` bietet separat `speech_segments`, `has_speech`, `unload` und konfigurierbare VAD-Threads; seine halb offenen Sample-Intervalle beziehen sich auf die Originalaufnahme. Wer VAD separat verwendet, gibt es mit `unload` oder `Drop` frei; die STT-Engines übernehmen dessen Leerlaufverwaltung.

Modellfreie Tests laufen im regulären Prüflauf. Modelltests sind mit `#[ignore]` markiert und laden ebenfalls nichts herunter. Für ein installiertes Modell gezielt starten:

```powershell
cargo test --package quatsch-mit-sosse-vad --test installed_models -- --ignored --test-threads=1
cargo test --package quatsch-mit-sosse-stt --test installed_models parakeet_v3 -- --ignored --test-threads=1
cargo test --package quatsch-mit-sosse-stt --test installed_models parakeet_primeline -- --ignored --test-threads=1
cargo test --package quatsch-mit-sosse-stt --test installed_models whisper -- --ignored --test-threads=1
```

Die ASR-Tests verwenden standardmäßig `testdata/private/record_test.wav`; alternativ vorher `$env:QMS_TEST_WAV = "C:\Pfad\aufnahme.wav"` setzen. Fehlende Modelle oder Aufnahmen lassen den ausdrücklich gestarteten Test fehlschlagen. Details zu Aufteilung, Leerlauf, nativer Laufzeit und Telemetrieabschaltung stehen in [Entscheidung 0009](docs/decisions/0009-spracherkennung-und-sprachaktivitaet.md).

## Leerlauf und Startzeit messen

Die laufende App zuerst über **Beenden** schließen. Während der Messung die Einstellungen geschlossen lassen und keinen parallelen Build starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/measure-idle.ps1"
```

Das Skript baut die Release-App ohne Installer nach `C:\qms-target`, startet sie, wartet auf `QMS_READY`, lässt sie 60 Sekunden aufwärmen und misst anschließend mindestens 30 Sekunden. Bereits installierte Build-Abhängigkeiten werden offline verwendet. Mit `-SkipBuild` wird der vorhandene Release-Build verwendet; ein abweichender Release-Pfad lässt sich zusätzlich mit `-ExecutablePath "C:\Pfad\quatsch-mit-sosse.exe"` angeben.

Die Tabelle enthält Private Bytes, Working Set und CPU des gesamten erkannten App-Prozessbaums einschließlich aller erfassten Kindprozesse. MB bedeutet 1.000.000 Bytes; CPU ist auf alle logischen Prozessoren normiert. Speicherziele werden gegen das Maximum, das CPU-Ziel gegen den Mittelwert geprüft. Die App beendet sich danach regulär; erst nach bestätigtem Ende wird `docs/benchmarks/idle-JJJJ-MM-TT.json` geschrieben. Eine erneute Messung am selben Tag ersetzt diesen Tagesbericht.

`QMS_READY` bestätigt derzeit Tray und Ereignisschleife. Der Hotkey ist erst für Schritt 2.1 vorgesehen; diese Messung ist deshalb eine Basislinie für das Gerüst. Details zu Prozesszuordnung, Messgrenzen und Bereitschaft stehen in [Entscheidung 0004](docs/decisions/0004-leerlaufmessung.md). Die Leistungsoptimierung folgt in Schritt 8.2; Werte über dem Ziel sind jetzt kein fehlgeschlagener Funktionstest. Der vollständige Installer-Test ist noch nicht vorgesehen. Das Overlay bleibt unsichtbar und hat noch keine Bedienfunktion.

Lizenz: [Apache-2.0](LICENSE), Copyright 2026 Denis R. Hinweise zu Drittanbietern und Modellen werden in [NOTICE](NOTICE) gesammelt.
