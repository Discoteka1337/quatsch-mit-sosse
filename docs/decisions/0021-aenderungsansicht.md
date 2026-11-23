# 0021 · Änderungsansicht und Originaltext

23.11.2026 · Schritt 3.5

Die App übernimmt den Rohtext unmittelbar vom Erkenner und den Endtext samt Unsicherheitsmarkierungen und gewünschter Cursorposition aus demselben Bereinigungslauf. Die Desktop-App wendet die in „Allgemein“ gespeicherte Regelauswahl an. Im Auslieferungszustand sind Wörterbuch, Füllwörter in Standardstärke, Selbstkorrekturen, Zahlen, Zeichensetzung und Snippets aktiv. Sprachbefehle lassen sich zuschalten. Das Wörterbuch schützt persönliche Schreibweisen auch bei abgeschalteter Ersetzung. Kann die App die benötigten Einträge nicht lesen, übernimmt sie den Rohtext unverändert als Endtext und zeigt den Grund im Overlay an.

Jedes erkannte Diktat erhält einen Eintrag im Arbeitsspeicher, auch bei leerem Endtext oder fehlgeschlagenem Einfügen. Stille und abgebrochene Aufnahmen erzeugen keinen Eintrag. Die Änderungsansicht zeigt das jüngste Diktat. Das bestehende Kürzel zum erneuten Einfügen verwendet weiterhin den jüngsten einfügbaren Endtext. Die dauerhafte Speicherung und die Auswahl älterer Diktate sind für Schritt 4.2 vorgesehen.

Die Original-Aktionen beziehen sich auf das Diktat im geöffneten Fenster. „Original einfügen“ verwendet die Einfügeroutine am aktuellen Cursor oder an der Auswahl des bei Aufruf erfassten Ziels. Wer eine frühere Einfügung ersetzen möchte, markiert sie vor dem Aufruf. So bleibt auch Text erhalten, den der Nutzer seit dem Diktat bearbeitet hat. „Original kopieren“ übernimmt den Rohtext in die Zwischenablage.

Ein Hintergrundauftrag berechnet den Wortvergleich und bereitet die Seiten für die native Anzeige vor. Jede Spalte zeigt bis zu 8192 UTF-16-Einheiten und 96 farbige Bereiche je Seite. Zusätzliche Seiten sind über „Zurück“ und „Weiter“ erreichbar. Original-Aktionen behalten den Bezug zum vollständigen Diktat. Wenn das Zielfenster vor dem Einfügen geschlossen wurde, bleibt die Kopiermöglichkeit im Vergleich erhalten.

Die Ansicht verarbeitet auch das Kopieren einer Auswahl über Strg+C, Strg+Einfg, den Kopierbefehl von Windows und das Kontextmenü selbst. Sie verwendet die drei Schutzformate aus der Einfügeroutine. Die schreibgeschützten Textfelder fangen außerdem das RichEdit-Diagnosekürzel ab, das auf entsprechend konfigurierten Systemen markierten Text in eine temporäre Datei schreiben kann. [RichEdit-Tastenkürzel](https://learn.microsoft.com/windows/win32/controls/about-rich-edit-controls#rich-edit-shortcut-keys)
