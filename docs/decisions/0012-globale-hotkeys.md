# 0012 – Globale Hotkeys für Diktat und erneutes Einfügen

20.11.2026 · Schritt 2.1

## Standardbelegung

| Aktion | Kürzel | Begründung |
| --- | --- | --- |
| Diktat starten und beenden | Strg+Umschalt+Leertaste | Der Masterplan gibt dieses Kürzel vor. Die Leertaste lässt sich beim Sprechen mit dem Daumen halten. |
| Letzten Text erneut einfügen | Strg+Win+F8 | Das Kürzel hat eine eigene Auslösetaste und ist im Katalog der bekannten Windows- und Office-Belegungen frei. |

Beide Kürzel lassen sich konfigurieren. Der lokale Katalog in `crates/hotkey/conflicts.toml` erfasst ausgewählte Standardbelegungen. Individuell vergebene oder von weiteren Programmen registrierte Kürzel können ebenfalls kollidieren. Beim Diktat-Standard weist die Konfliktprüfung auf zwei bestehende Funktionen hin. Word fügt ein geschütztes Leerzeichen ein, Excel markiert je nach Ausgangslage den aktuellen Bereich oder das Arbeitsblatt. [Word-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/word/keyboard-shortcuts-in-word) · [Excel-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/excel/keyboard-shortcuts-in-excel) · [Windows-Tastenkombinationen](https://support.microsoft.com/de-de/accessibility/windows/keyboard-shortcuts-in-windows)

Auf deutschen Tastaturen kann AltGr als Strg und rechte Alt-Taste erscheinen. Die rechte Alt-Taste bleibt für AltGr reserviert. Kürzel mit Alt verwenden die linke Alt-Taste. Beide Standards lassen damit beispielsweise die Eingabe von @ mit AltGr+Q zu. Für ein konfiguriertes Strg+Alt-Kürzel erscheint eine Warnung. Reine Sondertastenkürzel wie Strg+Win erhalten ebenfalls eine Warnung, weil sie bereits beim Ansetzen eines anderen Kürzels auslösen können.

## Ereignisse

Ein eigener Windows-Thread betreibt den globalen `WH_KEYBOARD_LL`-Hook. Er fängt die Auslösetaste eines erkannten Kürzels einschließlich Tastenwiederholung und Loslassen ab. Verbraucher erhalten ausschließlich Aktionsereignisse.

- Beim Halten meldet das vollständige Diktatkürzel `Start`. Sobald eine erforderliche Taste losgelassen wird, folgt `Stop`.
- Beim Umschalten meldet der erste Druck `Start`, der nächste Druck `Stop`. Das bloße Halten löst keinen weiteren Wechsel aus.
- Esc während einer Aufnahme meldet `Cancel`. Der Verbraucher verwirft das Diktat ohne Einfügen. Außerhalb der Aufnahme erreicht Esc die Zielanwendung.
- Das zweite Kürzel meldet `ReinsertLast`. Den letzten Text vorzuhalten und einzufügen ist Aufgabe der späteren Pipeline.

Ein separater Vordergrund-Hook prüft beim Fokuswechsel die Integritätsstufe des Zielprozesses gegen die des eigenen Prozesses. Bei einer höheren Stufe meldet die Crate `ElevatedTarget` bereits beim Wechsel in das Fenster. Das ermöglicht einen Hinweis, selbst wenn Windows die Tastatureingaben für dieses Fenster abschirmt. Eine laufende Aufnahme wird bei diesem Wechsel abgebrochen. Schlägt die Abfrage fehl, wird ein laufendes Diktat ebenfalls verworfen. Neue Diktat- und Einfügeaktionen sind wieder möglich, sobald sich die Integritätsstufe erfolgreich prüfen lässt und das Ziel mit gleichen oder niedrigeren Rechten läuft.

## Manuelle Prüfung unter Windows

1. In einer für MSVC vorbereiteten PowerShell im Projektordner `cargo run -p quatsch-mit-sosse-hotkey --example hotkey_test` starten und einen normalen Editor fokussieren.
2. Strg+Umschalt+Leertaste halten und loslassen. Die Konsole soll einmal „Start“ und einmal „Stopp“ anzeigen. Im Editor darf kein Leerzeichen erscheinen.
3. Erneut halten, Esc drücken und dann das Kürzel loslassen. Erwartet wird ein Abbruch ohne anschließenden Stopp. Mit Strg+Win+F8 das Ereignis zum erneuten Einfügen prüfen.
4. Das Beispiel mit `cargo run -p quatsch-mit-sosse-hotkey --example hotkey_test -- umschalten` neu starten. Zwei einzelne Betätigungen des Diktatkürzels sollen „Start“ und „Stopp“ melden.
5. Bei normal gestartetem Beispiel zu einem als Administrator geöffneten Fenster wechseln. Der Hinweis auf die höhere Integritätsstufe soll schon beim Fokuswechsel erscheinen.
