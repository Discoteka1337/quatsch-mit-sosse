# 0001 – Standardmodell nach dem privaten ASR-Benchmark

Datum: 20.11.2026 · Bauschritt: 1.5

## Entscheidung

Empfohlenes Standardmodell ist **Parakeet primeLine Deutsch int8**
(`parakeet-primeline-de-int8`). Rückfallmodell ist **Parakeet TDT 0.6B v3 int8**
(`parakeet-v3-int8`). Die Empfehlung gilt für den vorliegenden privaten Korpus
und die gemessene CPU-Konfiguration. Die spätere App-Anbindung soll diese
Reihenfolge übernehmen; dieser Schritt verändert keine Engine-Voreinstellung.

Der [Benchmark vom 20.11.2026](../benchmarks/asr-2026-11-20.md) wurde lokal
auf einem AMD Ryzen 7 5800X mit 16 logischen Prozessoren und rund 32 GiB RAM
ausgeführt: Windows, Release-Build, zwei ASR-Threads, ein VAD-Thread,
Sprachhinweis `de`. Gemessen wurden alle installierten ASR-Modelle über den
bestehenden `SpeechEngine`-Trait einschließlich Silero-VAD und Aufteilung an
Sprachpausen. Die Modelle liefen nacheinander in getrennten Prozessen.

| Messgröße | primeLine Deutsch | Parakeet v3 |
| --- | ---: | ---: |
| Gepoolte Wortfehlerrate | **23,14 %** | 27,93 % |
| Wortfehler / Referenzwörter | 348 / 1504 | 420 / 1504 |
| Höchster Median einer Zehnsekundenprobe | **0,933 s** | 1,094 s |
| Langsamster einzelner Zehnsekundenlauf | 0,972 s | 1,257 s |
| RTF der vollständigen Sprachaufnahmen | 0,066 | 0,080 |
| Ende→Text aller vollständigen Aufnahmen, Mittelwert | 0,837 s | 1,023 s |
| Ende→Text aller vollständigen Aufnahmen, P95 | 1,606 s | 1,890 s |
| Kaltstart mit Aufnahme 001 (16,330 s Audio) | 8,585 s | 7,641 s |
| Prozess-Spitzen-RAM einschließlich Laden und Korpus | 1891,21 MiB | 1892,98 MiB |
| Phantomtexte bei leerer Referenz | **0 / 10** | **0 / 10** |

primeLine hat 72 Wortfehler weniger, also eine um 4,79 Prozentpunkte niedrigere
WER. Beide Modelle erfüllen die Zeitgrenze von strikt unter 1,5 Sekunden für
zehn Sekunden Audio bei geladenem Modell. Damit entscheidet gemäß Auftrag
die niedrigere WER zugunsten von primeLine. Parakeet v3 ist als bereits
gemessene, ebenfalls schnelle Alternative der Rückfall bei Problemen mit
primeLine oder seiner Installation.

Whisper large-v3-turbo ist nicht installiert und wurde weder heruntergeladen
noch gemessen. Der Masterplan nennt Whisper als Rückfallkandidaten. Für diese
Entscheidung wird bewusst das tatsächlich geprüfte Parakeet v3 bevorzugt;
Whisper erhält ohne Messwerte keine Qualitäts- oder Leistungsbewertung. Bei
einer späteren Installation soll der gleiche Benchmark erneut entscheiden.

## Messregel und Datenschutz

Die WER zählt Ersetzungen, Auslassungen und Einfügungen über alle 40
Sprachreferenzen mit insgesamt 1504 Wörtern. Unicode-NFC, Kleinschreibung und
Entfernung aller Unicode-Satzzeichen gehen voraus; Umlaute und ß bleiben
erhalten. Es gibt keine zusätzliche Zahlen-, Fachwort- oder Füllwortbereinigung.
Die zehn leeren Referenzen werden separat auf nicht leere Ausgaben geprüft.

Für 37 ausreichend lange Sprachaufnahmen wurden je drei mittige Ausschnitte
mit exakt zehn Sekunden verarbeitet. Die Zeitgrenze verwendet den höchsten
Median **aller** Proben, einschließlich leerer Ausgaben. Mindestens eine Probe
muss dreimal nicht leeren Text liefern, damit das Ergebnis nicht allein auf
VAD-Stilleprüfungen beruht. Dies ist bei beiden Modellen für 36 von 37 Proben
erfüllt. Eine einzelne leere Probe ist kein zusätzliches Qualitätsveto neben
dem verlangten WER-/Zeitkriterium; ihre Fehler bleiben in der WER enthalten.

Referenzen und Erkennungstexte bleiben ausschließlich im Prozessspeicher.
Terminal, temporäre Messdateien und Markdown enthalten nur Modellkennungen,
Aufnahmenummern und Messwerte. Der private Korpus wurde nicht verändert.

## Einschränkungen und menschliche Prüfung

- **002–005 lokal anhören und mit ihren TXT-Referenzen vergleichen.** 002,
  004 und 005 dauern nur 5,530, 4,340 und 2,080 Sekunden. 002, 003 und 005
  liefern bei beiden Modellen vollständige Auslassungen; 004 liegt bei
  97,37–100 % WER. Auch die Zehnsekundenprobe 003 bleibt in allen drei
  Wiederholungen leer. Ob Aufnahme, Referenz, VAD oder ASR verantwortlich
  ist, wurde nicht festgestellt. Kein Paar wurde aus der WER entfernt.
- Nach bestätigten Referenzkorrekturen oder neuen Aufnahmen den Benchmark
  wiederholen. Die Empfehlung beruht auf dem gegenwärtigen Korpus;
  die absolute WER ist noch kein Nachweis guter Alltagserkennung.
- Die warme Zehnsekundenzeit belegt **keinen** Kaltstart unter 1,5 Sekunden.
  Die Kaltstartwerte oben betreffen die vollständige Aufnahme 001. Für
  schnelle erste Diktate ist das Laden während der Aufnahme beziehungsweise
  eine spätere Ladezeitoptimierung zu prüfen.
- Die RAM-Spitze ist der höchste Working Set des gesamten Messprozesses,
  einschließlich eingelesenem Korpus und Modellladen. Sie ist kein
  Leerlaufwert der Desktop-App.
- 0 Phantomtexte gelten für die aktuelle Pipeline mit VAD. Ein separater
  Rohmodelltest wurde in Schritt 1.5 nicht durchgeführt. Die spätere Abnahme
  der Satzanfänge und des Nachfilters steht im Nachtrag zum Benchmark-Bericht.

## Reproduktion und Prüfungen

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
$env:CARGO_TARGET_DIR = "C:\qms-target"
. "./scripts/import-msvc.ps1"
Import-MsvcEnvironment
cargo run --offline --release --package bench -- --date "2026-11-20"
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/check.ps1"
```

Ein erneuter Lauf ersetzt den Tagesbericht. Die Benchmark-Tests prüfen
Korpuspaare, WAV-Formate, Unicode-WER, echte Zehnsekunden-Ausschnitte,
leere Erkennungen, strikte Zeitgrenzen, numerische Berichte und Fehlerfälle
ohne Veröffentlichung privater Eingaben. Die 30 neuen Benchmark-Tests sind
bestanden. Die zentrale Vollprüfung mit `scripts/check.ps1` wurde am
20.11.2026 mit Exitcode 0 vollständig bestanden: Formatierung,
Workspace-Clippy ohne Warnungen, Workspace-Tests, UI-Check und UI-Tests,
Pfad-/Prüfregel-/CI-Regressionen, Datenschutz-Wächter samt Negativtests,
Messskript-Regressionen und native Laufzeit-/Fensterprüfungen.

Die optionalen `#[ignore]`-Modelltests wurden nicht gesondert gestartet;
beide installierten ASR-Modelle und Silero wurden stattdessen im echten
Benchmark verwendet. Zusätzlich wurden die 50 Aufnahmezeilen und
37 Probezeilen je Modell, die WER-Summen und das Fehlen vollständiger
privater Referenztexte in den öffentlichen Dokumenten geprüft. Private
Korpusdateien bleiben ignoriert und fehlen im Git-Index.

## Nachtrag zu Schritt 1.6

Für den Wortanfangstest gelten die ersten 20 Sprachreferenzen nach
Aufnahmenummer. Diese Auswahl stand vor dem Vergleich fest und schließt die
bereits in Schritt 1.5 auffälligen Aufnahmen ein. Alle leeren Referenzen
gehen in die Stilleprüfung ein.

Der Erkenner erhält am ersten von Silero gemeldeten Sprachbeginn bis zu
500 ms Vorlauf aus der Aufnahme. Vor dem bisherigen 100-ms-Rand kommen bis
zu 400 ms hinzu, soweit sie in das erste Stück von höchstens 20 Sekunden
passen. So bleibt ein knapp 20 Sekunden langer Satz für die Erkennung
zusammenhängend. Der Nachfilter vergleicht erst den zusammengesetzten
Text, damit eine eigenständig erkannte Floskel innerhalb eines längeren
Diktats erhalten bleibt.

Nach der Hörprüfung von 002–006 wurde der Stand mit 15 von 20 korrekten
Satzanfängen beim Standardmodell für den Commit freigegeben. Für künftige
Abnahmeläufe gilt weiterhin das Ziel von 20 von 20. Messwerte und Hörbefund
stehen im [Benchmark-Bericht](../benchmarks/asr-2026-11-20.md#schritt-16--stille-und-satzanfänge).
