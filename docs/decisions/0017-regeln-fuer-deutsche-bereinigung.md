# 0017 – Deutsche Füllwörter und Sprachbefehle

21.11.2026 · Schritt 3.1

Die Pipeline in `crates/textfx` aktiviert standardmäßig die fünf eindeutigen Füllwörter. „Also“, „halt“ und „quasi“ können die Aussage tragen. Wer auch diese Wörter entfernen möchte, wählt bei eingeschalteter Füllwortregel die Stärke `Strong`. Die Reihenfolge der Regeln ist unabhängig davon, in welcher Reihenfolge der Aufrufer sie einschaltet.

Sprachbefehle erfordern eine ausdrückliche Aktivierung. „Punkt“ wird am Ende einer Äußerung oder eines Satzes als Befehl verstanden. In „Der Punkt ist erledigt Punkt“ wird daher nur das letzte Wort zum Satzzeichen. Ein vom Erkenner bereits gesetztes Satzschlusszeichen kann den Befehl abschließen. So endet „Hast du Zeit Fragezeichen.“ nach der Bereinigung mit genau einem Fragezeichen. Die Befehle „neue Zeile“ und „neuer Absatz“ erzeugen einen beziehungsweise zwei Zeilenvorschübe.

Ein Satzende kann auch innerhalb direkter Rede liegen. Aus `„Komm mit Ausrufezeichen!“, rief sie.` wird `„Komm mit!“, rief sie.` Das Komma vor dem Begleitsatz bleibt erhalten.

Die Füllwortregel bereinigt Abstände und Pausenkommas direkt am Einschub. Ein ausdrücklich diktiertes Komma bleibt erhalten, etwa in „Liebe Mia ähm Komma“. Weitere Entscheidungen beruhen auf dem unmittelbaren Satzbau. So bleibt ein vorhandenes Komma vor „dass“ oder „weil“, zwischen Zahlen einer Aufzählung und an erkennbaren Nebensatzgrenzen wie in „Wenn es regnet, ähm, bleiben wir daheim“ erhalten.

Der Aufrufer übergibt die schützenswerten Wörterbucheinträge an die Pipeline. Vor dem ersten Regelschritt markiert sie Namen und numerische Schreibweisen im Originaltext. Großschreibung und Unicode-Normalisierung dienen nur dem Vergleich. Auch wenn ein Name einen Befehlsausdruck, ein Füllwort oder mehrere Leerzeichen enthält, bleibt seine Schreibweise erhalten. Der Schutz gilt bei jeder Auswahl der acht Regeln.

Ein Komma am Anfang eines Zahlenausdrucks gehört zur Zahl, wenn direkt danach eine Ziffer folgt. So ergibt „ähm, ,5“ den Wert „,5“. In „ähm, 5“ gehört das Komma zur Sprechpause und das Ergebnis lautet „5“.
