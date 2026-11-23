# 0009 – Lokale Spracherkennung und Sprachaktivität

Datum: 19.11.2026 · Bauschritt: 1.3

## Entscheidung

`crates/stt` implementiert `SpeechEngine` mit `transcribe(&mut self, samples, sample_rate, language_hint) -> Result<Transcript>`. Das Ergebnis enthält Text und gemessene Verarbeitungsdauer einschließlich Kaltstart, VAD und aller ASR-Stücke. Die Schnittstelle nimmt endliche, normalisierte Mono-Samples mit 16 kHz entgegen; andere Raten werden bewusst abgewiesen. Das Audio-Modul aus Schritt 1.1 liefert dieses Format bereits. Ein zusätzlicher Resampler gehört nicht in diesen Bauschritt.

Die fest gesperrte Abhängigkeit ist `sherpa-rs = 0.6.8`, mit `download-binaries` und `sys`, ohne Standardfeatures, TTS oder GPU. Sie verwendet die CPU-Laufzeit sherpa-onnx 1.12.9. Beide Parakeet-Modelle verwenden `nemo_transducer`, `greedy_search`, 80 Merkmale und eine einstellbare Thread-Anzahl. Whisper ist implementiert, weil der geprüfte Turbo-Export im Manifest vorhanden ist. Modellpfade stammen ausschließlich vom Modell-Manager; die Erkennung installiert keine Modelle und führt keine Netzwerkaufrufe aus.

Whisper erhält bei fehlendem Sprachhinweis die leere Sprache für automatische Erkennung, nicht den englischen Default des Wrappers. Hinweise werden gegen die Sprachen des Modells geprüft: Die native Whisper-Implementierung kann bei unbekannten Sprachen den Prozess beenden. Ein Wechsel des Whisper-Hinweises verwirft zunächst das bisherige ASR-Modell und lädt danach mit der neuen Sprache. So liegen nicht zwei große Modelle gleichzeitig im Speicher. Parakeet unterstützt keinen solchen Zwangshinweis und erkennt seine Sprache selbst.

## Aufteilung und Lebensdauer

Silero verarbeitet die Aufnahme in überlappenden internen Analysefenstern von höchstens 32 Sekunden. Die Überlappung berücksichtigt Mindestsprach- und Mindeststilledauer; ein Reset pro Fenster verhindert die Übernahme von Zuständen aus früheren Aufnahmen. Das begrenzt den nativen Audiopuffer auch für lange Diktate. Erkannte Abschnitte werden auf die Originalaufnahme zurückgerechnet, um standardmäßig 100 ms erweitert, auf die Aufnahmegrenzen geklemmt und zusammengeführt. Das zusätzliche Padding ist konfigurierbar.

Die ASR-Aufteilung fasst Sprachintervalle einschließlich kurzer Zwischenpausen bis höchstens 20 Sekunden zusammen. Würde der nächste Abschnitt die Grenze überschreiten, endet das aktuelle Stück an der letzten vorhandenen Pause. Nur ein durchgehender Sprachabschnitt ohne Pause wird hart an der Grenze geteilt. Jedes Sprachsample bleibt genau einmal erhalten; angrenzende Intervalle gelten nicht als Pause. Die Stücke werden nacheinander erkannt und ihre nicht leeren Texte mit Leerzeichen verbunden. Führende und abschließende Stille wird entfernt; reine Stille liefert leeren Text, ohne ASR zu laden.

Konstruktoren speichern nur Konfiguration und starten einen schlafenden Leerlaufhelfer. Beim ersten nicht leeren Diktat wird Silero geladen, bei erkannter Sprache anschließend ASR. Die Engine hält beide gemeinsam in einer Sitzung. Ein Condvar-Timer entlädt nach standardmäßig 300 Sekunden ohne Nutzung tatsächlich auch dann, wenn kein weiterer Methodenaufruf erfolgt. Das Zeitintervall beginnt nach Ende der Verarbeitung, auch bei Verarbeitungsfehlern; während einer Erkennung bleibt das Modell gesperrt. `Drop` beendet den Helfer und gibt die Sitzung frei. `idle_timeout = 0` entlädt unmittelbar nach jedem Aufruf. Separat verwendetes `SileroVad` hat `unload`/`Drop`; die automatische Leerlaufverwaltung gehört zur STT-Engine.

Der sherpa-rs-VAD-Wrapper prüft in dieser Version den Erzeugungszeiger nicht und bietet keinen vollständigen Reset. Deshalb verwendet `crates/vad/src/native.rs` eine kleine gekapselte C-API-Anbindung über die `sys`-Reexports derselben Crate. Nullzeigerprüfung, exklusive Besitzverhältnisse, Reset und Freigabe sind explizit. Das ist der im Masterplan vorgesehene Rückfall für fehlende Bindungsfunktionen.

## Datenschutz der nativen Laufzeit

Die Windows-Laufzeit enthält ONNX Runtime 1.17.1. Deren Standard-Telemetrie wird vor der ersten VAD-/ASR-Session über `DisableTelemetryEvents` abgeschaltet. `runtime.rs` erstellt dazu einmalig die prozessweit wiederverwendete ONNX-Umgebung mit einem Logger, der Meldungen verwirft. Scheitert Initialisierung oder Abschaltung, wird kein Modell geladen. Die kleine Umgebung bleibt bis Prozessende erhalten, enthält jedoch keine Modelle oder deren Threadpools; somit bleibt die Abschaltung auch nach Entladen und erneutem Laden wirksam.

`ort-sys` liefert nur API-17-Bindings, ohne Downloads oder zusätzliches Linken. Unter Windows wird `OrtGetApiBase` aus der bereits durch sherpa importierten `onnxruntime.dll` aufgelöst und das Modul festgehalten. Es wird keine weitere DLL gesucht oder geladen. Alle sherpa-Konfigurationen setzen `debug = false`; Fehler werden ohne Audio, Transkript, Token oder native Diagnosetexte weitergegeben. `Transcript` implementiert bewusst kein `Debug`.

Das WAV-Beispiel schreibt das gewünschte Ergebnis ausschließlich als private Produktdatei nach `testdata/private/transcribe_file.txt`, nicht in Terminal- oder Logausgaben. Die Datei wird bei erfolgreicher Erkennung ersetzt. Es werden keine privaten Aufnahmen für gewöhnliche Unit-Tests gelesen.

## Windows-Build

Die gebündelte sherpa-DLL öffnet Modellpfade über schmale Zeichenketten.
Damit sie die von Rust übergebenen UTF-8-Pfade auch in einem Windows-Profil
mit Umlauten oder ß auflösen kann, ergänzt der MSVC-Build das
[UTF-8-Manifestfragment](../../crates/vad/utf8-paths.manifest) in den ausführbaren
VAD-, STT- und Benchmark-Targets. Die Einstellung gilt für den jeweiligen
Prozess. Beim Einbinden der Erkennung in die Desktop-App in Schritt 2.4
muss deren Anwendungsmanifest dieselbe Einstellung übernehmen. Das gilt
ebenso für weitere Windows-Programme, die die Bibliothek einbetten.

Die native Bibliothek wird nur beim Einrichten/Bauen durch die Abhängigkeit heruntergeladen und anhand ihrer hinterlegten SHA256 geprüft. LLVM/Clang bleibt für Bindgen erforderlich. Das ist eine Build-Abhängigkeit, kein Netzwerkpfad in der Anwendung.

Cargo 1.100 hat sein internes Build-Verzeichnislayout geändert. sherpa-rs-sys 0.6.8 erwartet beim DLL-Kopieren weiterhin vorhandene `debug/deps` beziehungsweise `release/deps`. `Import-MsvcEnvironment` legt deshalb diese Ordner und die Beispielordner vor dem Build an, auch wenn MSVC schon initialisiert ist. Die bestehenden Entwickler- und Prüfbefehle verwenden dieses Skript. Ein gewähltes `CARGO_TARGET_DIR` mit Leerzeichen oder Umlauten bleibt beim Import erhalten. Es erfolgt kein Eingriff in die Cargo-Registry, kein Downgrade und kein Austausch nativer Binärdateien. Bei künftigem sherpa-Update ist dieser Kompatibilitätsschritt erneut zu prüfen.

Ein paketbezogener Entwicklungsprofil-Override deaktiviert `debug_assertions` ausschließlich für `sherpa-rs-sys` einschließlich dessen Buildskript. Dieses fügt sonst die Debug-C-Laufzeit `msvcrtd` hinzu, obwohl die vorgefertigten nativen DLLs und Rust die Release-C-Laufzeit verwenden; im ersten Testlauf entstand dadurch LNK4098. Das Testprofil erbt den Override. Assertions im Projekt bleiben aktiv; globale Linkerflags sind nicht nötig.

## Prüfungen und Grenzen

Modellfreie Tests decken Pausenauswahl, harte Grenze, vollständige Abdeckung ohne Doppelung, ungültige Intervalle, leere Aufnahmen, Parameterprüfung, sequenzielle Erkennung, Lazy Loading, autonomes Entladen, Timergrenzen, erneute Nutzung, Fehler und Drop ab. VAD-Tests prüfen Fensterüberlappung, Padding, Zusammenführung und fehlerhafte Eingaben. Die Datenschutz-Initialisierung wird mit Fehlerattrappen sowie der nativen Laufzeit ohne Modelle geprüft. Das Beispiel prüft WAV-Formate, Argumente und beschädigte Eingaben mit synthetischen Daten.

Vier mit `#[ignore]` markierte Integrationstests setzen ausdrücklich installierte Modelle voraus: je ein ASR-Test für beide Parakeets und Whisper sowie ein Silero-Test mit synthetischer Stille und wiederholter Nutzung. Sie werden erst nach Installation gemäß README gestartet. Die sprachliche Qualität, Grenzübergänge echter Sprache und Leistung sind damit noch nicht nachgewiesen; der Benchmark folgt in Schritt 1.5. Hotkey-/Tray-Anbindung und die weitergehenden Halluzinationsregeln aus Schritt 1.6 sind noch nicht vorgesehen.

Die nativen Modellkonstruktoren setzen gültige ONNX-Dateien voraus. Der Modell-Manager prüft die Downloads per SHA256 und später Installationsmarker sowie Dateigrößen. Nachträgliche Beschädigungen gleicher Größe werden dadurch nicht erkannt; native Exceptions oder Prozessabbrüche lassen sich vom Rust-Wrapper nicht zuverlässig in `Result::Err` übersetzen. Für die unterstützte Nutzung müssen die geprüften Installationen unverändert bleiben. Eine Prozessisolation der nativen Inferenz ist in diesem Bauschritt nicht enthalten.

## Quellen

- [sherpa-rs 0.6.8: Quellcode und Buildfeatures](https://docs.rs/crate/sherpa-rs/0.6.8/source/)
- [sherpa-rs-sys 0.6.8: Buildskript und native Version](https://docs.rs/crate/sherpa-rs-sys/0.6.8/source/)
- [ONNX Runtime: C-API, gemeinsame Umgebung und DisableTelemetryEvents](https://onnxruntime.ai/docs/api/c/struct_ort_api.html)
- [ONNX Runtime: Telemetrieeinstellung unter Windows](https://onnxruntime.ai/docs/get-started/with-c.html)
- [Cargo: Build-Cache und Änderung ab 1.100](https://doc.rust-lang.org/cargo/reference/build-cache.html)
