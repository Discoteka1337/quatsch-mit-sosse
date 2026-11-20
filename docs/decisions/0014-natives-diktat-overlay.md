# 0014 – Natives Diktat-Overlay

20.11.2026 · Schritt 2.3

Die manuelle Abnahme im Entwicklungsmodus ist bestanden. Der verschobene Bereitschaftspunkt erschien nach dem Neustart an derselben Stelle.

Das Overlay verwendet das native Tauri-Fenster aus Entscheidung 0006. Windows zeichnet die Leiste in der aktuellen hellen oder dunklen App-Farbeinstellung. Sie misst 160 × 36 logische Pixel. Für den Administratorhinweis und für Fehler wird sie breiter und höher, damit Erklärung und Kopierknopf lesbar sind.

Im Auslieferungszustand ist „Bereit“ ausgeblendet. In den Tray-Einstellungen lässt sich ein dezenter Punkt einschalten. Seine Fensterregion begrenzt zugleich die Fläche, auf der er Mausklicks annimmt. Punkt und Leiste lassen sich ziehen. Das Fenster verwendet von der Erstellung an `WS_EX_NOACTIVATE`. Anzeigen, Verschieben und Kopieren erhalten den Fokus der Zielanwendung.

Die beiden Optionen, der zuletzt verwendete Monitor und die Positionen pro Monitor stehen in `%LOCALAPPDATA%\QuatschMitSosse\data\overlay.json`. Die Position bezieht sich auf die Arbeitsfläche des jeweiligen Monitors. Bei einem kleineren Arbeitsbereich rückt eine teilweise sichtbare Leiste vollständig ins Bild. Fehlt der zuletzt verwendete Monitor, verwendet die App die gespeicherte Position des Hauptmonitors. Ohne sichtbare gespeicherte Position erscheint die Leiste unten mittig oberhalb der Taskleiste. Beim Wiederanschließen kann die App die vorige Position wiederherstellen.

„In Bildschirmaufnahmen verbergen“ setzt `WDA_EXCLUDEFROMCAPTURE` am Overlay. Die Option ist anfangs ausgeschaltet. Windows wendet sie bei unterstützten Aufnahmeverfahren an. Der Laufzeittest liest den Wert am Fenster aus. [Microsoft zu SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)

Schritt 2.4 kann die Anzeige über `set_overlay_state` ansteuern. Der Hörzustand übernimmt den Pegel in Prozent. Ein Fehler übernimmt eine lokalisierte Erklärung und den noch verfügbaren Diktattext. „Kopieren“ legt den Text auf ausdrücklichen Klick mit den Windows-Schutzformaten für Zwischenablageverlauf und Cloud-Abgleich ab. Fehlt der Diktattext, kopiert der Knopf die Erklärung. Nach erfolgreichem Kopieren kehrt die Anzeige zu „Bereit“ zurück. Bei belegter Zwischenablage bleibt der Text für einen weiteren Versuch im Arbeitsspeicher.
