# 0010 – Externe ONNX-Gewichte des deutschen Parakeet-Modells

Datum: 19.11.2026 · Fehlerbehebung zu Schritt 1.3

Der erste Test mit installiertem `parakeet-primeline-de-int8` brach mit einer fremden nativen Exception ab. Silero funktionierte; die SHA256-Prüfsummen der Encoder-Dateien waren korrekt. sherpa-onnx 1.12.9 lädt den Encoder aus einem Speicherpuffer. ONNX Runtime kann dabei die externe Datei `encoder.int8.onnx.data` nicht relativ zum Modellpfad auflösen. Die Gewichtsreferenzen liegen sowohl in Graph-Initialisierern als auch in Tensor-Attributen von Constant-Knoten.

Vor dem Laden dieses Modells erstellt STT deshalb eine temporäre ONNX-Datei mit eingebetteten Gewichten. Ein enger Protobuf-Adapter erhält unbekannte Felder unverändert, besucht Graphen, Knoten, Attribute und Tensoren und ersetzt externe Verweise durch `raw_data`. Er akzeptiert ausschließlich den erwarteten Dateinamen und prüft Offset, Länge, Grenzen und Rekursionstiefe. Die Originalinstallation bleibt unverändert und ihre Prüfmarker bleiben gültig.

Die temporäre Datei liegt im installierten Modellordner. Ihr Schreibhandle wird vor dem nativen Laden geschlossen, da ONNX Runtime unter Windows sonst den Zugriff verweigert. Nach dem synchronen Modellladen wird sie über RAII gelöscht, auch bei Rust-Fehlern. Ein harter Prozessabbruch kann eine temporäre Datei hinterlassen. Pro Kaltstart fallen ungefähr 652 MB zusätzliche temporäre Dateiausgabe und vorübergehend zusätzliche Speicherbelegung an; während der Wiederverwendung des geladenen Modells entfällt die Umwandlung. Es werden weder Modelle heruntergeladen noch Audio oder Transkripte protokolliert.

Tests: synthetische externe Initialisierer und Constant-Tensoren, Erhalt anderer Felder, ungültige Protobuf-Daten, unerlaubte Dateinamen, Bereichsüberläufe, lesbare temporäre Datei und deren Löschung. Der zuvor abstürzende Integrationstest wird mit dem installierten deutschen Modell und der privaten Aufnahme erneut ausgeführt; zusätzlich werden das Basismodell und Silero geprüft. Die Integrationsprüfungen geben keinen erkannten Text aus.

Grundlage: gebündelter Quellcode von sherpa-rs-sys 0.6.8, `offline-transducer-nemo-model.cc`, sowie das [offizielle ONNX-Protobuf-Schema](https://github.com/onnx/onnx/blob/main/onnx/onnx.proto).
