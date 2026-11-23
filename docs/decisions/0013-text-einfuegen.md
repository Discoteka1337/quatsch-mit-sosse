# 0013 – Text am Cursor einfügen

20.11.2026 · Schritt 2.2

Die Windows-Funktion `insert` in `crates/inject` erfasst beim Aufruf das Vordergrundfenster und gibt den verwendeten Einfügeweg oder einen Fehler zurück. Sie gehört auf einen Hintergrundthread, weil sie auf die Zielanwendung und gegebenenfalls die Wiederherstellung der Zwischenablage wartet. Gleichzeitig laufende Aufrufe erhalten einen Fehler.

## Auswahl des Einfügewegs

Bis einschließlich 2000 Unicode-Skalarwerten versucht die Crate UI Automation, danach Unicode-Tastatureingabe und zuletzt die Zwischenablage. Ein Emoji außerhalb der BMP zählt dabei als ein Zeichen. Ein erzwungener Weg ist für diesen Bereich verbindlich. Ab 2001 Zeichen hat grundsätzlich der direkte Zwischenablageweg Vorrang. Ein erkennbarer Fokuswechsel oder eine mögliche Teilzustellung beendet den Versuch. Damit wird ein bereits eingefügter Anfang beim Wechsel des Verfahrens nicht verdoppelt.

**Nachtest vom 21.11.2026.** Word fügte beim erzwungenen Zwischenablageweg wiederholt die vorherige Kopie ein, obwohl der Zielprozess den angebotenen Text bereits abgerufen hatte. Ein Lesezugriff durch Word genügt daher als Bestätigung für das Einfügen an der Schreibmarke nicht. Bei `winword.exe` und dem klassischen Outlook mit Word-Editor (`outlook.exe`) weist die App diesen Weg vor jeder Änderung der Zwischenablage zurück. In Chrome und Edge blieb beim erzwungenen Versuch die Zustellung unbestätigt.

Für diese vier Programme verwendet die automatische Auswahl auch bei langen Texten die geprüfte Unicode-Tastatureingabe. Das dauert bei langen Diktaten länger. Die Prüfung des aktiven Ziels zwischen den Zeichenblöcken bleibt wirksam. Die Ausnahme von der Langtextregel des Masterplans beruht auf den [Einfügetests in den Zielprogrammen](../../tests/inject-matrix.md#einfügetest-vom-21112026).

Die optionale Datei `%LOCALAPPDATA%\QuatschMitSosse\data\inject.toml` ordnet ausführbaren Dateien einen Weg zu. Groß- und Kleinschreibung spielen beim Prozessnamen keine Rolle. Ohne Eintrag gilt die automatische Auswahl. Die folgende Beispieltabelle zeigt alle zulässigen Werte.

```toml
[apps]
"notepad.exe" = "clipboard"
"msedge.exe" = "unicode"
"beispiel-editor.exe" = "uia"
```

## Einfügeposition und Rückmeldung

UI Automation prüft das fokussierte Element und dessen Schreibschutz. Bei nativen Edit- und RichEdit-Feldern ersetzt `EM_REPLACESEL` die aktuelle Auswahl oder fügt an der Einfügemarke ein. Die Aktion unterstützt Rückgängig. Bei anderen Anbietern kann `ValuePattern.SetValue` ein leeres oder vollständig markiertes Edit-Feld befüllen. Liefert ein Anbieter bei einer Teilmarkierung nur Lesezugriff über `TextPattern`, übernimmt im automatischen Modus die Tastatureingabe. Dadurch bleiben benachbarter Text und dessen Formatierung erhalten. [Microsoft zum UI-Automation-Textmodell](https://learn.microsoft.com/windows/win32/winauto/uiauto-understandingtheuiautomationtextobjectmodel) · [EM_REPLACESEL](https://learn.microsoft.com/windows/win32/controls/em-replacesel)

Die Tastatureingabe sendet Blöcke von bis zu 16 Zeichen. Ein Surrogatpaar bleibt zusammen. CRLF, CR und LF ergeben jeweils einen Druck auf Enter. Vor jedem Block prüft die Crate das Vordergrundfenster. Bei `SendInput` bestätigt Windows die Aufnahme der Ereignisse in den Eingabestrom. Ob etwa ein Chat Enter als Zeilenumbruch oder als Absenden auswertet, entscheidet die Zielanwendung. [Microsoft zu SendInput](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput)

## Zwischenablage

Vor dem Leeren sichert die Crate sämtliche angebotenen Formate. Sie verwendet für Bitmaps, Paletten und Metadateien passende Windows-Kopierfunktionen. Bei Formaten, die weiterhin ihren bisherigen Besitzer benötigen, bricht sie vor der ersten Änderung ab. Das betrifft etwa lebende OLE-Datenobjekte und besitzerabhängige private Formate. [Windows-Zwischenablageformate](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats) · [OleGetClipboard](https://learn.microsoft.com/windows/win32/api/ole2/nf-ole2-olegetclipboard)

Der vorübergehende Text erhält die drei Windows-Schutzformate `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` und `CanUploadToCloudClipboard`. Die beiden Freigabewerte stehen auf null. Nach Strg+V wartet die Crate bis zu zwei Sekunden auf die Anforderung des Textformats durch Windows. Bei ausbleibender Anforderung meldet sie eine ungewisse Zustellung. Sie stellt die gesicherten Formate auch bei einem Einfügefehler wieder her, sobald die Zwischenablage frei ist. Kopiert der Nutzer währenddessen etwas Neues, bleibt die neue Kopie erhalten und der Aufruf meldet den Wechsel. [Windows zu Verlauf und Cloud-Zwischenablage](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats) · [Anforderung eines Zwischenablageformats](https://learn.microsoft.com/windows/win32/dataxchg/wm-renderformat)

Beim verzögerten Bereitstellen muss Windows das lesende Fenster dem Zielprozess zuordnen können. Bleibt die Anforderung anonym, wartet die App auf eine zuordenbare Anforderung bis zur Zeitgrenze. Danach zieht sie den Text zurück und meldet die ungewisse Zustellung. Über „Kopieren“ im Overlay lässt sich der Text anschließend bewusst übernehmen. [Microsoft zur Zuordnung des lesenden Fensters](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getopenclipboardwindow)

Beim Wiederherstellen veröffentlicht die App vorhandene Windows-Schutzformate vor den gesicherten Nutzdaten. Schlägt das Setzen eines Schutzformats auch beim zweiten Versuch fehl, meldet sie den Wiederherstellungsfehler und hält die geschützten Inhalte zurück. Die Reihenfolge der Nutzdatenformate bleibt erhalten.

Hält eine andere Anwendung die Zwischenablage nach dem Einfügeversuch länger als eine Sekunde geöffnet, erscheint ein Hinweis im Overlay. Weitere Diktate sowie das Einfügen über UI Automation und Unicode sind wieder möglich. Ein einzelner Hintergrundthread hält die Sicherung im Arbeitsspeicher und prüft erneut, bis er die vorige Kopie wiederherstellen kann oder inzwischen etwas Neues kopiert wurde. Ein vorheriger Einfügeversuch wird dabei nie wiederholt. Das synchrone Testprogramm wartet weiterhin bis zum Abschluss.

Zum Erhalt der vorherigen Kopie die App bis zur Freigabe der Zwischenablage geöffnet lassen. Bei ausdrücklichem Beenden während der Sperre endet der Hintergrundthread mit dem Prozess. Seine Sicherung geht verloren. Falls Windows den vorübergehenden Text bereits an das Ziel geliefert hat, kann dieser mit seinen Schutzformaten in der Zwischenablage verbleiben.

## Manuelle Prüfung

1. In einer für MSVC vorbereiteten PowerShell `cargo run -p quatsch-mit-sosse-inject --example inject_test` im Projektordner starten. Innerhalb von drei Sekunden den Cursor zwischen zwei vorhandene Wörter setzen.
2. In Editor, Browser und Word prüfen, dass „Grüße aus Überlingen – 3,5 %“ genau einmal an der Einfügemarke erscheint. Mit markiertem Text wiederholen. Die Konsole nennt den verwendeten Weg.
3. Für einen gezielten Zwischenablagetest den Prozess auf `"clipboard"` setzen und vor dem Start formatierten Text oder ein Bild kopieren. Nach dem Einfügen mit Strg+V in einem geeigneten Dokument kontrollieren, dass die ursprüngliche Kopie mit Formatierung beziehungsweise Bild erhalten ist.
