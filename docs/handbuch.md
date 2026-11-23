# Diktieren mit Quatsch mit Soße

## Dein erstes Diktat mit den Standardkürzeln

1. Starte „Quatsch mit Soße“ und setze den Cursor in ein Textfeld, etwa im Windows-Editor.
2. Halte **Strg+Umschalt+Leertaste**. Beim ersten Aufruf mit fehlenden Modellen öffnet sich die Modellverwaltung.
3. Klicke dort auf **Herunterladen**. Sobald die Modelle bereit sind, schließe das Fenster und wähle dein Textfeld erneut.
4. Halte zum Diktieren **Strg+Umschalt+Leertaste** und sprich, sobald im Overlay **Hört zu** erscheint.
5. Lass das Kürzel los. Die App gibt das Mikrofon frei. Lass das Zielfenster im Vordergrund, bis dein Text erscheint.
6. Um eine laufende Aufnahme zu verwerfen, drücke **Esc**, während du das Diktatkürzel hältst.
7. Mit **Strg+Win+F8** fügst du das letzte erkannte Diktat am aktuellen Cursor erneut ein.
8. Alternativ wähle zuerst das Textfeld und dann am Tray-Symbol **Letzten Text erneut einfügen**.
9. Bei einem Einfügefehler übernimmt **Kopieren** im Overlay den erkannten Text. Füge ihn mit **Strg+V** selbst ein.
10. Zwischen Diktaten ist das Mikrofon für andere Apps frei. Mit **Beenden** im Tray schließt du die App und verwirfst ihren Textspeicher.

## Änderungen anzeigen und das Original zurückholen

Setze den Cursor in das gewünschte Textfeld und drücke **Strg+Win+F9**. Falls dieses Kürzel belegt ist, findest du die verwendete Kombination unter **Einstellungen → Allgemein**. Neben dem Overlay erscheint das letzte Diktat.

Unter **Vorher** steht der Wortlaut des Erkenners. Bei „Ich möchte, äh, nach Hause“ siehst du das entfernte „äh“ rot und durchgestrichen. Bei aktiver Zeichensetzung steht unter **Nachher** „Ich möchte nach Hause.“ Hinzugefügter Text erscheint grün und unterstrichen. Gelb kennzeichnet Stellen, an denen eine Selbstkorrektur mehrdeutig war.

**Original einfügen** schreibt den Rohtext an den Cursor im vorherigen Fenster. Um den bereinigten Text zu ersetzen, markiere ihn vor dem Öffnen der Ansicht. **Original kopieren** legt den Rohtext in die Zwischenablage. Mit **Esc** schließt du die Ansicht und kannst im vorherigen Fenster weiterschreiben.

Bei langen Vergleichen blätterst du mit **Zurück** und **Weiter**. Beide Original-Aktionen verwenden den gesamten Rohtext. Für einen Ausschnitt markierst du die gewünschte Stelle und drückst **Strg+C** oder wählst **Kopieren** im Kontextmenü. Diese Kopien erhalten die Windows-Schutzkennzeichen für Zwischenablageverlauf und Cloud-Synchronisierung.

Die App hält Rohtext und Endtext jedes erkannten Diktats bis zum Beenden im Arbeitsspeicher. Auch wenn ein Diktat nur aus einem entfernten Füllwort bestand, kannst du das Original zurückholen.

## Allgemeine Einstellungen

Öffne **Einstellungen** über das Tray-Symbol. Auf der Seite **Allgemein** kannst du folgende Einstellungen ändern.

- **Tastenkürzel und Diktatmodus.** Wähle das Aufzeichnungsfeld für Diktat oder erneutes Einfügen und drücke dein gewünschtes Kürzel. Mit **Esc** brichst du die Eingabe ab. Bei einem bekannten Konflikt erscheint eine Warnung. Im Modus **Halten** endet die Aufnahme beim Loslassen, im Modus **Umschalten** beim zweiten Drücken. **Esc** verwirft eine laufende Aufnahme in beiden Modi.
- **Mikrofon.** Wähle das Windows-Standardgerät oder ein bestimmtes Mikrofon. Klicke auf **Mikrofon testen** und sprich einige Sekunden. Der Pegel zeigt, ob dein Gerät ein Signal liefert. Mit **Test beenden** kannst du die Prüfung vorzeitig beenden.
- **Sprachmodell.** Die automatische Auswahl bevorzugt ein installiertes primeLine Deutsch und verwendet andernfalls ein installiertes Parakeet v3. Du kannst auch ein bestimmtes Sprachmodell auswählen und seinen Installationsstatus sehen. **Download öffnen …** zeigt die benötigten Modelle und die Downloadgröße. Erst **Herunterladen** startet den Download mit SHA256-Prüfung. **Entfernen …** fragt nach, bevor die App das gewählte Modell löscht.
- **Töne und Lautstärke.** Ein kurzer aufsteigender Ton meldet den Aufnahmestart, ein absteigender Ton das Ende. Über **Start- und Endton abspielen** kannst du beide ausschalten. Im Windows-Mixer stehen die Lautstärkeregler anderer Apps während der Aufnahme standardmäßig bei höchstens 20 %. Eine zuvor auf 60 % eingestellte App spielt also mit 20 % und erhält danach ihre 60 % zurück. Bei 100 % entfällt die Absenkung. Nach einem Absturz öffne Quatsch mit Soße erneut, damit die App die Mixerwerte wiederherstellt.
- **Autostart.** Aktiviere **Bei Windows-Anmeldung starten**, wenn die App bei der nächsten Anmeldung in deinem Windows-Konto im Tray bereitstehen soll. Entferne das Häkchen, um den Autostart auszuschalten.
- **Overlay.** **Bereit-Punkt anzeigen** macht das Overlay auch zwischen den Diktaten sichtbar. Mit **In Bildschirmaufnahmen verbergen** bittest du Windows, das Overlay bei unterstützten Aufnahmeverfahren auszublenden. Zum Verschieben ziehst du den Punkt oder die Diktatleiste.

Die App speichert deine Auswahl unter `%LOCALAPPDATA%\QuatschMitSosse\data\settings.json`. Beim nächsten Start gelten die zuletzt gespeicherten Einstellungen.

Unter **Deutsche Bereinigung** kannst du Wörterbuch, Selbstkorrekturen, Zahlen, Zeichensetzung und Snippets einzeln schalten. **Sprachbefehle ausführen** ist anfangs aus. Schalte es ein, wenn zum Beispiel „neuer Absatz“ einen Absatz einfügen soll. Bei **Füllwörter entfernen** lässt „Standard“ Wörter wie „also“ und „halt“ stehen. „Stark“ entfernt sie ebenfalls. Unter **Zahlwörter als Ziffern ab** ist 13 voreingestellt. Eine neue Schwelle wird beim Verlassen des Feldes gespeichert.

## Wörterbuch

Öffne im Tray **Einstellungen** und dann **Wörterbuch**. Unter **Begriff** steht die gewünschte Schreibweise, etwa „Codex“. Trage unter **Gesprochene Varianten** je Zeile eine Form ein, die der Erkenner liefert, etwa „kodex“. Das Feld **Ersetzung (optional)** ist für eine abweichende Ausgabe gedacht. Trägst du beim Begriff „Büro Nord“ die Ersetzung „Projektbüro Nord“ ein, erscheint im Diktat „Projektbüro Nord“.

Über **Neuer Eintrag** und **Hinzufügen** legst du einen Begriff an. Zum Bearbeiten suchst du nach dem Begriff oder einer Variante, wählst den Treffer aus und klickst auf **Änderungen speichern**. Gespeicherte Änderungen gelten beim nächsten Diktat. **Löschen** entfernt den ausgewählten Eintrag nach Bestätigung.

Die längste passende Variante gewinnt. Ist dieselbe gesprochene Variante bei zwei verschiedenen Schreibweisen hinterlegt, behält das Wörterbuch an dieser Stelle den erkannten Text bei.

**CSV exportieren …** schreibt das gesamte Wörterbuch als UTF-8-Datei. Du kannst diese Datei über **CSV importieren …** wieder einlesen. Für eine eigene Liste genügt eine Spalte mit der Überschrift `term`. Der vollständige Export enthält die Spalten `term,replacement,aliases,source,created_at`. Varianten stehen als JSON-Liste in einem CSV-Feld, der Erstellungszeitpunkt in Unix-Sekunden. Wähle für Wörterbuch und Snippets eine Datei auf einem lokalen Laufwerk. Die App weist Netzwerkfreigaben und verbundene Netzlaufwerke beim Import und Export zurück. Ein vorhandener Begriff wird beim Import aktualisiert. Enthält die Datei einen ungültigen Datensatz, verwirft die App den gesamten Import.

Die lokale Datenbank liegt unter `%LOCALAPPDATA%\QuatschMitSosse\data\quatsch-mit-sosse.db`. Vor jeder Änderung der Datenbankstruktur legt die App im Unterordner `backups` eine Sicherung des bisherigen Stands an.

## Snippets

Öffne **Einstellungen → Snippets**. Sprich „Gedanken ordnen Prompt“ als eigenen Satz. Im Zielfeld erscheint der gespeicherte Arbeitsauftrag. In einem Satz wie „Ich möchte den Gedanken ordnen Prompt ändern“ bleiben die gesprochenen Wörter stehen.

Für deine Adresse wählst du **meine E-Mail-Adresse**, trägst sie unter **Einzufügender Text** ein und klickst auf **Änderungen speichern**. Solange das Feld leer ist, bleibt der Auslöser beim Diktieren erhalten. Unter **Neuer Eintrag** kannst du eigene Vorlagen anlegen. Der Text darf mehrere Absätze enthalten.

| Platzhalter | Ergebnis beim Diktieren |
| --- | --- |
| `{datum}` | Lokales Datum, etwa `22.11.2026` |
| `{uhrzeit}` | Lokale Uhrzeit, etwa `09:07` |
| `{zwischenablage}` | Aktueller Text aus der Windows-Zwischenablage. Enthält sie nur ein Bild, bleibt die Stelle leer. |
| `{cursor}` | Gewünschte Schreibposition im Zielfeld. Bei mehreren Marken gilt die erste. |

Für einen Brief kannst du etwa `Hallo {cursor},` als erste Zeile und deinen Gruß als letzte Zeile speichern. In einem unterstützten Zielfeld kannst du nach dem Diktieren direkt den Namen tippen. **Letzten Text erneut einfügen** verwendet die beim Diktat eingesetzten Werte und dieselbe gewünschte Cursorposition.

Die Suche findet Auslöser und Vorlagentexte. **CSV exportieren …** schreibt alle Snippets mit den Spalten `trigger,text,created_at`. Zum Import genügen `trigger,text`. Setze mehrzeilige CSV-Felder in doppelte Anführungszeichen. Ein vorhandener Auslöser wird aktualisiert. Bei einer fehlerhaften Zeile verwirft die App den gesamten Import.
