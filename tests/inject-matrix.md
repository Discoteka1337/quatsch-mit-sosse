# Schritt 2.5 · Einfüge-Test in 15 Programmen

**Testsatz** `Äpfel, Öl & Übermaß – 3,5 % für 12,99 €`

Im manuellen Durchlauf **vor der Korrektur** wurde der Testsatz mit dem Diktier-Hotkey gesprochen. Laut Rückmeldung kam der Text in allen 15 Programmen an. Als einzige Abweichung wurde durchgehend Doppel-s anstelle von ß gemeldet. Die Spalte „Umlaute ok“ umfasst in diesem Test auch das ß. Für den Einfügeweg und die Kontrolle der Zwischenablage liegen keine Einzelangaben vor.

| Programm | Weg (UIA, Unicode, Zwischenablage) | Umlaute ok | Zwischenablage unverändert | Bemerkung |
| --- | --- | --- | --- | --- |
| Editor (Notepad) | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Word | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Outlook | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Chrome | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Edge | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Firefox | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| VS Code | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Cursor | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Windows Terminal | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Alte Eingabeaufforderung (conhost) | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| PowerShell als Administrator | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Remote-Desktop-Fenster | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| ChatGPT-Desktop-App | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| Codex-Eingabe | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |
| WhatsApp-Desktop | nicht angegeben | nein | nicht angegeben | ß als Doppel-s |

## Befund und Nachtest für das Diktat

Die lokale Windows-Stimme Hedda sprach den Matrixsatz. Bei identischem Audio lieferte primeLine Deutsch schon vor dem Einfügen „Übermass“, Parakeet v3 lieferte „Übermaß“. Ein zweiter Satz zeigte denselben Fehler bei „Grüße“, „Straße“, „draußen“, „großen“ und „Fuß“. Ein Wechsel des Einfügewegs pro Programm würde die Schreibweise des erkannten Textes übernehmen.

Die App korrigiert diese sechs belegten Wortformen beim Diktat mit primeLine. Die Auswahl der Einfügewege bleibt automatisch, sofern du keinen Weg vorgegeben hast. Die Regel arbeitet mit ganzen Wörtern. Weitere Wortformen und die Unterscheidung von „Masse“ und „Maße“ anhand ihrer Bedeutung sind offen. Ein gleich geschriebenes Wort in einem Eigennamen kann ebenfalls von der Korrektur erfasst werden.

Der manuelle Nachtest im Editor ist bestanden. Beim Diktat **„Ein Übermaß an Wasser. Die Masse muss stimmen.“** erschienen „Übermaß“, „Masse“ und „muss“ in der erwarteten Schreibweise. Die Tabelle dokumentiert den ursprünglichen Durchlauf in allen 15 Programmen.

## Einmal vorbereiten

Öffne eine PowerShell mit normalen Benutzerrechten in einem eigenen Fenster. Baue dort das Testprogramm. Es übergibt den Testsatz direkt an die Einfügefunktion und zeigt den verwendeten Weg an. Ein Mikrofon oder Sprachmodell brauchst du dafür nicht.

```powershell
Set-Location -LiteralPath "H:\Quatsch mit Soße"
$env:CARGO_TARGET_DIR = "C:\qms-target"
. ".\scripts\import-msvc.ps1"
Import-MsvcEnvironment
cargo build --locked --offline -p quatsch-mit-sosse-inject --example inject_test
```

Öffne für die Kontrolle ein zweites, leeres Dokument im lokalen Editor. Der Test verwendet die Zwischenablage für eine bekannte Kopie. Sichere vorher gegebenenfalls deren aktuellen Inhalt.

## Ziel für jede Zeile

- Öffne in **Editor und Word** jeweils ein leeres Dokument. Verwende in **VS Code und Cursor** jeweils eine neue Textdatei.
- Klicke in **Outlook** in den Nachrichtentext eines neuen Entwurfs. Verwende in der **ChatGPT-Desktop-App**, der **Codex-Eingabe** und **WhatsApp-Desktop** jeweils das Nachrichtenfeld. Lass den Text als Entwurf stehen.
- Öffne in **Chrome, Edge und Firefox** jeweils `data:text/html,<textarea autofocus></textarea>` und klicke in das Textfeld der lokalen Seite.
- Öffne in **Windows Terminal** ein neues Fenster mit einer leeren Befehlszeile. Starte für die **alte Eingabeaufforderung** über Win+R `conhost.exe cmd.exe`. Öffne für **PowerShell als Administrator** ein eigenes erhöhtes Fenster. Starte das Testprogramm für alle drei Ziele weiterhin aus der normalen PowerShell. Lass den eingefügten Text auf der Befehlszeile stehen, ohne ihn auszuführen.
- Öffne im **Remote-Desktop-Fenster** ein leeres Editor-Dokument der verbundenen Sitzung. Starte das Testprogramm auf dem lokalen Rechner. Vermerke, ob die Zwischenablageumleitung in der Verbindung aktiv war, sofern bekannt.

## Pro Programm einmal durchführen

1. Tippe `[]` ins Zielfeld und setze die Einfügemarke mit der linken Pfeiltaste zwischen die Klammern. So fällt auf, wenn vorhandener Text ersetzt wird oder der Satz an der falschen Stelle landet.
2. Wechsle zur lokalen Start-PowerShell und führe beide Zeilen zusammen aus. Die erste setzt die Kontrollkopie, die zweite startet den Versuch. Sobald die Drei-Sekunden-Meldung erscheint, wechsle mit Alt+Tab zurück ins vorbereitete Ziel und lass die Tasten los. Lass das Fenster bis zum Ende des Versuchs im Vordergrund und kopiere währenddessen nichts.

   ```powershell
   Set-Clipboard -Value "QMS-Probe 47"
   & "C:\qms-target\debug\examples\inject_test.exe" --matrix
   ```

3. Vergleiche das sichtbare Ergebnis mit `[Äpfel, Öl & Übermaß – 3,5 % für 12,99 €]`. Trage unter **Umlaute ok** „ja“ ein, wenn Ä, Ö, Ü, ü und ß korrekt erscheinen, sonst „nein“. Prüfe auch, ob der ganze Satz genau einmal zwischen den Klammern steht. Beschreibe unter **Bemerkung** jede Abweichung, etwa ein fehlendes €-Zeichen, einen falschen Gedankenstrich oder eine verschwundene Klammer.
4. Lies in der Start-PowerShell die Meldung „Verwendeter Einfügeweg“. Trage für „UI Automation“ **UIA**, für „Unicode-Tastatureingabe“ **Unicode** und für „Zwischenablage“ **Zwischenablage** ein. Bei einer Fehlermeldung trage im Feld **Weg** einen Strich und unter **Bemerkung** den Fehler ein. Halte auch fest, wenn die Meldung Erfolg anzeigt, aber im Ziel Text fehlt. Falls für das Programm bereits ein Weg in `%LOCALAPPDATA%\QuatschMitSosse\data\inject.toml` vorgegeben ist, vermerke das.
5. Drücke im leeren Kontrolldokument im **lokalen Editor** Strg+V. Erscheint genau `QMS-Probe 47`, trage unter **Zwischenablage unverändert** „ja“ ein. Andernfalls trage „nein“ ein und beschreibe, was erschien. Leere das Kontrolldokument für die nächste Zeile mit Strg+A und Entf.

Die Kontrollkopie prüft Klartext. Möchtest du zusätzlich die Formatierung prüfen, kopiere ein fett formatiertes Wort aus Word. Wiederhole den Versuch ab dem Start der EXE ohne `Set-Clipboard` und füge danach in ein leeres Word-Dokument mit Quellformatierung ein. Vermerke unter **Bemerkung**, ob Wort und Fettdruck erhalten blieben.

Fehlt ein Programm oder eine RDP-Verbindung, trage „nicht getestet“ mit dem Grund unter **Bemerkung** ein. Lass die drei Ergebnisfelder dieser Zeile frei.
