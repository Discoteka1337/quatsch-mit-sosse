# 0016 – Grundeinstellungen, Anmeldestart und Lautstärke

20.11.2026 · Schritt 2.6

## Einstellungsdatei und Migration

Die Seite „Allgemein“ verwendet native Windows-Steuerelemente. Die App speichert die Auswahl als UTF-8-JSON unter `%LOCALAPPDATA%\QuatschMitSosse\data\settings.json`. Die erste Fassung hatte die Schemaversion 1. Version 2 ergänzt den Abschnitt `cleanup` für die Bereinigung und übernimmt bestehende Einstellungen bei der Migration. Beim ersten Start übernimmt die App die Anzeigeoptionen aus der bisherigen `overlay.json`. Dort bleiben die monitorbezogenen Positionen erhalten. Die Prozessregeln für das Einfügen liegen weiterhin in `inject.toml`.

Unversionierte Einstellungen erhalten fehlende Standardwerte. Vor dem Austausch prüft die App Kürzel, Modell und Wertebereich, schreibt eine temporäre Datei im selben Ordner und synchronisiert deren Inhalt. Eine beschädigte Datei oder eine jüngere, unbekannte Version führt zu einem Fehler und bleibt für die Wiederherstellung erhalten.

Die Oberfläche zeigt Änderungen sofort. Ein Hintergrundthread speichert sie in Eingabereihenfolge. Scheitert eine Änderung, zeigt das Fenster den gültigen Wert und den Fehler an. Ist das Fenster inzwischen geschlossen, erscheint die Meldung im Overlay. Beim Beenden wird das Mikrofon sofort freigegeben. Vor dem Prozessende werden die bereits angenommenen Schreibaufträge abgearbeitet.

## Autostart für das aktuelle Konto

Der Schalter ist anfangs aus. Einschalten registriert den absoluten, in Anführungszeichen gesetzten Programmpfad mit `--autostart` als Wert `quatsch-mit-sosse` im Schlüssel `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`. Ausschalten löscht diesen Wert. Bei aktivem Autostart erneuert die App den Pfad beim Start. Scheitert beim Speichern der Austausch der Einstellungsdatei, setzt sie den Registrierungseintrag auf den vorherigen Zustand zurück.

Nach einem unterbrochenen Speichervorgang gleicht auch ein manueller Start den Registrierungseintrag mit der gespeicherten Auswahl ab. Verweigert Windows den Zugriff, zeigt das Overlay einen Hinweis. Die übrigen Einstellungen und das Diktat bleiben verfügbar.

## Verhalten im Windows-Lautstärkemixer

Der Prozentwert ist eine Obergrenze pro Audiositzung. Bei 20 % sinkt ein Mixerwert von 65 % auf 20 %, ein Wert von 10 % bleibt bei 10 %. Mit 100 % überspringt die App die Absenkung. Die Gesamtlautstärke, die Windows-Systemtöne und die eigene Wiedergabe behalten ihre Pegel. Die beiden Signaltöne entstehen aus je 90 Millisekunden Sinusschwingungen. Die Tonerzeugung gehört zum Projekt und ist unter seiner Apache-2.0-Lizenz gebührenfrei nutzbar.

Vor dem Absenken speichert die App SHA256-Prüfsummen der Sitzungs- und Gerätekennungen sowie die betroffenen Mixerwerte in `volume-recovery.json` im lokalen Datenordner. Nach der Aufnahme und beim nächsten App-Start gleicht sie die Werte mit dem Mixer ab. Eine inzwischen manuell geänderte Lautstärke hat Vorrang. Fehlt eine Sitzung oder ist das Ausgabegerät getrennt, behält die App den Eintrag und prüft erneut. Sobald die gespeicherten Einträge abgearbeitet sind, entfernt sie die Wiederherstellungsdatei.

Nach dem Start und beim Aufnahmestopp versucht die App die Wiederherstellung sofort. Bleiben Einträge offen, folgt der erste Leerlaufversuch nach fünf Sekunden. Anschließend verdoppelt sich der Abstand bis auf höchstens fünf Minuten. Eingaben bleiben während der Wartezeit möglich. Nach einer weiteren Aufnahme beginnt die Frist wieder bei fünf Sekunden. Sobald alle Werte wiederhergestellt sind, endet die regelmäßige Suche.
