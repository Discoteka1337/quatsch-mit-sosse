# 0008 – Modellquellen und geprüfte Download-Dateien

Datum: 19.11.2026 · Bauschritt: 1.2

## Entscheidung

Das eingebettete Manifest `crates/models/manifest.toml` enthält alle vier
angefragten Modelle. Jede Download-Datei besitzt ihre eigene SHA256-Prüfsumme,
Byte-Größe und explizite Liste der zu installierenden Dateien. Die Zielnamen
bleiben ASCII. Modellgewichte werden weder im Projektordner noch im Git-Index
abgelegt.

### Parakeet TDT 0.6B v3

Quelle ist das offizielle [sherpa-onnx-Release `asr-models`](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models),
Datei `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2`.
Die [sherpa-Dokumentation](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html)
beschreibt den NeMo-Transducer-Export. Das [NVIDIA-Basismodell](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
nennt CC-BY-4.0 und die 25 europäischen Sprachen. NVIDIA sowie
k2-fsa/sherpa-onnx und Fangjun Kuang werden genannt; Export und
int8-Quantisierung sind als Änderungen gegenüber dem Basismodell ausgewiesen.

Installiert werden Encoder, Decoder, Joiner und Tokenliste. README und
Beispielaufnahmen im Archiv werden nicht installiert.

### Parakeet primeLine Deutsch

Quelle ist [flozen1981/parakeet-primeline-onnx](https://huggingface.co/flozen1981/parakeet-primeline-onnx/tree/d548e25b9bfe559aa274f361892dc4ed5d64743a).
Alle URLs sind auf die Revision `d548e25b9bfe559aa274f361892dc4ed5d64743a`
festgelegt. Die Modellkarte beschreibt den deutschen Fine-Tune von
[primeLine AI Services](https://huggingface.co/primeline/parakeet-primeline),
das NVIDIA-Basismodell und CC-BY-4.0. Alle drei Urheberstufen werden genannt.

Der Encoder besteht zwingend aus `encoder.int8.onnx` **und**
`encoder.int8.onnx.data`. Beide Dateien werden nebeneinander installiert,
zusammen mit Decoder, Joiner und `tokens.txt`. Das laut Modellkarte für sherpa-onnx
nicht erforderliche Referenzvokabular `bpe.vocab` wird nicht installiert.

### Whisper large-v3-turbo ist verfügbar

Es gibt einen offiziellen Export: `sherpa-onnx-whisper-turbo.tar.bz2` im
[sherpa-onnx-Release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models).
Der abweichende Archivname ist kein anderes Modell: OpenAIs
[Modellzuordnung](https://github.com/openai/whisper/blob/main/whisper/__init__.py)
führt `turbo` und `large-v3-turbo` auf dieselbe Gewichtsdatei.
Das offizielle [Exportskript](https://github.com/k2-fsa/sherpa-onnx/blob/master/scripts/whisper/export-onnx.py)
lädt `turbo` über OpenAI Whisper; der
[Release-Workflow](https://github.com/k2-fsa/sherpa-onnx/blob/master/.github/workflows/export-whisper-to-onnx.yaml)
exportiert und veröffentlicht diesen Modellnamen.

Installiert werden `turbo-encoder.int8.onnx`, `turbo-decoder.int8.onnx` und
`turbo-tokens.txt`. Die Sprachkennungen folgen OpenAIs
[Modellkarte](https://huggingface.co/openai/whisper-large-v3-turbo) und
[Tokenizer](https://github.com/openai/whisper/blob/main/whisper/tokenizer.py),
einschließlich Kantonesisch (`yue`). Die [MIT-Lizenz von OpenAI](https://github.com/openai/whisper/blob/main/LICENSE)
und die Namensnennung stehen in `NOTICE`.

### Silero VAD

Verwendet wird exakt `silero_vad.onnx` aus dem offiziellen
[sherpa-onnx-Release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models),
nicht die ebenfalls angebotene int8-, v4- oder v5-Datei.
[Silero VAD](https://github.com/snakers4/silero-vad) nennt mehrsprachige
Sprachaktivitätserkennung; `mul` kennzeichnet dies im Manifest, ohne eine
ASR-Sprachliste vorzutäuschen. Die [MIT-Lizenz des Silero-Teams](https://github.com/snakers4/silero-vad/blob/master/LICENSE)
steht einschließlich Copyright-Vermerk in `NOTICE`.

## Tatsächlich durchgeführte Prüfsummenprüfung

Am 19.11.2026 wurden alle acht im Manifest verwendeten Download-Dateien
vollständig per HTTPS heruntergeladen und mit PowerShell `Get-FileHash
-Algorithm SHA256` lokal geprüft. Die Archivverzeichnisse wurden mit
`tar.exe -tvjf` vollständig gelesen; die unkomprimierten Dateigrößen im Manifest
stammen aus diesen Archivverzeichnissen. Die lokale Prüfsumme des
Parakeet-Archivs und von Silero stimmt außerdem mit dem GitHub-Release-Digest
überein; die vier großen Primeline-Dateien stimmen mit den HF-LFS-Digests
überein. Die lokal berechneten Werte sind die maßgebliche Grundlage.

| Download-Datei | Bytes | Lokal berechnete SHA256 |
| --- | ---: | --- |
| Parakeet-v3-Archiv | 487211443 | `1495e91b59d7fc53ddca91f3431758ec99f8fa253fdb0e1e851be0f72b14b0f9` |
| Primeline `encoder.int8.onnx` | 1548009 | `d4232f86718da0330167fb10789d1a35cffe6a60cd58239957943a8d9bc24c63` |
| Primeline `encoder.int8.onnx.data` | 650776320 | `d3b0d27912043d38a3c2ce4f2c03124be30bc1ff57d976302908954b2e8fe7bb` |
| Primeline `decoder.int8.onnx` | 11845275 | `fb4ddefe200706cabb27ee3fc1c81efa50555a4c8a8e00b663cc795216fb9369` |
| Primeline `joiner.int8.onnx` | 6355277 | `8220c0d117d81bdd0d8c770881932ac340f1ce4b36932941d561d11ad1aaffce` |
| Primeline `tokens.txt` | 102132 | `ba8e4007c65f4bb4358ffe2ecc13d9ccc7a10351151065242b5c3a943e685742` |
| Whisper-turbo-Archiv | 563790207 | `b11acbbcd660b44a8e0df33724feb5aaa709cf65668f2823d59f656312544f22` |
| `silero_vad.onnx` | 643854 | `9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6` |

Die acht Dateien umfassen zusammen 1.722.272.517 Download-Bytes.
Die einmalig geprüften Downloads liegen außerhalb des Repositorys unter
`C:\qms-model-verification`; `verified-hashes.json` dort enthält das lokale
Prüfprotokoll. Diese Arbeitskopien sind keine Voraussetzung der Anwendung oder
der automatisierten Tests und dürfen nach der Abnahme entfernt werden.

## Erlaubte Download-Hosts

Die tatsächlichen HTTP-Weiterleitungen beim Prüflauf ergaben exakt:

- `github.com` → `release-assets.githubusercontent.com` für Release-Dateien.
- `huggingface.co` → `us.aws.cdn.hf.co` für große Primeline-Dateien;
  die Tokenliste bleibt auf `huggingface.co`.

Nur diese vier exakten Hostnamen stehen in `allowed_hosts`. Signierte,
kurzlebige CDN-Adressen werden nicht im Manifest gespeichert; die beständige
Quell-URL erzeugt sie bei jedem Download neu. Jeder Weiterleitungsschritt muss
erneut geprüft werden. Ändert ein Anbieter seinen CDN-Host, ist eine explizite
Manifeständerung erforderlich. Wildcards und automatisch freigegebene Hosts
sind ausgeschlossen.

Produktiver Netzwerkcode gehört ausschließlich nach `crates/models/src/net.rs`.
Lokale Testserver prüfen Fehlerfälle ohne Downloads echter Gewichte. `list`,
`is_installed`, `path_of` und `remove` benötigen keine Netzverbindung.

## Installation und Wiederaufnahme

`Manager::new()` verwendet `%LOCALAPPDATA%\QuatschMitSosse\models`;
`with_root` erlaubt einen getrennten absoluten Zielordner für Tests und Werkzeuge.
Das Manifest wird in die Bibliothek eingebettet. Eine Installation arbeitet
synchron und meldet Download, Prüfsummenprüfung, Bereitstellung und Abschluss
über einen Callback. Eine spätere Oberfläche muss dafür einen Arbeits-Thread
verwenden. Ein Abbruchsignal wird über `AtomicBool` weitergegeben.

Teildownloads liegen unter `.downloads/<id>/<index>.part`. Der Cache ist an
einen Fingerprint der Modellmetadaten gebunden; nach einer Manifeständerung
wird ein alter Cache verworfen. Vollständig vorhandene Downloads werden erneut
gehasht. HTTP-Bereichsanfragen setzen unvollständige Dateien fort; ignoriert
der Server die Bereichsanfrage, beginnt die Datei erneut. Nach Abbruch oder
Verbindungsfehler bleiben diese Dateien erhalten. Falsche SHA256-Werte oder
ungültige Archive entfernen Cache und unvollständige Installation des Modells.

Die Bereitstellung erfolgt zunächst unter `.staging/<id>`. Nur deklarierte
reguläre Dateien mit passender Größe werden übernommen; Archivpfade,
Windows-Sondernamen, alternative Datenströme und Verknüpfungen werden geprüft.
Erst nach allen Dateien wird der Installationsmarker geschrieben und der
Staging-Ordner in den endgültigen Modellordner umbenannt. Eine Betriebssystem-
Dateisperre verhindert parallele Installationen oder Löschungen desselben
Modells und wird auch nach einem Prozessabbruch freigegeben. Kleine inerte
Sperrdateien unter `.locks` bleiben bestehen.

`is_installed` prüft Marker, Manifest-Fingerprint und die Größen aller
erforderlichen Dateien. Es hasht die installierten Modellgewichte nicht bei
jeder Statusabfrage erneut. `path_of` gibt nur für diesen vollständigen Zustand
einen Pfad zurück. Zum Laden hält `Manager::acquire` eine gemeinsame
Lesesperre, bis der native Konstruktor seine Dateien geöffnet hat. Mehrere
Erkenner können gleichzeitig laden. Ein paralleles `install` oder `remove`
meldet währenddessen, dass das Modell belegt ist. `remove` entfernt Modell,
Staging und Downloadreste.

## Prüfungen in Schritt 1.2

Die Implementierung entstand nach zunächst fehlschlagenden Tests. 23 Modelltests
und drei CLI-Integrationstests prüfen unter anderem Prüfsummenfehler,
Abbruch/Fortsetzen, fremde Hosts und Weiterleitungen, falsche HTTP-Bereiche,
Archivpfade, Dateigrößen, konkurrierende Vorgänge und den Installationsstatus.
Die HTTP-Tests verwenden ausschließlich lokale Server. Deren angenommene
Sockets werden ausdrücklich auf blockierenden Betrieb gesetzt, da Windows
den nicht blockierenden Modus des Listeners vererbt.

Zusätzlich wurden alle vier echten Modelle über die CLI in einem getrennten
`LOCALAPPDATA`-Testordner installiert, anschließend gelistet und wieder entfernt.
Silero wurde dabei über den produktiven HTTPS-Transport geladen; die großen
Modelle verwendeten die bereits vollständig heruntergeladenen Prüfdateien als
Cache und durchliefen erneut SHA256-Prüfung und Bereitstellung. Die
Whisper-Installation wurde während der Bereitstellung gezielt als Prozess
beendet und anschließend erfolgreich aus dem Cache wiederaufgenommen.
Die regulären Benutzer-Modellordner wurden dabei nicht verändert.

Die abschließende Vollprüfung mit `scripts/check.ps1` wurde am 19.11.2026
**vollständig bestanden**: Formatierung, Workspace-Clippy, alle Workspace-Tests
einschließlich der 23 Modelltests und drei CLI-Tests, UI-Prüfungen,
Datenschutz-Wächter und sämtliche vorgesehenen Regressionstests.

Bei früheren Versuchen wurde der Start der neu gebauten Modell-Testdatei unter
`C:\qms-target` mehrfach von Windows mit Zugriffs- beziehungsweise
Dateisperrfehlern verweigert.
Die zunächst vorgelegten Kaspersky-Protokolle zeigten nur Zuweisungen zur
Programmgruppe „Schwach beschränkt“. Eine spätere Meldung nannte jedoch
`VHO:Trojan-Spy.Win32.Convagent.gen` für die Modell-Testdatei. Ein Fehlalarm ist
nicht bestätigt; eine Herstellerbewertung steht aus. Es wurden keine Dateien
zur Analyse hochgeladen.

Nachdem der Anwender eine Ausnahme eingerichtet und einen erneuten Prüflauf
angefordert hatte, scheiterte der Start derselben Testdatei weiterhin mit
Windows-Fehler 32 (Datei von einem anderen Prozess verwendet), Cargo-Exitcode
101. Erst beim anschließenden, erneut vom Anwender angeforderten Vollprüflauf
konnte die Testdatei gestartet werden; der gesamte Lauf endete mit Exitcode 0.
Die erfolgreiche Funktionsprüfung bestätigt keinen Fehlalarm des Virenschutzes.

## Abgrenzung

Dieser Schritt installiert Dateien und liefert Lizenz- und Namensnennungsdaten.
Er lädt kein Modell in eine Erkennungs-Laufzeit. Spracherkennung und VAD folgen
erst in Schritt 1.3, der Vergleich mit eigenen Aufnahmen in Schritt 1.5.
Die spätere native Modellauswahl und Über-Ansicht müssen die im Manifest
hinterlegten Namensnennungen sichtbar machen.
