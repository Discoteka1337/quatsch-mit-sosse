# 0012 – Globale Hotkeys

20.11.2026 · Schritt 2.1

## Standardbelegung

| Aktion | Kürzel | Begründung |
| --- | --- | --- |
| Diktat starten und beenden | Strg+Umschalt+Leertaste | Der Masterplan gibt dieses Kürzel vor. Die Leertaste lässt sich beim Sprechen mit dem Daumen halten. |
| Letzten Text erneut einfügen | Strg+Win+F8 | Das Kürzel hat eine eigene Auslösetaste und ist im Katalog der bekannten Windows- und Office-Belegungen frei. |
| Änderungen anzeigen | Strg+Win+F9 | F9 liegt neben dem Kürzel zum erneuten Einfügen. Die Kombination ist im lokalen Konfliktkatalog frei und lässt AltGr verfügbar. |

Diktat und erneutes Einfügen lassen sich konfigurieren. Der lokale Katalog in `crates/hotkey/conflicts.toml` erfasst ausgewählte Standardbelegungen. Individuell vergebene oder von weiteren Programmen registrierte Kürzel können ebenfalls kollidieren. Beim Diktat-Standard weist die Konfliktprüfung auf zwei bestehende Funktionen hin. Word fügt ein geschütztes Leerzeichen ein, Excel markiert je nach Ausgangslage den aktuellen Bereich oder das Arbeitsblatt. [Word-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/word/keyboard-shortcuts-in-word) · [Excel-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/excel/keyboard-shortcuts-in-excel) · [Windows-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/windows/keyboard-shortcuts-in-windows)

Auf deutschen Tastaturen kann AltGr als Strg und rechte Alt-Taste erscheinen. Die rechte Alt-Taste bleibt für AltGr reserviert. Kürzel mit Alt verwenden die linke Alt-Taste. Die Standardbelegung lässt beispielsweise die Eingabe von @ mit AltGr+Q zu. Für ein konfiguriertes Strg+Alt-Kürzel erscheint eine Warnung. Reine Sondertastenkürzel wie Strg+Win erhalten ebenfalls eine Warnung, weil sie bereits beim Ansetzen eines anderen Kürzels auslösen können.

Das Aufzeichnen im Einstellungsfenster erkennt die rechte Alt-Taste auch dann, wenn Windows sie als allgemeine Alt-Taste meldet. Ein Kürzel mit AltGr erhält die Meldung für eine nicht unterstützte Belegung. Für ein Kürzel mit Strg und Alt lässt sich die linke Alt-Taste aufzeichnen.

Seit Schritt 3.5 zeigt „Allgemein“ die aktive Belegung für „Änderungen anzeigen“. Die App versucht der Reihe nach Strg+Win+F9, Strg+Win+F10, Strg+Win+F11, Umschalt+Alt+J und F13. Sie überspringt die selbst gewählten Diktat- und Einfügekürzel, auch wenn eine reine Sondertastenbelegung schon beim Ansetzen der Kombination auslösen würde. Windows muss das freie Kürzel mit `RegisterHotKey` bestätigen. Sind alle Kandidaten belegt, zeigt „Allgemein“ die fehlende Verfügbarkeit an. Diktat und erneutes Einfügen funktionieren weiter. Beim Aufzeichnen gibt die App die Belegung vorübergehend frei und ermittelt sie anschließend erneut.

## Ereignisse

Ein eigener Windows-Thread betreibt den globalen `WH_KEYBOARD_LL`-Hook. Er fängt die Auslösetaste eines erkannten Kürzels einschließlich Tastenwiederholung und Loslassen ab. Verbraucher erhalten ausschließlich Aktionsereignisse.

- Beim Halten meldet das vollständige Diktatkürzel `Start`. Sobald eine erforderliche Taste losgelassen wird, folgt `Stop`.
- Beim Umschalten meldet der erste Druck `Start`, der nächste Druck `Stop`. Das bloße Halten löst keinen weiteren Wechsel aus.
- Esc während einer Aufnahme meldet `Cancel`. Der Verbraucher verwirft das Diktat ohne Einfügen. Außerhalb der Aufnahme erreicht Esc die Zielanwendung.
- Das zweite Kürzel meldet `ReinsertLast`. Den letzten Text vorzuhalten und einzufügen ist Aufgabe der späteren Pipeline.
- Für den Vergleich meldet Windows `WM_HOTKEY`. Die Registrierung mit `MOD_NOREPEAT` verhindert Wiederholungen beim Halten. Der Hook merkt sich für die Auslösetaste das Vordergrundfenster. Bei einem physischen Tastendruck und unverändertem zulässigem Ziel erhält die App `ShowChanges`. Die reguläre Hotkey-Nachricht ermöglicht die Aktivierung des angeforderten Vergleichsfensters. [Windows-Hotkey-Registrierung](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerhotkey) · [Vordergrundaktivierung bei registrierten Kürzeln](https://devblogs.microsoft.com/oldnewthing/20090226-00/?p=19013)

Ein separater Vordergrund-Hook prüft beim Fokuswechsel die Integritätsstufe des Zielprozesses gegen die des eigenen Prozesses. Bei einer höheren Stufe meldet die Crate `ElevatedTarget` bereits beim Wechsel in das Fenster. Das ermöglicht einen Hinweis, selbst wenn Windows die Tastatureingaben für dieses Fenster abschirmt. Eine laufende Aufnahme wird bei diesem Wechsel abgebrochen. Schlägt die Abfrage fehl, wird ein laufendes Diktat ebenfalls verworfen. Neue Diktat- und Einfügeaktionen sind wieder möglich, sobald sich die Integritätsstufe erfolgreich prüfen lässt und das Ziel mit gleichen oder niedrigeren Rechten läuft.

## Manuelle Prüfung unter Windows

1. In einer für MSVC vorbereiteten PowerShell im Projektordner `cargo run -p quatsch-mit-sosse-hotkey --example hotkey_test` starten und einen normalen Editor fokussieren.
2. Strg+Umschalt+Leertaste halten und loslassen. Die Konsole soll einmal „Start“ und einmal „Stopp“ anzeigen. Im Editor darf kein Leerzeichen erscheinen.
3. Erneut halten, Esc drücken und dann das Kürzel loslassen. Erwartet wird ein Abbruch ohne anschließenden Stopp. Mit Strg+Win+F8 das Ereignis zum erneuten Einfügen prüfen.
4. Das Beispiel mit `cargo run -p quatsch-mit-sosse-hotkey --example hotkey_test -- umschalten` neu starten. Zwei einzelne Betätigungen des Diktatkürzels sollen „Start“ und „Stopp“ melden.
5. Bei normal gestartetem Beispiel zu einem als Administrator geöffneten Fenster wechseln. Der Hinweis auf die höhere Integritätsstufe soll schon beim Fokuswechsel erscheinen.
6. Im normalen Editor das vom Beispiel angezeigte Vergleichskürzel gedrückt halten. Die Konsole soll einmal „Änderungen anzeigen“ melden.
