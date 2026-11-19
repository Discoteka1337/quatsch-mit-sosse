# 0011 – Korpus-Recorder für Schritt 1.4

Datum: 19.11.2026

## Entscheidung

Der Recorder verwendet ausschließlich `crates/audio`: Bereitschaft mit 500 ms Vorlauf, Start und Stopp einzelner Aufnahmen, 16 kHz mono. Audio wird als Float32-WAV ohne weitere Umwandlung gespeichert. Die Bereitschaft bleibt bis zum Sitzungsende aktiv und wird dann auch bei Fehlern freigegeben. Der CLI-Schritt benötigt weder Modelle noch Netzwerk.

Der Masterplan legt zehn Aufnahmen ohne Sprache fest, ohne eine Verteilung vorzugeben. Wir verwenden fünf Stilleaufnahmen (041–045) und fünf Aufnahmen mit Tastaturgeräuschen (046–050). Die vorhergehenden 40 Nummern entsprechen genau der verlangten Kategorienreihenfolge. Die Ansagen für die letzten zehn Nummern sind keine Referenztexte; ihre TXT-Dateien sind leer.

Enter startet und stoppt. Ein weiteres Enter bestätigt den Versuch und speichert ihn; davor kann W ihn wiederholen oder S die Nummer überspringen. So kann eine unpassende Aufnahme verworfen werden, bevor Dateien entstehen. W und S sind Einzel-Tasten. Während einer laufenden Tastaturaufnahme haben sie keine Steuerwirkung, damit normales Tippen die Aufnahme nicht unterbricht. Escape und Strg+C beenden aus jedem Eingabeschritt; während der anfänglichen Mikrofonbereitschaft ist eine Wartezeit von höchstens zehn Sekunden möglich.

Der Masterplan verlangt 10 bis 40 Sekunden lange Diktate. Die Satzliste orientiert sich an seinen 126 Wörtern pro Minute; tatsächliches Tempo und ausgesprochene Zahlen erfordern eine manuelle Kontrolle. Die CLI zeigt die Dauer an und warnt außerhalb der Zielzeit, ohne Audio abzuschneiden oder ungefragt zu verwerfen.

## Dateien und Datenschutz

Satzliste, WAV und Referenzen liegen ausschließlich unter `testdata/private/` und bleiben von Git ausgeschlossen. Auch die generierte Satzliste wird nicht als öffentliche Kopie in den Quellcode aufgenommen. Ein frischer Clone enthält deshalb keine private Satzliste.

Die Satzanzeige ist die ausdrücklich gewünschte interaktive Benutzeroberfläche. Die CLI akzeptiert dafür ausschließlich echte Windows-Konsolenhandles für Ein- und Ausgabe; Pipes und Dateiumleitungen werden vor dem Lesen der Liste oder Öffnen des Mikrofons abgewiesen. Audio, Referenzen und eingegebene Tasten werden nicht geloggt. Fehlertexte enthalten keine Satzinhalte.

Ein bestätigter Versuch wird zuerst in temporäre Dateien im privaten Zielordner geschrieben und synchronisiert. Beide fertigen Dateien werden ohne Überschreiben veröffentlicht. Bei einem Fehler mit der zweiten Datei wird die neu veröffentlichte erste Datei wieder entfernt. Zwei getrennte Dateien bilden keine atomare Transaktion bei Prozessabbruch oder Stromausfall; ein unvollständiges Paar wird beim nächsten Start erkannt und verlangt eine manuelle Prüfung. Bereits vorhandene Paare bleiben unangetastet, fehlende Nummern können beim Neustart ergänzt werden. Dies bewahrt auch manuell korrigierte Referenzen.

## Prüfung

Tests wurden vor der jeweiligen Implementierung angelegt. Synthetische Audiodaten prüfen WAV-Format, vollständige Samples einschließlich Vorlauf, Referenzen, Nummerierung, Wiederholung, Überspringen, Neustart, Abbruch und Speicherfehler. Tests für Tasteneingabe und Mikrofonzustand prüfen gehaltene Tasten, Gerätewechsel, Unterbrechungen und Bereitschaft. CLI-Tests prüfen Hilfe ohne Mikrofon und Ablehnung von Umleitungen. Die lokale Satzliste wurde auf Anzahl, Verteilung und Wortlängen geprüft, ohne ihren Inhalt auszugeben.

Die automatischen Tests öffnen kein Mikrofon und erzeugen keine echten Aufnahmen. Einsprechen, Anhören und Kontrolle der Referenzen bleiben gemäß Masterplan manuelle Schritte. Der Benchmark aus Schritt 1.5 ist noch nicht vorgesehen.
