# 0006: Native Fenster ohne WebView2

Datum: 17.11.2026 · Fehlerbehebung zu Entscheidung 0005

## Ursache und Entscheidung

WebView2 besitzt laut [Microsoft](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/data-privacy) erforderliche Diagnostik, die die einbettende Anwendung nicht vollständig kontrollieren kann. Der bisherige Crash-Upload-Schutz löst deshalb nicht das allgemeine Telemetrieproblem aus Entscheidung 0005.

Die Desktop-App erzeugt keine WebViews mehr. Einstellungen und Overlay werden als native Tauri-Fenster mit Windows-Textsteuerelementen aufgebaut. Tray, Ein-Instanz-Plugin, Fensterkonfiguration, Sprachkataloge und Bereitschafts-/Messprotokoll bleiben erhalten. Es werden keine globalen Windows-, Netzwerk-, Firewall- oder Sicherheitseinstellungen verändert.

Dies ersetzt die ursprüngliche Festlegung einer eingebetteten Svelte-Oberfläche im Masterplan. Das bestehende Svelte-/TypeScript-Gerüst und seine Prüfungen bleiben erhalten, werden aber nicht in die Desktop-App eingebettet. Spätere Oberflächen müssen dieser nativen Architektur folgen; ein erneuter Browser-Einsatz benötigt eine neue begründete Datenschutzentscheidung. Eine bloß dokumentierte Ausnahme vom Telemetrieverbot genügt nicht.

## Kleinste notwendige Laufzeitkorrektur

Native Tauri-Fenster allein reichen nicht: `tauri-runtime-wry` 2.12.4 fragt beim Initialisieren auch ohne WebView die installierte WebView2-Version über deren SDK ab. Ein öffentlicher Schalter dafür existiert in dieser Version nicht. Ein vollständiger Austausch von Tauri würde zusätzlich Tray, Ein-Instanz-Verhalten und Ereignisverwaltung ersetzen.

Deshalb wird genau diese bereits gesperrte Abhängigkeit unter `vendor/tauri-runtime-wry` mit drei Quelländerungen verwendet: Die Versionsprüfung erfolgt erst bei einer tatsächlichen WebView-Erstellung; das vorab ermittelte Kontextfeld entfällt. Die Anwendung benutzt diesen Browserpfad nicht. Herkunft, ursprünglicher Archiv-SHA256, Dateiprüfsummen und beide Originallizenzen liegen bei der Kopie. Die Versionsbindung und eine Prüfung sämtlicher übernommenen Dateien verhindern unbemerkte Abweichungen. Bei einem Tauri-Update ist diese kleine Anpassung erneut zu prüfen.

Tauris native `WindowBuilder`-API benötigt derzeit das Feature `unstable`; die App ist deshalb an die Tauri-Minorlinie 2.12 gebunden. Beide Fenster behalten `create=false`, damit keine automatischen WebViews entstehen. Auch der zukünftige Installer ist auf `webviewInstallMode: skip` eingestellt. Der bisherige eigene COM-/WebView2-Umgebungscode entfällt vollständig.

## Lebenszyklus und Nachweise

Die Texte stammen weiterhin aus den deutschen/englischen JSON-Katalogen. Native Textsteuerelemente gehören ihren Elternfenstern; die eigenen Schriftressourcen werden bei Fehlern und Schließen freigegeben. Erneutes Öffnen bleibt serialisiert, und native Steuerelemente werden nur auf dem Fensterthread eingerichtet. Das Schließen zerstört weiterhin das Einstellungsfenster.

Die Tests wurden vor der Korrektur ergänzt: Der unveränderte Release scheiterte am tatsächlich gestarteten WebView2-Kindprozess, die unveränderte Runtime am SDK-Aufruf während ihrer Initialisierung und der zentrale Prüfplan an den fehlenden neuen Pflichtprüfungen. Zusätzlich prüfen Rust-Tests native Fenster ohne WebView-Registrierung, Sprachtexte, Layout und DPI.

Der vollständige Prüflauf enthält jetzt `scripts/native-runtime.Tests.ps1` und `scripts/native-ui.Tests.ps1`. Die erste Prüfung kontrolliert den exakten Abhängigkeitspatch, Cargo-Auflösung, ausgeschaltete automatische WebViews und fehlende Browser-/SDK-Einstiege im Anwendungscode. Die zweite startet den gebauten Prozess, prüft echte native Texte, anfangs unsichtbare Fenster, Ein-Instanz-Verhalten, mehrfache Zerstörung/Neuerstellung, wiederholt die erfasste Prozessfamilie auf WebView2 und kontrolliert reguläres Beenden. Die Prozessprüfung pollt; sie ergänzt den Quellnachweis und kann sehr kurze Prozesse zwischen Abfragen allein nicht ausschließen. Es werden keine Screenshots erstellt.

Damit wird der konkret von dieser Anwendung verursachte WebView2-Diagnostikpfad entfernt. Dies ist keine Aussage über systemweite Windows-Diagnostik oder andere Anwendungen; diese werden weder verändert noch als durch die App kontrollierbar ausgegeben.

## Ergebnis

Der abschließende vollständige Prüflauf ist grün: Formatierung, Clippy ohne Warnungen, 28 Rust-Tests, Svelte-Check, vier UI-Tests, 60 Datenschutzfälle und sämtliche PowerShell-Regressionen einschließlich der neuen Prüfungen. Die native Fensterprüfung besteht mit Debug- und Release-Build jeweils einschließlich drei Schließen-/Neuerstellen-Zyklen. Der ursprüngliche Release mit WebView2 wurde vom selben Laufzeittest abgewiesen.

Die erneute Release-Messung mit 60 Sekunden Aufwärmen und 30 Sekunden Messfenster erfasst einen App-Prozess: 0,162 Sekunden bis bereit, maximal 2,84 MB Private Bytes und 18,49 MB Working Set sowie 0 % gemessene Leerlauf-CPU. Die CPU-Zähler sind vollständig, das Ende regulär; Rohdaten und Build-Hash stehen im [Tagesbericht](../benchmarks/idle-2026-11-17.json). Die sichtbare Tray-/Darstellungsprüfung bleibt beim Menschen; die Installer-Abnahme ist noch nicht vorgesehen.
