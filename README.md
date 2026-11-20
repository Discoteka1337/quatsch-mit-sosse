# Quatsch mit Soße

Deutsche, offline-first Diktier-App für Windows 11. Geplant sind lokale Spracherkennung und Textbereinigung sowie optionale, ausdrücklich aktivierte KI-Anbieter. Die Desktop-App verwendet Tauri 2, Rust und native Windows-Steuerelemente. Das Svelte-/TypeScript-Gerüst bleibt separat erhalten.

**Stand Schritt 1.6 – Phantomtexte und Satzanfänge.** Meldet Silero keine Sprache, liefert die Engine sofort leeren Text. Vor dem ersten Sprachabschnitt bleiben bis zu 500 ms der Aufnahme erhalten. Der Nachfilter verwirft ein Diktat, wenn sein gesamter Text einer Floskel aus der [Sperrliste](crates/stt/phantoms.txt) entspricht. `tools/bench` prüft Stille und erste Wörter im privaten Korpus. Der Recorder führt durch 40 deutsche Diktate und zehn Aufnahmen ohne Sprache. WASAPI-Aufnahmen mit 16 kHz mono können lokal mit beiden Parakeet-Modellen oder Whisper erkannt werden. Lange Aufnahmen werden an Pausen in höchstens 20 Sekunden lange Stücke geteilt. Modelle laden erst bei Bedarf und werden nach standardmäßig fünf Minuten Leerlauf automatisch entladen. Aufnahme, Modellverwaltung und Erkennung sind über Kommandozeilenprogramme nutzbar. Die Verbindung mit Tray und Hotkey folgt in späteren Schritten.

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

Das Tray-Menü **Einstellungen** öffnet die Overlay-Optionen. Mit **Bereit-Punkt anzeigen** wird der sonst ausgeblendete Punkt sichtbar. Den Punkt oder die aktive Leiste mit der Maus an die gewünschte Stelle ziehen. Die App merkt sich die Position pro Monitor. **In Bildschirmaufnahmen verbergen** schaltet den Windows-Aufnahmeschutz für das Overlay ein. Einzelheiten stehen in [Entscheidung 0014](docs/decisions/0014-natives-diktat-overlay.md).

Das Schließen zerstört das Einstellungsfenster samt Steuerelementen. Ein weiterer App-Start öffnet die Einstellungen der vorhandenen Instanz. **Beenden** im Tray beendet die App.

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

Manuell prüfen: ein gewünschtes Modell installieren, während des Downloads Strg+C drücken und denselben Befehl erneut starten. Danach muss `list` den Zustand „installiert“ anzeigen. Den systematischen Qualitätsvergleich beschreibt der Abschnitt „ASR-Benchmark“.

## Spracherkennung prüfen

Der deutsche primeLine-Encoder wird beim Laden lokal in eine temporäre ONNX-Datei mit eingebetteten Gewichten umgewandelt. Dafür werden zusätzlich ungefähr 652 MB freier Platz im Modellordner benötigt; nach erfolgreichem Laden wird die Datei gelöscht. Hintergrund: [Entscheidung 0010](docs/decisions/0010-primeline-externe-onnx-gewichte.md).

Voraussetzung: Das jeweilige ASR-Modell und `silero-vad` sind mit `models-cli` installiert. Die Erkennung lädt nichts aus dem Netz. Die Entwicklerumgebung auch in einer bereits eingerichteten MSVC-Shell laden: Sie bereitet die von `sherpa-rs` benötigten DLL-Ausgabeordner vor. LLVM/Clang wird für die Rust-Bindings benötigt; beim ersten Build lädt die Abhängigkeit die SHA256-geprüfte native CPU-Laufzeit.

```powershell
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --package quatsch-mit-sosse-stt --example transcribe_file -- "testdata/private/record_test.wav" parakeet-v3-int8
```

Das Beispiel akzeptiert WAV mit **16 kHz mono**, als Float32 oder PCM16/24/32. Es schreibt das Ergebnis als UTF-8 nach `testdata/private/transcribe_file.txt` im Projektordner; eine vorhandene Ergebnisdatei wird ersetzt. Transkript und Audio erscheinen nie im Terminal oder in Logs. Im Terminal stehen nur Status und Verarbeitungsdauer einschließlich Laden und VAD. Die Ergebnisdatei bleibt wie die Aufnahme von Git ausgeschlossen.

Danach denselben Befehl mit `parakeet-primeline-de-int8` beziehungsweise optional `whisper-large-v3-turbo` starten und jeweils die Ergebnisdatei lesen. Manuell prüfen: Der gesprochene deutsche Text stimmt erkennbar, insbesondere der erste Laut und längere Sätze über die Stückgrenzen. Der Qualitätsvergleich läuft über `tools/bench`.

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

## Testaufnahmen erstellen

Der Korpus-Recorder benötigt keine Modelle und kein Netzwerk. In einem interaktiven Windows-Terminal starten:

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --package corpus-recorder
```

Die lokale UTF-8-Datei `testdata/private/sentences.txt` enthält genau 40 Zeilen ohne Überschriften oder Leerzeilen. Sie wurde für diesen Schritt angelegt und bleibt wie alle Aufnahmen von Git ausgeschlossen. Die Einträge haben je 35 bis 41 Wörter, etwa 17 bis 20 Sekunden bei 126 Wörtern pro Minute; tatsächliche Sprechdauer und ausgesprochen lange Zahlen können abweichen. Nach einem frischen Git-Clone muss diese private Datei aus der eigenen lokalen Sicherung bereitgestellt werden. Während eines begonnenen Durchlaufs die Reihenfolge der Sätze nicht verändern.

| Nummern | Inhalt |
| --- | --- |
| 001–015 | KI-Prompts mit englischen Fachwörtern |
| 016–023 | Alltagsnachrichten |
| 024–029 | E-Mails im Sie-Ton |
| 030–034 | Zahlen, Datum, Uhrzeit, Prozent und Euro |
| 035–040 | Selbstkorrekturen und Füllwörter |
| 041–045 | Stille, ohne Sprache oder Tippen |
| 046–050 | Nur Tastaturgeräusche, ohne Sprache |

Das Programm öffnet sichtbar das Mikrofon und wartet auf 500 ms empfangenes Audio. Der Vorlauf bleibt während der Sitzung aktiv. Pro Aufnahme:

1. Den angezeigten Satz lesen; **Enter** startet, anschließend natürlich diktieren.
2. **Enter** stoppt und zeigt die Dauer einschließlich Vorlauf.
3. **Enter** bestätigt die Speicherung und geht weiter. **W** verwirft den Versuch und zeigt denselben Satz erneut; **S** verwirft ihn und überspringt seine Nummer. W und S benötigen kein zusätzliches Enter.

Vor dem Start überspringt S ebenfalls. Während einer Sprach- oder Stilleaufnahme können W und S direkt verwerfen; bei einer laufenden Tastaturaufnahme bleiben beide normale Buchstaben, damit freies Tippen möglich ist. Hier beendet Enter die Aufnahme, danach stehen W und S wieder zur Verfügung. Escape oder Strg+C beendet die Sitzung und verwirft einen unbestätigten Versuch. Das Mikrofon wird auch im Fehlerfall freigegeben.

Für Stille und Tastaturgeräusche erscheint jeweils eine eigene Ansage: 10 bis 20 Sekunden ohne Sprache aufnehmen, die Ansage nicht mitsprechen. Diese zehn Aufnahmen haben eine **vollständig leere TXT-Referenz**. Die Sprachaufnahmen sollten 10 bis 40 Sekunden dauern; außerhalb dieses Bereichs erscheint ein Hinweis, die Speicherung bleibt möglich.

Gespeichert wird unter `testdata/private/corpus/NNN.wav` (16 kHz mono, Float32, mit Vorlauf) und `NNN.txt` (UTF-8, exakt der angezeigte Satz). Audio und Referenzen werden nicht protokolliert; die Satzanzeige erfolgt nur in der interaktiven Konsole. Umleitungen und Pipes werden vor Mikrofonöffnung abgewiesen. Die Satzliste bleibt ebenfalls privat.

Bereits vorhandene Paare werden beim erneuten Start ausgelassen und niemals überschrieben; übersprungene Nummern werden wieder angeboten. Ein einzelnes WAV oder TXT ohne Partner führt zu einem Fehler vor Mikrofonöffnung: die betroffene Datei zunächst prüfen und privat sichern. Für die spätere Wiederholung einer bereits bestätigten Aufnahme beide Dateien des betroffenen Paars privat sichern und aus dem Korpusordner entfernen, dann erneut starten. Details stehen in [Entscheidung 0011](docs/decisions/0011-korpus-recorder.md).

Ein anderes Mikrofon wählen oder die Hilfe anzeigen:

```powershell
cargo run --package corpus-recorder -- --device "Exakter Mikrofonname"
cargo run --package corpus-recorder -- --help
```

Manuell prüfen: Alle 50 Aufnahmen einsprechen beziehungsweise aufnehmen, einige WAV-Dateien anhören und besonders auf vollständige Wortanfänge achten. W, S und einen Sitzungsneustart ausprobieren. Wenn die gesprochene Formulierung abweicht, die zugehörige TXT-Datei anpassen. Bei 041–050 müssen die Referenzdateien leer bleiben und die WAV-Dateien dürfen keine Sprache enthalten. Anschließend prüfen, dass Windows das Mikrofon als freigegeben anzeigt.

## ASR-Benchmark

`tools/bench` liest sämtliche nummerierten WAV/TXT-Paare aus dem privaten Korpus
und misst jedes vollständig installierte ASR-Modell über die vorhandene
`SpeechEngine` einschließlich Silero-VAD. Es lädt keine Modelle herunter.
Andere rechenintensive Programme und Builds während der Messung pausieren.

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --offline --release --package bench -- --date "2026-11-20"
```

Ohne `--date` wird das lokale Tagesdatum verwendet. Optional sind `--threads N`
(Standard: 2), `--corpus "C:\Pfad\Korpus"` und `--output "C:\Pfad\Berichte"`.
`--help` zeigt die Optionen. Der Bericht `docs/benchmarks/asr-JJJJ-MM-TT.md`
wird erst nach erfolgreicher Messung aller installierten Modelle atomar
bereitgestellt; ein vorhandener Tagesbericht wird ersetzt. Ein fehlendes Paar,
ungültiges WAV oder ein Modellfehler lässt den Lauf fehlschlagen.

Je Modell startet ein eigener Prozess: Kaltstart, vollständige Aufnahmen und
je drei Wiederholungen mittiger Zehnsekunden-Ausschnitte aus allen ausreichend
langen Sprachaufnahmen. Die Zeitgrenze ist streng: Der höchste Median muss
unter 1,5 Sekunden liegen; mindestens eine Probe muss dreimal nicht leeren
Text liefern. Auch leere Probeausgaben bleiben in den Zeitmessungen und der
WER der vollständigen Aufnahme enthalten.
Unter den geeigneten Modellen entscheidet die niedrigste gepoolte WER.
Die genauen Messgrenzen, Einzelwerte, RAM-Spitze und Phantomzahlen stehen im
Bericht. Die Speicherzahl enthält den gesamten Messprozess samt Korpus und
Modellladen; sie ist kein Leerlaufwert der App.

Satztexte, Audio und Tokenlisten werden weder im Terminal noch im Bericht oder
in temporären Messdateien ausgegeben. Der private Korpus bleibt unverändert.
Die Modellentscheidung steht in
[0001-standardmodell.md](docs/decisions/0001-standardmodell.md).
Für die Abnahme von Schritt 1.6 kann der neue Bericht getrennt vom
veröffentlichten Modellvergleich in einem lokalen Ordner gespeichert werden.

```powershell
cargo run --locked --offline --release --package bench -- --check-protection --output "$env:TEMP\qms-asr-1-6"
```

Der Bericht enthält das Ergebnis für jede Aufnahme mit leerer Referenz und
die Trefferzahl für das erste Wort der ersten 20 Sprachreferenzen nach Nummer.
Die Abnahme liefert nach dem Schreiben des Berichts Exitcode 1, falls ein
gemessenes Modell eines der beiden Ziele verfehlt oder Daten für die Prüfung fehlen.
Zum Nachprüfen die aufgeführten Fehlernummern lokal anhören und mit dem ersten
Wort der jeweiligen TXT-Referenz vergleichen.

## Leerlauf und Startzeit messen

Die laufende App zuerst über **Beenden** schließen. Während der Messung die Einstellungen geschlossen lassen und keinen parallelen Build starten:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/measure-idle.ps1"
```

Das Skript baut die Release-App ohne Installer nach `C:\qms-target`, startet sie, wartet auf `QMS_READY`, lässt sie 60 Sekunden aufwärmen und misst anschließend mindestens 30 Sekunden. Bereits installierte Build-Abhängigkeiten werden offline verwendet. Mit `-SkipBuild` wird der vorhandene Release-Build verwendet; ein abweichender Release-Pfad lässt sich zusätzlich mit `-ExecutablePath "C:\Pfad\quatsch-mit-sosse.exe"` angeben.

Die Tabelle enthält Private Bytes, Working Set und CPU des gesamten erkannten App-Prozessbaums einschließlich aller erfassten Kindprozesse. MB bedeutet 1.000.000 Bytes; CPU ist auf alle logischen Prozessoren normiert. Speicherziele werden gegen das Maximum, das CPU-Ziel gegen den Mittelwert geprüft. Die App beendet sich danach regulär; erst nach bestätigtem Ende wird `docs/benchmarks/idle-JJJJ-MM-TT.json` geschrieben. Eine erneute Messung am selben Tag ersetzt diesen Tagesbericht.

`QMS_READY` bestätigt derzeit Tray und Ereignisschleife. Die Basismessung aus Schritt 0.3 erfolgte mit ausgeblendetem Overlay. Die Anbindung an den Diktatablauf folgt in Schritt 2.4, die Leistungsoptimierung in Schritt 8.2. Der vollständige Installer-Test ist noch nicht vorgesehen. Details zu Prozesszuordnung, Messgrenzen und Bereitschaft stehen in [Entscheidung 0004](docs/decisions/0004-leerlaufmessung.md).

Lizenz: [Apache-2.0](LICENSE), Copyright 2026 Denis R. Hinweise zu Drittanbietern und Modellen werden in [NOTICE](NOTICE) gesammelt.
