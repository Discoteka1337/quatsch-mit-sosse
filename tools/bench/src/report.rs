use crate::measurement::ModelResult;

pub struct ReportContext {
    pub date: String,
    pub threads: u32,
    pub logical_processors: usize,
    pub processor: String,
    pub build: String,
    pub revision: String,
    pub uninstalled_models: Vec<String>,
}

pub fn render_report(context: &ReportContext, results: &[ModelResult]) -> String {
    let mut report = format!(
        "# ASR-Benchmark vom {}\n\n\
         - Rechner: {}; {} logische Prozessoren.\n\
         - Betrieb: Windows, CPU; ASR-Threads: {}; Silero-VAD: ein Thread.\n\
         - Build: {}; Revision: {}.\n\
         - Nicht installierte ASR-Modelle: {}.\n\n\
         ## Messverfahren\n\n\
         Alle vollständigen NNN.wav/NNN.txt-Paare werden lokal ausgewertet. \
         Der Bericht enthält ausschließlich Nummern und Messwerte, keine Referenz- oder Erkennungstexte. \
         Jedes Modell läuft nacheinander in einem eigenen Prozess. \
         Der Kaltstart ist die erste Erkennung einer Aufnahme mit nicht leerer Referenz, \
         einschließlich Laden von VAD und ASR, soweit Sprache erkannt wird. \
         Falls diese Ausgabe leer ist, werden weitere Sprachaufnahmen zum Aufwärmen versucht. \
         Erst eine nicht leere Ausgabe bestätigt das Laden der ASR-Engine. \
         Danach wird jedes vollständige Paar einmal gemessen; ohne bestätigtes Aufwärmen gelten \
         die Werte nicht als warme Messung und das Modell erhält keine Zeitfreigabe. \
         Die ASR-Modelle verwenden sherpa-onnx über den bestehenden SpeechEngine-Trait; \
         Sprachhinweis: de, Parakeet mit greedy_search.\n\n\
         Ende→Text ist die Wandzeit des blockierenden transcribe-Aufrufs auf bereits eingelesenem Audio, \
         einschließlich VAD, Aufteilung und Erkennung. Dateilesen und Fehlerberechnung sind ausgeschlossen. \
         Einfügen und spätere UI-Verarbeitung sind nicht Bestandteil der Messung. \
         RTF = Summe der Verarbeitungszeiten / Summe der Audiodauern; \
         P95 ist das empirische Quantil nach der Nächster-Rang-Methode.\n\n\
         Die Wortfehlerrate (WER) ist (S + D + I) / Referenzwörter, über alle Sprachaufnahmen gepoolt. \
         S = Ersetzungen, D = Auslassungen, I = Einfügungen. Unicode-NFC und Kleinschreibung werden angewandt; \
         Unicode-Satzzeichen werden entfernt, Umlaute und ß bleiben erhalten, es gibt kein Casefolding. \
         Symbole wie € und + bleiben erhalten. Leere Referenzen kennzeichnen Stille; \
         jede nicht leere Erkennung dort zählt als ein Phantomtext, unabhängig von ihrer Wortzahl. \
         Stille geht nicht in den WER-Nenner ein.\n\n\
         Für die Zeitgrenze wird aus jeder Sprachaufnahme mit mindestens zehn Sekunden \
         ein mittiger Ausschnitt von exakt 160.000 Samples (16 kHz, zehn Sekunden) \
         dreimal warm erkannt. Maßgeblich ist der höchste Median einer solchen Probe, \
         strikt unter 1,5 Sekunden. Es müssen Proben und eine definierte WER vorhanden sein; \
         mindestens eine Probe muss dreimal nicht leeren Text liefern, damit tatsächliche ASR-Verarbeitung belegt ist. \
         Leere Ausgaben bleiben sichtbar und gehen weiterhin in sämtliche Zeiten und die WER der vollständigen Aufnahme ein; \
         sie sind kein zusätzliches Ausschlusskriterium für ein ansonsten messbares Modell. \
         Kürzere Aufnahmen bleiben in WER und RTF enthalten. \
         Unter den geeigneten Modellen entscheidet die niedrigste WER, bei Gleichstand die Modell-ID.\n\n\
         Spitzen-RAM ist der vom Betriebssystem erfasste höchste Working Set des gesamten Modellprozesses \
         einschließlich Korpus, Laufzeit, VAD, ASR und Kaltstart (MiB = 1.048.576 Byte). \
         Die temporäre Inline-Encoder-Erzeugung bei primeLine gehört zur Ladespitze. \
         Die Phantomzahl misst die aktuelle Pipeline einschließlich VAD, kein Rohmodelltest: \
         Ohne VAD-Sprachsegmente wird der ASR-Aufruf bereits übersprungen. \
         Eine warme Zeitfreigabe gilt nicht automatisch für den Kaltstart.\n\n",
        escape(&context.date),
        escape(&context.processor),
        context.logical_processors,
        context.threads,
        escape(&context.build),
        escape(&context.revision),
        if context.uninstalled_models.is_empty() {
            "keine".into()
        } else {
            context
                .uninstalled_models
                .iter()
                .map(|id| escape(id))
                .collect::<Vec<_>>()
                .join(", ")
        },
    );
    if results.is_empty() {
        report.push_str("Keine installierten ASR-Modelle gemessen.\n");
        return report;
    }
    report.push_str("## Vergleich\n\n| Modell | WER | 10-s-Grenze | Höchster Probe-Median (s) | Spitzen-RAM | Phantomtexte |\n| --- | ---: | --- | ---: | ---: | ---: |\n");
    for result in results {
        let summary = Summary::new(result);
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} MiB | {} / {} |\n",
            escape(&result.model_id),
            percent(summary.wer()),
            if qualifies(result) {
                "erfüllt"
            } else {
                "nicht erfüllt"
            },
            decimal(worst_probe_median(result), 3),
            decimal(Some(result.peak_ram_bytes as f64 / 1_048_576.0), 2),
            summary.phantoms,
            summary.silence_count,
        ));
    }
    let (standard, fallback) = select_models(results);
    report.push('\n');
    if let Some(standard) = standard {
        report.push_str(&format!(
            "Empfohlenes Standardmodell: `{}`.\n\n",
            escape(&standard.model_id)
        ));
    } else {
        report.push_str("Kein Modell erfüllt das Auswahlkriterium; eine Standardfreigabe ist nicht begründet.\n\n");
    }
    if let Some(fallback) = fallback {
        report.push_str(&format!(
            "Rückfallmodell: `{}` ({}).\n\n",
            escape(&fallback.model_id),
            if qualifies(fallback) {
                "Auswahlkriterium erfüllt"
            } else {
                "Auswahlkriterium nicht erfüllt"
            },
        ));
    } else {
        report.push_str("Kein zusätzliches Rückfallmodell gemessen.\n\n");
    }
    for result in results {
        render_model(&mut report, result);
    }
    render_protection_checks(&mut report, results);
    report.truncate(report.trim_end_matches('\n').len());
    report.push('\n');
    report
}

const SENTENCES: usize = 20;

#[derive(Default)]
pub struct ProtectionChecks {
    silences: usize,
    sentences: usize,
    phantoms: Vec<u16>,
    wrong_first_words: Vec<u16>,
}

impl ProtectionChecks {
    fn silence_passed(&self) -> Option<bool> {
        (self.silences > 0).then_some(self.phantoms.is_empty())
    }

    fn first_words_passed(&self) -> Option<bool> {
        (self.sentences == SENTENCES).then_some(self.wrong_first_words.is_empty())
    }

    pub fn passed(&self) -> bool {
        self.silence_passed() == Some(true) && self.first_words_passed() == Some(true)
    }
}

pub fn protection_checks(model: &ModelResult) -> ProtectionChecks {
    let mut ordered: Vec<_> = model.samples.iter().collect();
    ordered.sort_unstable_by_key(|sample| sample.id);
    let mut checks = ProtectionChecks::default();
    for sample in ordered {
        if sample.silence {
            checks.silences += 1;
            if sample.phantom {
                checks.phantoms.push(sample.id);
            }
        } else if checks.sentences < SENTENCES {
            checks.sentences += 1;
            if !sample.first_word_matches {
                checks.wrong_first_words.push(sample.id);
            }
        }
    }
    checks
}

fn render_protection_checks(report: &mut String, results: &[ModelResult]) {
    report.push_str(&format!(
        "## Schritt 1.6 · Stille und Satzanfänge\n\n\
         Geprüft werden alle Aufnahmen mit leerer Referenz. Jede Ausgabe mit mindestens einem Zeichen zählt als Phantomtext. \
         Für die Satzanfänge zählen die ersten {SENTENCES} vollständigen Aufnahmen mit Sprachreferenz in aufsteigender Aufnahmenummer. \
         Das erste Wort der Ausgabe muss nach derselben Normalisierung wie bei der WER mit dem ersten Referenzwort übereinstimmen. \
         Eine leere Ausgabe oder ein abweichendes erstes Wort zählt als Fehler. \
         Das Stilleziel verlangt mindestens eine Stille-Aufnahme und null Phantomtexte. Das Ziel für die Satzanfänge liegt bei {SENTENCES} von {SENTENCES}.\n\n\
         | Modell | Phantomtexte / Stille-Aufnahmen | Fehler-Nr. Stille | Stilleziel | Erstes Wort erhalten / geprüfte Sätze | Fehler-Nr. Satzanfang | Ziel für Satzanfänge |\n\
         | --- | ---: | --- | --- | ---: | --- | --- |\n"
    ));
    for model in results {
        let checks = protection_checks(model);
        report.push_str(&format!(
            "| {} | {} / {} | {} | {} | {} / {} | {} | {} |\n",
            escape(&model.model_id),
            checks.phantoms.len(),
            checks.silences,
            format_ids(&checks.phantoms),
            format_verdict(checks.silence_passed()),
            checks.sentences - checks.wrong_first_words.len(),
            checks.sentences,
            format_ids(&checks.wrong_first_words),
            format_verdict(checks.first_words_passed()),
        ));
    }
}

fn format_verdict(passed: Option<bool>) -> &'static str {
    match passed {
        Some(true) => "bestanden",
        Some(false) => "nicht bestanden",
        None => "unvollständig",
    }
}

fn format_ids(ids: &[u16]) -> String {
    if ids.is_empty() {
        return "—".into();
    }
    ids.iter()
        .map(|id| format!("{id:03}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn select_models(results: &[ModelResult]) -> (Option<&ModelResult>, Option<&ModelResult>) {
    let mut ranked: Vec<_> = results.iter().collect();
    ranked.sort_by(|left, right| {
        Summary::new(left)
            .wer()
            .unwrap_or(f64::INFINITY)
            .total_cmp(&Summary::new(right).wer().unwrap_or(f64::INFINITY))
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    let standard = ranked.iter().copied().find(|result| qualifies(result));
    let remaining: Vec<_> = ranked
        .into_iter()
        .filter(|result| !standard.is_some_and(|selected| std::ptr::eq(*result, selected)))
        .collect();
    let fallback = remaining
        .iter()
        .copied()
        .find(|result| qualifies(result))
        .or_else(|| remaining.first().copied());
    (standard, fallback)
}

#[derive(Default)]
struct Summary {
    reference_words: usize,
    substitutions: usize,
    deletions: usize,
    insertions: usize,
    speech_count: usize,
    silence_count: usize,
    phantoms: usize,
    speech_audio_seconds: f64,
    speech_latency_seconds: f64,
    all_audio_seconds: f64,
    all_latency_seconds: f64,
}

impl Summary {
    fn new(result: &ModelResult) -> Self {
        let mut summary = Self::default();
        for sample in &result.samples {
            summary.all_audio_seconds += sample.audio_seconds;
            summary.all_latency_seconds += sample.latency_seconds;
            if sample.silence {
                summary.silence_count += 1;
                summary.phantoms += usize::from(sample.phantom);
            } else {
                summary.speech_count += 1;
                summary.reference_words += sample.reference_words;
                summary.substitutions += sample.substitutions;
                summary.deletions += sample.deletions;
                summary.insertions += sample.insertions;
                summary.speech_audio_seconds += sample.audio_seconds;
                summary.speech_latency_seconds += sample.latency_seconds;
            }
        }
        summary
    }

    fn wer(&self) -> Option<f64> {
        (self.reference_words > 0).then(|| {
            (self.substitutions + self.deletions + self.insertions) as f64
                / self.reference_words as f64
        })
    }
}

fn qualifies(result: &ModelResult) -> bool {
    result.warmup_succeeded
        && Summary::new(result).wer().is_some()
        && result.probes.iter().any(|probe| probe.empty_outputs == 0)
        && worst_probe_median(result).is_some_and(|seconds| seconds < 1.5)
}

fn median(mut values: [f64; 3]) -> Option<f64> {
    if values
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[1])
}

fn worst_probe_median(result: &ModelResult) -> Option<f64> {
    result
        .probes
        .iter()
        .map(|probe| median(probe.seconds))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max_by(f64::total_cmp)
}

fn ratio(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator > 0.0 && denominator.is_finite() && numerator >= 0.0 && numerator.is_finite())
        .then(|| numerator / denominator)
}

fn decimal(value: Option<f64>, precision: usize) -> String {
    match value.filter(|value| value.is_finite()) {
        Some(value) => format!("{value:.precision$}").replace('.', ","),
        None => "—".into(),
    }
}

fn percent(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{} %", decimal(Some(value * 100.0), 2)),
        None => "—".into(),
    }
}

fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '\\' | '|' | '`' | '*' | '_' | '[' | ']' | '<' | '>' => vec!['\\', character],
            character if character.is_control() => vec![' '],
            character => vec![character],
        })
        .collect()
}

fn render_model(report: &mut String, result: &ModelResult) {
    let summary = Summary::new(result);
    let mut latencies: Vec<_> = result
        .samples
        .iter()
        .map(|sample| sample.latency_seconds)
        .collect();
    latencies.sort_by(f64::total_cmp);
    let p95_index = (latencies.len() * 95).div_ceil(100).checked_sub(1);
    let p95 = p95_index.and_then(|index| latencies.get(index)).copied();
    report.push_str(&format!(
        "## {}\n\n\
         - Korpus: {} Sprachaufnahmen, {} Stille-Aufnahmen; {} s Audio insgesamt.\n\
         - Kaltstart (Aufnahme {:03}): {} s.\n\
         - Spitzen-RAM des Modellprozesses: {} MiB.\n\
         - Gepoolte WER: {}; {} Referenzwörter, S = {}, D = {}, I = {}.\n\
         - Ende→Text, {}: Mittelwert: {} s; P95: {} s; Maximum: {} s.\n\
         - Gewichteter RTF, Sprachaufnahmen: {}; alle Aufnahmen: {}.\n\
         - Phantomtexte: {} von {} Stille-Aufnahmen.\n\n",
        escape(&result.model_id),
        summary.speech_count,
        summary.silence_count,
        decimal(Some(summary.all_audio_seconds), 3),
        result.cold_id,
        decimal(Some(result.cold_seconds), 3),
        decimal(Some(result.peak_ram_bytes as f64 / 1_048_576.0), 2),
        percent(summary.wer()),
        summary.reference_words,
        summary.substitutions,
        summary.deletions,
        summary.insertions,
        if result.warmup_succeeded {
            "warme vollständige Aufnahmen"
        } else {
            "vollständige Aufnahmen (Aufwärmen nicht bestätigt)"
        },
        decimal(
            ratio(summary.all_latency_seconds, latencies.len() as f64),
            3
        ),
        decimal(p95, 3),
        decimal(latencies.last().copied(), 3),
        decimal(
            ratio(summary.speech_latency_seconds, summary.speech_audio_seconds),
            3
        ),
        decimal(
            ratio(summary.all_latency_seconds, summary.all_audio_seconds),
            3
        ),
        summary.phantoms,
        summary.silence_count,
    ));
    // Keep a failed warmup visible even when a model returns valid empty transcripts.
    if !result.warmup_succeeded || result.warmup_attempts > 1 {
        report.push_str(&format!("\nAufwärmen: {} Versuche; nicht leere Ausgabe bestätigt: {}. Ohne Bestätigung sind die obigen warmen Zeitwerte nicht nachgewiesen.\n\n", result.warmup_attempts, if result.warmup_succeeded { "ja" } else { "nein" }));
    }
    report.push_str("### Vollständige Aufnahmen\n\n\
         | Nr. | Art | Audio (s) | Ende→Text (s) | RTF | Ref.-Wörter | S | D | I | WER | Phantom |\n\
         | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for sample in &result.samples {
        let wer = (!sample.silence)
            .then(|| {
                ratio(
                    (sample.substitutions + sample.deletions + sample.insertions) as f64,
                    sample.reference_words as f64,
                )
            })
            .flatten();
        report.push_str(&format!(
            "| {:03} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            sample.id,
            if sample.silence { "Stille" } else { "Sprache" },
            decimal(Some(sample.audio_seconds), 3),
            decimal(Some(sample.latency_seconds), 3),
            decimal(ratio(sample.latency_seconds, sample.audio_seconds), 3),
            sample.reference_words,
            sample.substitutions,
            sample.deletions,
            sample.insertions,
            percent(wer),
            if sample.silence {
                if sample.phantom { "ja" } else { "nein" }
            } else {
                "—"
            },
        ));
    }
    report.push_str("\n### Zehnsekundenproben\n\n");
    if result.probes.is_empty() {
        report.push_str("Keine Sprachaufnahme mit mindestens zehn Sekunden verfügbar; die Zeitgrenze ist nicht nachgewiesen.\n\n");
        return;
    }
    report.push_str("| Nr. | Lauf 1 (s) | Lauf 2 (s) | Lauf 3 (s) | Median (s) | Leere Ausgaben |\n| --- | ---: | ---: | ---: | ---: | ---: |\n");
    for probe in &result.probes {
        report.push_str(&format!(
            "| {:03} | {} | {} | {} | {} | {} |\n",
            probe.id,
            decimal(Some(probe.seconds[0]), 3),
            decimal(Some(probe.seconds[1]), 3),
            decimal(Some(probe.seconds[2]), 3),
            decimal(median(probe.seconds), 3),
            probe.empty_outputs,
        ));
    }
    let maximum = result
        .probes
        .iter()
        .flat_map(|probe| probe.seconds)
        .max_by(f64::total_cmp);
    report.push_str(&format!(
        "\nHöchster Probe-Median: {} s; langsamster Einzellauf: {} s; leere Ausgaben: {}.\n\n",
        decimal(worst_probe_median(result), 3),
        decimal(maximum, 3),
        result
            .probes
            .iter()
            .map(|probe| probe.empty_outputs)
            .sum::<usize>(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::{Probe, SampleMetrics};

    #[test]
    fn unsuccessful_warmup_preserves_the_recording_table_and_disqualifies_the_model() {
        let mut model = result("test-model", 1, 0.5);
        model.warmup_succeeded = false;
        model.warmup_attempts = 2;
        assert!(select_models(&[model.clone()]).0.is_none());
        let mut report = String::new();
        render_model(&mut report, &model);
        let note = report.find("Aufwärmen: 2 Versuche").unwrap();
        let table = report.find("### Vollständige Aufnahmen").unwrap();
        assert!(note < table);
        assert!(!report.contains("warme vollständige Aufnahmen"));
    }

    fn result(id: &str, errors: usize, latency: f64) -> ModelResult {
        ModelResult {
            model_id: id.into(),
            cold_id: 1,
            cold_seconds: 2.5,
            warmup_attempts: 1,
            warmup_succeeded: true,
            peak_ram_bytes: 64 * 1024 * 1024,
            samples: vec![SampleMetrics {
                id: 1,
                audio_seconds: 10.0,
                latency_seconds: 1.0,
                reference_words: 10,
                substitutions: errors,
                deletions: 0,
                insertions: 0,
                silence: false,
                phantom: false,
                first_word_matches: true,
            }],
            probes: vec![Probe {
                id: 1,
                seconds: [latency - 0.1, latency + 0.1, latency],
                empty_outputs: 0,
            }],
        }
    }

    fn context() -> ReportContext {
        ReportContext {
            date: "2026-11-20".into(),
            threads: 2,
            logical_processors: 8,
            processor: "Testprozessor".into(),
            build: "release".into(),
            revision: "abc123".into(),
            uninstalled_models: vec!["uninstalled-model".into()],
        }
    }

    #[test]
    fn reports_have_no_blank_line_at_eof_and_keep_section_spacing() {
        let with_probes = result("with-probes", 1, 0.8);
        let mut without_probes = result("without-probes", 1, 0.8);
        without_probes.probes.clear();
        for models in [
            vec![],
            vec![with_probes.clone()],
            vec![without_probes.clone()],
            vec![with_probes.clone(), without_probes.clone()],
            vec![without_probes, with_probes],
        ] {
            let report = render_report(&context(), &models);
            assert!(report.ends_with('\n'));
            assert!(!report.lines().last().unwrap().trim().is_empty());
            if let Some(second) = models.get(1) {
                assert!(report.contains(&format!("\n\n## {}\n\n", second.model_id)));
            }
            if !models.is_empty() {
                assert!(report.contains("\n\n### Zehnsekundenproben\n\n"));
            }
        }
    }

    #[test]
    fn benchmark_files_have_no_blank_line_at_eof() {
        for contents in [
            include_str!("../Cargo.toml"),
            include_str!("../../../docs/benchmarks/asr-2026-11-20.md"),
        ] {
            assert!(contents.ends_with('\n'));
            assert!(!contents.lines().last().unwrap().trim().is_empty());
        }
    }

    #[test]
    fn selects_lowest_pooled_error_rate_under_the_strict_latency_limit() {
        let models = [
            result("accurate-but-slow", 0, 1.5),
            result("eligible-b", 2, 0.5),
            result("eligible-a", 1, 1.4),
        ];
        let (standard, fallback) = select_models(&models);
        assert_eq!(standard.unwrap().model_id, "eligible-a");
        assert_eq!(fallback.unwrap().model_id, "eligible-b");
    }

    #[test]
    fn requires_all_probe_medians_and_nonempty_outputs_for_qualification() {
        let mut slow_probe = result("slow-probe", 0, 0.2);
        slow_probe.probes.push(Probe {
            id: 2,
            seconds: [1.4, 1.5, 1.6],
            empty_outputs: 0,
        });
        let mut empty_output = result("empty-output", 0, 0.2);
        empty_output.probes[0].empty_outputs = 1;
        let mut no_probe = result("no-probe", 0, 0.2);
        no_probe.probes.clear();
        let mut no_words = result("no-words", 0, 0.2);
        no_words.samples[0].reference_words = 0;
        let models = [slow_probe, empty_output, no_probe, no_words];
        assert!(select_models(&models).0.is_none());
        assert!(select_models(&[]).0.is_none());
    }

    #[test]
    fn empty_probes_remain_visible_without_overriding_the_requested_wer_and_time_rule() {
        let mut model = result("test-model", 3, 0.8);
        model.probes.push(Probe {
            id: 3,
            seconds: [0.03; 3],
            empty_outputs: 3,
        });
        assert!(qualifies(&model));
        let report = render_report(&context(), &[model.clone()]);
        assert!(report.contains("| 003 | 0,030 | 0,030 | 0,030 | 0,030 | 3 |"));
        assert!(report.contains("30,00 %"));
        model.probes[1].seconds = [1.6; 3];
        assert!(!qualifies(&model));
    }

    #[test]
    fn keeps_an_unqualified_fallback_separate_from_a_valid_standard() {
        let models = [result("standard", 2, 0.8), result("fallback", 0, 2.0)];
        let (standard, fallback) = select_models(&models);
        assert_eq!(standard.unwrap().model_id, "standard");
        assert_eq!(fallback.unwrap().model_id, "fallback");
        let report = render_report(&context(), &models);
        assert!(report.contains("Rückfallmodell: `fallback` (Auswahlkriterium nicht erfüllt)"));
        let single = [result("single", 0, 0.8)];
        assert!(select_models(&single).1.is_none());
    }

    #[test]
    fn reports_pooled_wer_weighted_rtf_silence_and_all_numbered_measurements() {
        let mut model = result("test-model", 2, 0.8);
        model.samples.push(SampleMetrics {
            id: 2,
            audio_seconds: 30.0,
            latency_seconds: 3.0,
            reference_words: 90,
            substitutions: 0,
            deletions: 0,
            insertions: 0,
            silence: false,
            phantom: false,
            first_word_matches: true,
        });
        model.samples.push(SampleMetrics {
            id: 41,
            audio_seconds: 10.0,
            latency_seconds: 0.5,
            reference_words: 0,
            substitutions: 0,
            deletions: 0,
            insertions: 3,
            silence: true,
            phantom: true,
            first_word_matches: false,
        });
        let report = render_report(&context(), &[model]);
        for expected in [
            "2026-11-20",
            "Testprozessor",
            "abc123",
            "release",
            "uninstalled-model",
            "2,00 %",
            "64,00 MiB",
            "Sprachaufnahmen: 0,100",
            "alle Aufnahmen: 0,090",
            "Mittelwert: 1,500 s",
            "P95: 3,000 s",
            "Maximum: 3,000 s",
            "Phantomtexte: 1 von 1",
            "| 001 | Sprache |",
            "| 002 | Sprache |",
            "| 041 | Stille |",
            "| 001 | 0,700 | 0,900 | 0,800 | 0,800 | 0 |",
            "Kaltstart",
            "2,500 s",
            "VAD",
            "kein Rohmodelltest",
        ] {
            assert!(
                report.contains(expected),
                "Missing report value: {expected}"
            );
        }
        assert!(!report.contains("NaN"));
        assert!(!report.contains("| inf |"));
        assert!(!report.contains(" inf s"));
    }

    #[test]
    fn missing_and_invalid_metrics_cannot_produce_a_passing_recommendation() {
        for invalid in [f64::NAN, f64::INFINITY, -1.0] {
            let mut model = result("invalid", 0, 0.3);
            model.probes[0].seconds[0] = invalid;
            assert!(select_models(&[model]).0.is_none());
        }
        let report = render_report(&context(), &[result("slow", 0, 2.0)]);
        assert!(report.contains("Kein Modell erfüllt das Auswahlkriterium"));
        let report = render_report(&context(), &[]);
        assert!(report.contains("Keine installierten ASR-Modelle gemessen"));
        assert!(!report.contains("NaN"));
    }

    #[test]
    fn ties_are_deterministic_and_model_names_do_not_break_markdown() {
        let models = [result("b", 1, 0.8), result("a", 1, 0.8)];
        assert_eq!(select_models(&models).0.unwrap().model_id, "a");
        let mut metadata = context();
        metadata.processor = "CPU|name\n## injected".into();
        let report = render_report(&metadata, &models);
        assert!(!report.contains("\n## injected"));
        assert!(report.contains("CPU\\|name"));
    }
}
