# 0002: App-Gerüst unter Windows

Datum: 17.11.2026 · Bauschritt 0.1

## Fenster und Übersetzungen

Einstellungen und Overlay werden zunächst versteckt erzeugt. Der normale Schließvorgang der Einstellungen wird nicht verhindert: Fenster und WebView werden zerstört. Das Tray erstellt bei Bedarf ein neues Einstellungsfenster aus derselben Konfiguration. Nur ein implizites Prozessende nach dem letzten Fenster wird verhindert; „Beenden“ beendet ausdrücklich den Prozess.

Das Wiedererstellen läuft in einem Hintergrund-Task mit einer Sperre gegen parallele Erzeugung. Die [Tauri-Dokumentation](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html#method.from_config) beschreibt für synchrone Windows-Eventhandler einen möglichen WebView2-Deadlock. Eine zweite Instanz wird durch das Single-Instance-Plugin beendet und öffnet die Einstellungen der vorhandenen Instanz. Argumente und Arbeitsverzeichnis werden nicht protokolliert.

Rust und Svelte verwenden dieselben einfachen Sprachdateien unter `app/ui/src/locales/`, standardmäßig Deutsch. Für die von Tauri vor dem Start benötigten Installer-Metadaten und Fenstertitel steht der Produktname zusätzlich in `tauri.conf.json`. `build.rs` prüft diese Werte gegen `de.json`; ein abweichender Name stoppt den Build. Ein Sprachumschalter ist in diesem Schritt nicht vorgesehen.

## Starten und Testen

Auf dem Entwicklungsrechner wählte ein direkter Cargo-Aufruf einen Linker ohne vollständige MSVC-Bibliothekspfade (`LNK1104: msvcrt.lib`). Start- und Prüfscripte laden daher dieselbe Visual-Studio-2022-Umgebung wie der bereits vorhandene Pfadtest. Rust-Artefakte bleiben unter `C:\qms-target`. Der kleine npm-CLI-Starter wechselt nach `app`, damit Tauri die Geschwisterordner `src-tauri` und `ui` zuverlässig findet.

Tauris Windows-Testprogramme benötigen ebenfalls das Common-Controls-v6-Manifest; sonst enden Tests bereits beim Laden mit `STATUS_ENTRYPOINT_NOT_FOUND`. Das App-Buildskript setzt dafür die MSVC-Linkerargumente. Hintergrund: [Tauri-Fehlerbericht 13419](https://github.com/tauri-apps/tauri/issues/13419).

Die Rust-Tests prüfen Fensterkonfiguration, native Texte und das Erstellen/Wiederverwenden fehlender Einstellungen. Der Tauri-Mock bildet native Sichtbarkeit und Zerstörungsereignisse nicht vollständig ab. Die tatsächliche Fensterlebensdauer und das Tray bleiben daher Teil der Windows-Abnahme; Mocktests behaupten keinen vollständigen Oberflächentest.

Die bestehende Regel `models/` ignorierte auch den neuen Quellordner `crates/models`. Eine eng begrenzte Ausnahme macht diese Crate versionierbar. Modellgewichte, private Testdaten und Zugangsdaten bleiben ignoriert; der vorhandene Prüfregeltest sichert das ab.

## Nachweis für Schritt 0.1

Der zentrale Lauf mit `-Stage 0.1` besteht einschließlich Formatierung, Clippy ohne Warnungen, 15 Rust-Tests, Svelte-Check, vier UI-Tests und den PowerShell-Regressionen. Der UI-Produktionsbuild und der Entwicklungsstart funktionieren im Projektpfad mit Leerzeichen und ß.

Eine native Windows-Stichprobe ohne Screenshots prüfte zwei anfangs unsichtbare App-Fenster und den Always-on-top-Status des Overlays. In zwei Durchläufen beendete sich jeweils die zweite Instanz, während die vorhandene Instanz die Einstellungen öffnete. Nach einer normalen Schließanforderung war das jeweilige native Fenster tatsächlich zerstört und ließ sich neu erstellen. Die sichtbare Tray-Bedienung bleibt manuell zu prüfen. Datenschutz-Wächter, Leistungsabnahme und vollständiger Installer-Test sind in diesem Bauschritt noch nicht vorgesehen.
