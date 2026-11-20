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

## Allgemeine Einstellungen

Öffne **Einstellungen** über das Tray-Symbol. Auf der Seite **Allgemein** kannst du folgende Einstellungen ändern.

- **Tastenkürzel und Diktatmodus.** Wähle das Aufzeichnungsfeld für Diktat oder erneutes Einfügen und drücke dein gewünschtes Kürzel. Mit **Esc** brichst du die Eingabe ab. Bei einem bekannten Konflikt erscheint eine Warnung. Im Modus **Halten** endet die Aufnahme beim Loslassen, im Modus **Umschalten** beim zweiten Drücken. **Esc** verwirft eine laufende Aufnahme in beiden Modi.
- **Mikrofon.** Wähle das Windows-Standardgerät oder ein bestimmtes Mikrofon. Klicke auf **Mikrofon testen** und sprich einige Sekunden. Der Pegel zeigt, ob dein Gerät ein Signal liefert. Mit **Test beenden** kannst du die Prüfung vorzeitig beenden.
- **Sprachmodell.** Die automatische Auswahl bevorzugt ein installiertes primeLine Deutsch und verwendet andernfalls ein installiertes Parakeet v3. Du kannst auch ein bestimmtes Sprachmodell auswählen und seinen Installationsstatus sehen. **Download öffnen …** zeigt die benötigten Modelle und die Downloadgröße. Erst **Herunterladen** startet den Download mit SHA256-Prüfung. **Entfernen …** fragt nach, bevor die App das gewählte Modell löscht.
- **Töne und Lautstärke.** Ein kurzer aufsteigender Ton meldet den Aufnahmestart, ein absteigender Ton das Ende. Über **Start- und Endton abspielen** kannst du beide ausschalten. Im Windows-Mixer stehen die Lautstärkeregler anderer Apps während der Aufnahme standardmäßig bei höchstens 20 %. Eine zuvor auf 60 % eingestellte App spielt also mit 20 % und erhält danach ihre 60 % zurück. Bei 100 % entfällt die Absenkung. Nach einem Absturz öffne Quatsch mit Soße erneut, damit die App die Mixerwerte wiederherstellt.
- **Autostart.** Aktiviere **Bei Windows-Anmeldung starten**, wenn die App bei der nächsten Anmeldung in deinem Windows-Konto im Tray bereitstehen soll. Entferne das Häkchen, um den Autostart auszuschalten.
- **Overlay.** **Bereit-Punkt anzeigen** macht das Overlay auch zwischen den Diktaten sichtbar. Mit **In Bildschirmaufnahmen verbergen** bittest du Windows, das Overlay bei unterstützten Aufnahmeverfahren auszublenden. Zum Verschieben ziehst du den Punkt oder die Diktatleiste.

Die App speichert deine Auswahl unter `%LOCALAPPDATA%\QuatschMitSosse\data\settings.json`. Beim nächsten Start gelten die zuletzt gespeicherten Einstellungen.
