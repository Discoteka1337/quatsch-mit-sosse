# 0013 – Text am Cursor einfügen

20.11.2026 · Schritt 2.2

Die Windows-Funktion `insert` in `crates/inject` erfasst beim Aufruf das Vordergrundfenster und gibt den verwendeten Einfügeweg oder einen Fehler zurück. Sie gehört auf einen Hintergrundthread, weil sie auf die Zielanwendung und gegebenenfalls die Wiederherstellung der Zwischenablage wartet. Gleichzeitig laufende Aufrufe erhalten einen Fehler.

## Auswahl des Einfügewegs

Bis einschließlich 2000 Unicode-Skalarwerten versucht die Crate UI Automation, danach Unicode-Tastatureingabe und zuletzt die Zwischenablage. Ein Emoji außerhalb der BMP zählt dabei als ein Zeichen. Ein erzwungener Weg ist für diesen Bereich verbindlich. Ab 2001 Zeichen hat die Vorgabe für Langtexte Vorrang und die Crate verwendet direkt die Zwischenablage. Ein erkennbarer Fokuswechsel oder eine mögliche Teilzustellung beendet den Versuch. Damit wird ein bereits eingefügter Anfang beim Wechsel des Verfahrens nicht verdoppelt.

Die optionale Datei `%LOCALAPPDATA%\QuatschMitSosse\data\inject.toml` ordnet ausführbaren Dateien einen Weg zu. Groß- und Kleinschreibung spielen beim Prozessnamen keine Rolle. Ohne Eintrag gilt die automatische Auswahl. Die folgende Beispieltabelle zeigt alle zulässigen Werte.

```toml
[apps]
"notepad.exe" = "uia"
"msedge.exe" = "unicode"
"winword.exe" = "clipboard"
```

## Einfügeposition und Rückmeldung

UI Automation prüft das fokussierte Element und dessen Schreibschutz. Bei nativen Edit- und RichEdit-Feldern ersetzt `EM_REPLACESEL` die aktuelle Auswahl oder fügt an der Einfügemarke ein. Die Aktion unterstützt Rückgängig. Bei anderen Anbietern kann `ValuePattern.SetValue` ein leeres oder vollständig markiertes Edit-Feld befüllen. Liefert ein Anbieter bei einer Teilmarkierung nur Lesezugriff über `TextPattern`, übernimmt im automatischen Modus die Tastatureingabe. Dadurch bleiben benachbarter Text und dessen Formatierung erhalten. [Microsoft zum UI-Automation-Textmodell](https://learn.microsoft.com/windows/win32/winauto/uiauto-understandingtheuiautomationtextobjectmodel) · [EM_REPLACESEL](https://learn.microsoft.com/windows/win32/controls/em-replacesel)

Die Tastatureingabe sendet Blöcke von bis zu 16 Zeichen. Ein Surrogatpaar bleibt zusammen. CRLF, CR und LF ergeben jeweils einen Druck auf Enter. Vor jedem Block prüft die Crate das Vordergrundfenster. Bei `SendInput` bestätigt Windows die Aufnahme der Ereignisse in den Eingabestrom. Ob etwa ein Chat Enter als Zeilenumbruch oder als Absenden auswertet, entscheidet die Zielanwendung. [Microsoft zu SendInput](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput)

## Zwischenablage

Vor dem Leeren sichert die Crate sämtliche angebotenen Formate. Sie bewahrt die Reihenfolge und verwendet für Bitmaps, Paletten und Metadateien passende Windows-Kopierfunktionen. Bei Formaten, die weiterhin ihren bisherigen Besitzer benötigen, bricht sie vor der ersten Änderung ab. Das betrifft etwa lebende OLE-Datenobjekte und besitzerabhängige private Formate. [Windows-Zwischenablageformate](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats) · [OleGetClipboard](https://learn.microsoft.com/windows/win32/api/ole2/nf-ole2-olegetclipboard)

Der vorübergehende Text erhält die drei Windows-Schutzformate `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` und `CanUploadToCloudClipboard`. Die beiden Freigabewerte stehen auf null. Nach Strg+V wartet die Crate bis zu zwei Sekunden auf die Anforderung des Textformats durch Windows. Bei ausbleibender Anforderung meldet sie eine ungewisse Zustellung. Sie stellt die gesicherten Formate auch bei einem Einfügefehler wieder her, sobald die Zwischenablage frei ist. Kopiert der Nutzer währenddessen etwas Neues, bleibt die neue Kopie erhalten und der Aufruf meldet den Wechsel. [Windows zu Verlauf und Cloud-Zwischenablage](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats) · [Anforderung eines Zwischenablageformats](https://learn.microsoft.com/windows/win32/dataxchg/wm-renderformat)

## Manuelle Prüfung

1. In einer für MSVC vorbereiteten PowerShell `cargo run -p quatsch-mit-sosse-inject --example inject_test` im Projektordner starten. Innerhalb von drei Sekunden den Cursor zwischen zwei vorhandene Wörter setzen.
2. In Editor, Browser und Word prüfen, dass „Grüße aus Überlingen – 3,5 %“ genau einmal an der Einfügemarke erscheint. Mit markiertem Text wiederholen. Die Konsole nennt den verwendeten Weg.
3. Für einen gezielten Zwischenablagetest den Prozess auf `"clipboard"` setzen und vor dem Start formatierten Text oder ein Bild kopieren. Nach dem Einfügen mit Strg+V in einem geeigneten Dokument kontrollieren, dass die ursprüngliche Kopie mit Formatierung beziehungsweise Bild erhalten ist.
