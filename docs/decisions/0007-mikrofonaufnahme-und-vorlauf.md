# 0007: Mikrofonaufnahme, Vorlauf und Geräteausfälle

Datum: 19.11.2026 · Schritt 1.1

## Entscheidung

`crates/audio` verwendet CPAL 0.18.2 mit dem ausdrücklich gewählten WASAPI-Host. Ein eigener Verwaltungs-Thread besitzt den Stream, die Formatumwandlung und den Aufnahmepuffer. Die Desktop-App wird in diesem Schritt noch nicht mit der Aufnahme verbunden.

Der CPAL-Audiocallback normalisiert native Samples nach `f32` und übergibt feste Blöcke über eine auf 64 × 2048 Samples begrenzte Warteschlange. Er führt keine Dateioperationen und keine Anwender-Callbacks aus. Ein separater Fehlerkanal meldet Streamausfälle. Jeder Stream bekommt eigene Kanäle, damit verspätete Daten oder Fehler eines früheren Geräts keinen Ersatzstream beeinflussen.

## Signalverarbeitung und Aufnahmegrenzen

Mehrere Kanäle werden gemittelt. Ein kausaler Blackman-Sinc-Filter mit vorab berechneten Phasen verhindert Aliasing beim Resampling auf 16 kHz. Der ganzzahlige Ausgabetakt vermeidet Drift; die Verarbeitung bleibt auch bei unvollständigen Mehrkanal-Frames unabhängig von Callback-Grenzen. Nicht endliche Werte werden zu null, übersteuerte Werte auf −1 bis +1 begrenzt. Bei 16 kHz entfällt das Filter.

Die Filterverzögerung beträgt bei 48 kHz 2 ms, bei 44,1 kHz rund 2,02 ms und bei 8 kHz 4 ms. Aufnahmegrenzen beziehen sich auf die bereits ausgegebenen Samples; zusätzlich existiert die Pufferlatenz des Treibers. Die letzten noch im Filter befindlichen Millisekunden werden beim Beenden eines Streams nicht mit künstlicher Stille ausgespült.

In Bereitschaft werden ausschließlich die neuesten 8000 Mono-Samples als Vorlauf gehalten. Eine Aufnahme kopiert diesen Vorlauf genau einmal und hängt danach neue Samples an. Direkt nach dem Aktivieren oder einem Gerätewechsel steht zunächst weniger als eine halbe Sekunde echter Vorlauf zur Verfügung; fehlendes Audio wird nicht durch erfundene Stille ersetzt. Das Beispiel wartet deshalb vor dem Start auf mindestens eine halbe Sekunde fließendes Audio. Die zehn Sekunden nach dem Start ergeben mit vollem Vorlauf ungefähr 10,5 Sekunden WAV.

RMS wird über aufeinanderfolgende Fenster von 533 beziehungsweise 534 Ausgabesamples berechnet: exakt 30 Fenster pro Sekunde, Werte zwischen 0 und 1. Bei Streamende wird zusätzlich null gemeldet, damit kein alter Pegel stehen bleibt. Die Callbacks laufen auf dem Verwaltungs-Thread und müssen kurz bleiben; synchrones Zurückrufen in dieselbe `AudioCapture`-Instanz würde blockieren. UI-Anbindung muss Ereignisse daher weiterreichen.

## Gerätewahl und Wiederherstellung

`input_devices` und `default_input_device` öffnen keinen Aufnahmestream. Gerätewahl erfolgt über den exakten Namen. Doppelte Namen werden als mehrdeutig gemeldet; der Mensch kann Geräte in Windows eindeutig benennen.

Für die Standardwahl wird CPALs echtes Standardgerät benutzt, damit dessen WASAPI-Überwachung Änderungen des Windows-Standardmikrofons meldet. Ein benanntes Gerät bleibt bis zur Neuwahl oder einem Ausfall ausgewählt. Bei Ausfall wird auf das dann aktuelle Standardgerät zurückgefallen. Der Rückfall bleibt bestehen, bis ausdrücklich wieder ein anderes Gerät gewählt wird; ein erneut angestecktes Wunschgerät übernimmt somit nicht überraschend die Aufnahme.

Falls ein Treiber weder Daten noch Fehler liefert, löst eine Frist von zwei Sekunden denselben Rückfall aus. Ohne verfügbares Standardgerät bleibt die Bereitschaft aktiv und versucht es alle 500 ms erneut. Wiederholte identische Fehler werden nicht als Ereignisflut ausgegeben. Beim Gerätewechsel bleibt eine laufende Aufnahme erhalten; Vorlauf, Resampler und RMS-Fenster werden zurückgesetzt. Fehlende Zeit während eines Ausfalls wird nicht mit Stille aufgefüllt und kann nicht rekonstruiert werden.

Ein Überlauf der begrenzten Datenwarteschlange wird ausdrücklich gemeldet. Der Stream wird neu aufgebaut und der unvollständige Vorlauf verworfen, damit verlorene Teilframes insbesondere bei Mehrkanalgeräten nicht zu falsch zugeordneten Kanälen führen.

Kurze vom Treiber gemeldete Datenunterbrechungen (`Xrun`) werden ebenfalls gemeldet, starten aber dasselbe ausgewählte Gerät neu. Fehlende Echtzeitpriorität (`RealtimeDenied`) ist nur ein Hinweis; die Aufnahme läuft weiter. Hinweise und fatale Fehler benutzen getrennte Kanäle, sodass eine volle Hinweiswarteschlange keinen Geräteausfall verschluckt.

`finish_recording` beendet nur die Aufnahme. `stop_ready` gibt dagegen den Stream frei, beendet den Verwaltungs-Thread und wartet auf dessen Ende; auch `Drop` räumt auf. Anschließend existieren weder Polling noch Audio-Threads dieser Instanz. Eine noch laufende Aufnahme wird von `stop_ready` zurückgegeben. Erneute Bereitschaft beginnt mit einer neuen Instanz.

Die API-Methoden warten auf die Verarbeitung ihres Befehls und gehören deshalb nicht direkt in den UI-Thread. Die Streamerstellung erhält ein Zwei-Sekunden-Limit; CPALs vorherige Abfrage des Standardformats kann intern jedoch ohne dieses Limit einen nativen Treiber aktivieren. Für fehlerhafte, dauerhaft hängende Audiotreiber kann deshalb keine feste Höchstdauer für Start oder Stop zugesichert werden.

## Datenschutz und Nachweis

Die Library schreibt kein Audio auf Datenträger und enthält weder Netzwerk noch Logging. `Recording` gibt über `Debug` nur die Sampleanzahl aus. Ausschließlich das ausdrücklich gestartete Beispiel schreibt eine lokale WAV-Datei in den bereits ignorierten Ordner `testdata/private/`.

Hardwarefreie Tests prüfen Ringreihenfolge, Vorlaufgrenzen, wiederholte Aufnahme, Downmix, Sinussignale, Aliasing-Unterdrückung, Blockgrenzen, RMS, Gerätefehler, verzögerte Fehler alter Streams, Rückfall, Wiederanschließen, Datenstillstand, Überlauf und Freigabe beim Stoppen/Verwerfen. Die tatsächliche Aufnahmequalität, Windows-Mikrofonberechtigung und das Abziehen realer Geräte bleiben manuelle Abnahme.

CPAL-Verhalten wurde anhand der [offiziellen API](https://docs.rs/cpal/0.18.2/cpal/) und der [WASAPI-Streamimplementierung](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs) geprüft. Insbesondere beendet `Stream::drop` den CPAL-Thread und wartet auf ihn; die Anwendung hält dabei keine Callback-Sperre.
