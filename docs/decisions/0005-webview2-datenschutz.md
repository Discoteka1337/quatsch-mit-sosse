# 0005: Datenschutzgrenze der WebView2-Laufzeit

Datum: 17.11.2026 · Review Phase 0

**Status: durch [Entscheidung 0006](0006-native-fenster-ohne-webview2.md) ersetzt.** Die folgende Beschreibung dokumentiert den damaligen Befund und Zwischenstand; die Desktop-App verwendet inzwischen native Fenster ohne WebView2-Initialisierung.

## Hoher Fund und Abnahmegrenze

Der Masterplan legt Tauri 2 unter Windows fest und verbietet zugleich Telemetrie sowie externe Absturzberichte. Tauri verwendet hier WebView2. Die im Projekt aufgelösten Versionen Wry 0.57.2 und webview2-com 0.39.1 verwenden ohne Anpassung `IsCustomCrashReportingEnabled=false`. Damit hängen automatische Crash-Uploads von der Windows-Diagnoseeinwilligung ab; die App unterbindet sie nicht selbst.

Die [Microsoft-Dokumentation zur Environment-Option](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environmentoptions3) beschreibt `IsCustomCrashReportingEnabled=true` als unterstützten Weg, Crashdatenversand an Microsoft zu verhindern. Der Name bedeutet hier, dass die Anwendung selbst verantwortlich wäre; Quatsch mit Soße implementiert keinen eigenen Upload.

Darüber hinaus beschreibt Microsoft [erforderliche WebView2-Diagnostik unabhängig von der Windows-Einstellung und fehlende Kontrolle des Entwicklers über die gesamte Diagnostik](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/data-privacy). Dieser **hohe Architektur- und Abnahmefund bleibt offen**. Der lokale Datenschutz-Wächter prüft Projektquellen und Abhängigkeiten auf seine Verbotsmuster; sein Erfolg beweist keine vollständige Telemetriefreiheit der eingebetteten Laufzeit.

## Entscheidung im Review

Der unterstützte Crash-Upload-Schutz wird in der Anwendung gesetzt und durch Regressionstests abgesichert. Die verbleibende Diagnostikgrenze wird nicht als behoben oder als niedrige Aufgabe umetikettiert. Die Datenschutzregeln werden nicht abgeschwächt, und es erfolgt kein eigenmächtiger Austausch der im Masterplan festgelegten Oberfläche.

`app/src-tauri/src/webview_privacy.rs` erzeugt die gemeinsame WebView2-Umgebung mit der genannten COM-Option vor dem Fensterbau. Beide Fenster haben `create=false`, damit Tauri keine ungeschützte Standardumgebung vorab startet. Initiale Fenster und später neu erstellte Einstellungen verwenden dieselbe Umgebung; ein fehlender Umgebungszustand ist ein Fehler. Ihr Profil liegt unter `%LOCALAPPDATA%\QuatschMitSosse\data\webview2`.

Die COM-Initialisierung und eigene Zugriffe auf die Umgebung bleiben auf dem Hauptthread. Während der asynchronen Environment-Erstellung verarbeitet die Windows-Nachrichtenschleife weiterhin COM-Nachrichten. Die spätere Fenstererstellung bleibt im bisherigen Hintergrundpfad; der UI-Thread wartet nicht auf diesen Worker. Frühe Öffnungsanforderungen werden bis zur abgeschlossenen Initialisierung vorgemerkt, damit die Nachrichtenschleife keine doppelte Fenstererstellung auslöst. `QMS_READY` folgt weiterhin erst nach erfolgreichem Setup.

Die neuen Tests prüfen den tatsächlichen Wert über `ICoreWebView2EnvironmentOptions3`, den Ausschluss automatischer Standardfenster, das Verhalten bei fehlender geschützter Umgebung, den Datenpfad und frühe Öffnungsanforderungen. Die Regressionen für den alten Standardwert und die automatische Fenstererstellung waren vor der Korrektur rot. Diese Tests sichern die eigene Konfiguration ab; sie simulieren keinen Crash und behaupten keine vollständige Kontrolle der Windows-Diagnostik.

CSP und Filter für Webinhalte kontrollieren nicht nachweislich die gesamte Laufzeitdiagnostik. Auch ein einzelner unauffälliger Netzwerktest wäre dafür keine Garantie. Undokumentierte Browserflags, globale Firewall- oder Registry-Änderungen sind kein Ersatz für eine belastbare Lösung; Microsoft [rät vom Produktionseinsatz der Diagnoseflags ab](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags).

Vor einer uneingeschränkten Datenschutzabnahme ist deshalb eine Architekturentscheidung erforderlich: eine nachweislich passende Laufzeit oder eine überprüfbare Isolation sämtlicher von der App verursachter Diagnoseübertragungen. Bis dahin darf das Projekt keine vollständige Telemetriefreiheit des WebView2-Prozessbaums behaupten. Ein vollständiger grüner Entwicklungsprüflauf hebt diesen offenen Fund nicht auf.

## Prüfergebnis des Reviews

Der abschließende vollständige Lauf von `scripts/check.ps1` besteht: Formatierung, Clippy ohne Warnungen, 26 Rust-Tests, Svelte-Check ohne Fehler oder Warnungen, vier UI-Tests, 60 Datenschutzfälle einschließlich 54 abgewiesener Negativbeispiele sowie die Pfad-, Start-, CI- und Messskriptregressionen. Der Release-Build ohne Installer und die neue Leerlaufmessung bestehen ebenfalls; Zahlen und Build-Hash stehen im [Tagesbericht](../benchmarks/idle-2026-11-17.json).

Eine native Windows-Prüfung ohne Screenshots bestätigte zwei anfangs unsichtbare Fenster, die Behandlung einer nachweislich vor `QMS_READY` eingehenden zweiten Instanz und insgesamt vier erfolgreiche Zyklen aus Schließen, tatsächlicher Fensterzerstörung und erneutem Öffnen. Beide App-Läufe und ihre jeweils sechs erfassten WebView2-Kinder endeten regulär. Dies ist eine Lebenszyklusprüfung, kein Nachweis allgemeiner Telemetriefreiheit. Die sichtbare Tray-Bedienung bleibt vom Menschen zu prüfen; der Installer-Test ist erst später vorgesehen.
