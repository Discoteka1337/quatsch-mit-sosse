# Lokale Anpassung von tauri-runtime-wry

Upstream: Tauri Programme within The Commons Conservancy, `tauri-runtime-wry` **2.12.4**, veröffentlicht unter Apache-2.0 oder MIT. Beide unveränderten Lizenztexte liegen in diesem Verzeichnis. Quelle: das bereits durch die Projekt-Lockdatei festgelegte [Registry-Paket](https://crates.io/crates/tauri-runtime-wry/2.12.4), übernommen aus dem lokalen Cargo-Cache.

SHA256 des ursprünglichen `.crate`-Archivs: `0d9be5e7af9560a4a60cdd441389a066cdd3e7c5d464a006ae34cb276cc78abd`. Der Hash des vorhandenen Cache-Archivs wurde vor dem Kopieren mit dem bisherigen `Cargo.lock` verglichen. `UPSTREAM-CHECKSUMS.json` enthält die Prüfsummen aller übernommenen Upstream-Dateien; für diese Textdateien werden UTF-8 ohne BOM und LF-Zeilenenden verwendet, damit Windows-Checkouts keine falschen Abweichungen erzeugen.

Es gibt genau drei Änderungen in `src/lib.rs`: Das Feld `webview_runtime_installed` und seine Initialisierung entfallen; die Versionsprüfung erfolgt stattdessen unmittelbar am Beginn von `create_webview`. Damit ruft der Start einer ausschließlich nativen Tauri-Oberfläche die WebView2-SDK-Funktion zur Versionsabfrage nicht mehr auf. Die Fehlerbehandlung bei einer tatsächlich angeforderten WebView bleibt bestehen. Eine solche Browsererstellung ist in der Quatsch-mit-Soße-Anwendung zusätzlich ausgeschlossen; diese Anpassung allein verbietet WebViews nicht.

`scripts/native-runtime.Tests.ps1` wurde zuerst gegen die unveränderte Kopie ausgeführt und scheiterte am vorzeitigen SDK-Aufruf. Der Test prüft den verzögerten Pfad und rekonstruiert für den Prüfsummenvergleich die ursprüngliche Quelle durch Rücknahme ausschließlich dieser drei Änderungen. Jede weitere Änderung an den kopierten Dateien erfordert einen neuen Review.

Dies ist kein allgemeiner Telemetrie-Schalter für Windows und keine Änderung globaler Systemeinstellungen. Bei einem Tauri-Update müssen Patch, Quellprüfung und nativer Prozess-/Fenstertest erneut geprüft werden; die Kopie darf nicht ungeprüft aktualisiert werden.
