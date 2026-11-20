# 0015 – Diktat und Mikrofonbereitschaft

20.11.2026 · Schritt 2.4

## Wann das Mikrofon läuft

Nach dem ersten erfolgreichen Diktat im Editor fiel die anhaltende Windows-Mikrofonanzeige auf. Auf ausdrücklichen Nutzerwunsch ersetzt der Zugriff per Tastendruck die ursprünglich in Schritt 2.4 vorgesehene zehnminütige Bereitschaft. Die Tray-App öffnet das Windows-Standardmikrofon erst beim Diktatkürzel. Bei fehlenden Modellen öffnet das Kürzel die Modellverwaltung. Nach der Installation wartet die App auf einen neuen Tastendruck.

Loslassen beendet die Aufnahme und gibt den Eingabestream samt Ringpuffer vor der Erkennung frei. Esc verwirft die Aufnahme und gibt das Mikrofon ebenfalls frei. Das erneute Einfügen verwendet ausschließlich den Text im Arbeitsspeicher.

## Erster Wortanfang

Bei jedem Diktat beginnt der Ringpuffer mit dem Öffnen des Streams. Audio aus der Zeit vor dem Tastendruck steht deshalb nicht zur Verfügung. Spricht man bereits beim Drücken, kann der erste Laut fehlen.

Für einen vollständigen Wortanfang erst sprechen, wenn „Hört zu“ erscheint. Diese Anzeige folgt auf die Bestätigung der Aufnahme durch das Audiomodul. Eine zusätzliche Wartezeit von 500 ms würde den bereits verstrichenen Ton nicht zurückholen. Als spätere Bedienhilfe bietet sich ein kurzer, optionaler Bereitschaftston nach dem Aufwecken an.

## Erkennung, Einfügen und Wiederherstellung

Die lokale SpeechEngine verarbeitet die vollständige Monoaufnahme mit 16 kHz einschließlich des verfügbaren Vorlaufs. Silero erkennt Sprachabschnitte. Für deren Erkennung gilt die Reihenfolge aus Entscheidung 0001. Vollständig installiertes primeLine Deutsch hat Vorrang, andernfalls wird ein installiertes Parakeet v3 verwendet. Meldet die Engine beim Laden von primeLine einen Modellfehler, zeigt das Overlay den Wechsel an und die App versucht dieselbe Aufnahme einmal mit installiertem Parakeet v3. Mikrofonfehler und Silero-Fehler werden direkt angezeigt. Die Modelle werden bei Bedarf geladen und durch die Leerlaufverwaltung der Engine nach fünf Minuten freigegeben.

Die App merkt sich beim Start das Zielfenster. Während der Erkennung bleibt die Oberfläche bedienbar. Vor dem Einfügen speichert die App den neuen Text im Arbeitsspeicher und prüft das Ziel erneut. Stille, ein Abbruch oder ein Erkennungsfehler lassen das letzte erfolgreiche Ergebnis erhalten. Nach einem Einfügefehler kann man es im Overlay kopieren oder in einem neu gewählten Textfeld per Kürzel oder Tray erneut einfügen. Beim Beenden verfällt dieser Speicher.

Die Leerlaufmessung verwendet denselben Startweg wie das Diktat. Der Marker `QMS_READY` belegt jetzt den aufgebauten Tray, die registrierten globalen Kürzel und die laufende Ereignisschleife. Die Initialisierung des Mikrofontreibers läuft auf einem eigenen Thread. Die Erkennung läuft erst nach einem Diktat.
