# 0019 – Wörterbuchkorrektur für Parakeet primeLine

Datum: 22.11.2026 · Bauschritt: 3.3

Das Standardmodell `parakeet-primeline-de-int8` ist ein deutscher NeMo-TDT-Transducer. Die App lädt den int8-Export als `nemo_transducer` über `sherpa-rs` 0.6.8 mit sherpa-onnx 1.12.9. [Modellentscheidung](0001-standardmodell.md) · [Modellkarte des Exports](https://huggingface.co/flozen1981/parakeet-primeline-onnx) · [Laufzeitkonfiguration](0009-spracherkennung-und-sprachaktivitaet.md)

Die kontextuelle Verstärkung in sherpa-onnx setzt `modified_beam_search` voraus. Für `nemo_transducer` wählt Version 1.12.9 den Erkenner `OfflineRecognizerTransducerNeMoImpl`. Dieser akzeptiert ausschließlich `greedy_search`. Jede andere Methode führt bereits beim Erzeugen des Decoders zum Abbruch. [Hotword-Dokumentation](https://k2-fsa.github.io/sherpa/onnx/hotwords/index.html) · [Auswahl des Erkenners in v1.12.9](https://github.com/k2-fsa/sherpa-onnx/blob/v1.12.9/sherpa-onnx/csrc/offline-recognizer-impl.cc#L73-L83) · [Decoder-Auswahl in v1.12.9](https://github.com/k2-fsa/sherpa-onnx/blob/v1.12.9/sherpa-onnx/csrc/offline-recognizer-transducer-nemo-impl.h#L36-L52)

Die App verwendet deshalb die lokalen Wörterbucheinträge in `textfx` nach der Spracherkennung. Ein Benchmark mit aktivierter Hotword-Verstärkung entfällt für diese Kombination aus Modell und Laufzeit. Bei einem Wechsel des Modells oder der sherpa-onnx-Version wird die Unterstützung am entsprechenden Decoder erneut geprüft.
