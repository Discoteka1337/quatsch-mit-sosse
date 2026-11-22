# 0018 – Selbstkorrekturen, Zahlenschreibweise und Satzgrenzen

22.11.2026 · Schritt 3.2

Die drei Regeln lassen sich einzeln über `Pipeline::new` oder `set_enabled` einschalten. Für bestehende Aufrufer behält `Pipeline::default()` die Füllwortbereinigung aus Schritt 3.1 bei.

Bei einer Selbstkorrektur muss sich das gemeinte Satzglied aus der unmittelbaren Umgebung ergeben. In „Wir kommen am Montag, nein, ich meine Dienstag zum Essen“ ändert sich nur der Wochentag. In „Das scheint seltsam, äh, schwierig“ ist auch ein Zögern denkbar. Die Pipeline erhält den fraglichen Ausdruck samt Pausenkommas. `apply_with_review` gibt den Endtext und die unsicheren Stellen als halboffene UTF-8-Bytebereiche in diesem Endtext zurück. Damit kann die Änderungsansicht auch nach einer kürzeren Zahlenschreibweise oder einem vorher entfernten Füllwort die richtige Stelle hervorheben. `apply` liefert weiterhin den Text allein.

Ein Zögerer kann auch direkt vor einer ausdrücklich gesprochenen Korrektur stehen. „Wir buchen zwei, äh, nein, ich meine drei Plätze“ ergibt „Wir buchen drei Plätze“. Fehlende Pausenkommas ändern diese Entscheidung nicht.

Die Einstellung `digits_from` bezeichnet den ersten Wert, der bei gewöhnlichen Kardinalzahlen als Ziffer geschrieben wird. Die Vorgabe 13 ergibt „zwölf oder 13 Plätze“. Für Beträge, Prozentangaben, Uhrzeiten mit Minuten und Kalendertage gelten eigene Schreibweisen. „Halb drei“ bleibt auch bei niedriger Ziffernschwelle erhalten. Geschützte Wörterbucheinträge und vorhandene numerische Schreibweisen behalten ihre Originalform.

Die Zeichensetzung nutzt vorhandene Satzgrenzen und ergänzt den abschließenden Punkt bei offenem Wortende. Ein einzelner Zeilenwechsel kann mitten im Satz liegen. Bei „wir lesen z. B. gerne“ und „am 3. fahren wir“ begründen die Punkte innerhalb der Äußerung keine Großschreibung. Der Benchmark aus Schritt 1.5 ignoriert Großschreibung und Satzzeichen. Für solche Änderungen prüfen die Golden-Tests den genauen Ausgabetext.

## Nachbesserung für die WER-Abnahme

Ein vollständig numerisch diktiertes Datum mit zwei Ordnungszahlen und vierstelligem Jahr erhält die Form `TT.MM.JJJJ`. Das Jahr kann in der Erkennung auch über mehrere Wörter verteilt sein. Vor dem Ersetzen prüft die Zahlenregel die Länge des Monats einschließlich der Schaltjahrregel. Ein diktierter Monatsname behält die bereits festgelegte Schreibweise „3. März“.

Für die Software-Diktate des Masterplans stellt `crates/textfx` den Konstruktor `software_dictation_pipeline()` bereit. Er aktiviert auch die Wörterbuchstufe. Diese vereinheitlicht für Deutsch (Deutschland) eindeutige gebeugte Formen wie „grosse“ und „anschliessend“. Mehrdeutige Wörter wie „Masse“ und geschützte Namen behalten ihre Schreibung.

Die Wortgrenzen der festgelegten Fachbegriffe lassen sich bereits vor dem datenbankgestützten Wörterbuch aus Schritt 3.3 prüfen. Das Profil enthält veröffentlichte Begriffe des Projekts, darunter „Affiliate-Link“ und „Pull Request“. Für eine Korrektur müssen die Buchstaben und ihre Reihenfolge übereinstimmen. Andere Aufrufer können ihre eigene Begriffsliste über `proper_names` übergeben.

Der Benchmark verwendet dieses Profil unverändert für den gesamten Korpus und beide Modelle. Der Endtext entsteht ausschließlich aus der ASR-Ausgabe. Erst danach bewertet der Benchmark Rohtext und Endtext gegen dieselbe wörtliche Referenz mit der WER aus Schritt 1.5. `--check-cleanup` wertet die ungerundeten Fehlerzahlen je Modell aus.
