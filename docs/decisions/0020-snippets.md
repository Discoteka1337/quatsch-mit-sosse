# 0020 · Platzhalter und Cursor in Snippets

- Eine leere Vorlage ist inaktiv. So kann „meine E-Mail-Adresse“ bis zum Eintragen der eigenen Adresse als gewöhnlicher Satz diktiert werden.
- Der Vergleich berücksichtigt Groß- und Kleinschreibung, Satzzeichen und die üblichen gesprochenen Schreibweisen von Bindestrichwörtern. Zuerst wird das gesamte Diktat geprüft, danach jeder einzelne Satz. Passen mehrere Auslöser, entscheidet die genaue Worttrennung. Ist die Zuordnung weiterhin mehrdeutig, bleibt der Satz erhalten.
- Die App setzt lokales Datum und lokale Uhrzeit im Format `TT.MM.JJJJ` und `HH:MM` ein. Sie liest die Zwischenablage erst, wenn eine passende Vorlage den Platzhalter benötigt. Enthält sie keinen Text, ist sein Wert leer. Scheitert der Zugriff, bleiben die betroffenen Auslöser im Diktat stehen und das Overlay meldet den Fehler.
- Platzhalter werden nur im gespeicherten Vorlagentext ausgewertet. Beispielsweise wird ein aus der Zwischenablage übernommenes `{datum}` wörtlich eingefügt. Die erste Cursormarke im Ergebnis bestimmt die Schreibposition. Weitere Marken entfallen.
- Nach erfolgreichem Einfügen setzt die App den Cursor, wenn sie den Fokus und die Zielposition im Eingabefeld bestätigen kann. Ein Fehler beim Setzen der Position löst keine zweite Einfügung aus.
