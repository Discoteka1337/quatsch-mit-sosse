pub(crate) const SAMPLE_RATE: u32 = 16_000;
pub(crate) const PRE_ROLL_SAMPLES: usize = 8_000;

pub(crate) struct RingBuffer {
    samples: Vec<f32>,
    next: usize,
    len: usize,
}

impl RingBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            next: 0,
            len: 0,
        }
    }

    pub(crate) fn push(&mut self, sample: f32) {
        if self.samples.is_empty() {
            return;
        }
        self.samples[self.next] = sample;
        self.next = (self.next + 1) % self.samples.len();
        self.len = (self.len + 1).min(self.samples.len());
    }

    pub(crate) fn snapshot(&self) -> Vec<f32> {
        if self.len < self.samples.len() {
            return self.samples[..self.len].to_vec();
        }
        self.samples[self.next..]
            .iter()
            .chain(&self.samples[..self.next])
            .copied()
            .collect()
    }

    pub(crate) fn clear(&mut self) {
        self.samples.fill(0.0);
        self.next = 0;
        self.len = 0;
    }
}

fn sanitize(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) struct Converter {
    channels: usize,
    frame_sum: f64,
    frame_channels: usize,
    resampler: Option<Resampler>,
}

impl Converter {
    pub(crate) fn new(input_rate: u32, channels: usize) -> Result<Self, &'static str> {
        if input_rate == 0 || input_rate > 768_000 {
            return Err("Die Abtastrate muss zwischen 1 und 768000 Hz liegen.");
        }
        if channels == 0 || channels > 256 {
            return Err("Die Kanalzahl muss zwischen 1 und 256 liegen.");
        }
        Ok(Self {
            channels,
            frame_sum: 0.0,
            frame_channels: 0,
            resampler: (input_rate != SAMPLE_RATE).then(|| Resampler::new(input_rate)),
        })
    }

    pub(crate) fn process(&mut self, interleaved: &[f32], output: &mut Vec<f32>) {
        for sample in interleaved {
            self.frame_sum += f64::from(sanitize(*sample));
            self.frame_channels += 1;
            if self.frame_channels != self.channels {
                continue;
            }
            let mono = (self.frame_sum / self.channels as f64) as f32;
            self.frame_sum = 0.0;
            self.frame_channels = 0;
            match &mut self.resampler {
                Some(resampler) => resampler.push(mono, output),
                None => output.push(mono),
            }
        }
    }
}

struct Resampler {
    input_rate: u32,
    position: u32,
    phases: Vec<Vec<f32>>,
    history: Vec<f32>,
    next: usize,
    len: usize,
}

impl Resampler {
    fn new(input_rate: u32) -> Self {
        let ratio = f64::from(input_rate) / f64::from(SAMPLE_RATE);
        let radius = (32.0 * ratio.max(1.0)).ceil() as usize;
        let cutoff = 0.94 / ratio.max(1.0);
        let phase_count = (SAMPLE_RATE / gcd(input_rate, SAMPLE_RATE)).min(1_024);
        let phases = (0..=phase_count)
            .map(|phase| {
                let fraction = f64::from(phase) / f64::from(phase_count);
                let mut coefficients: Vec<_> = (0..=radius * 2)
                    .map(|tap| {
                        let distance = tap as f64 - radius as f64 + fraction;
                        if distance.abs() > radius as f64 {
                            return 0.0;
                        }
                        let angle = std::f64::consts::PI * distance / radius as f64;
                        let window = 0.42 + 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos();
                        let x = std::f64::consts::PI * distance * cutoff;
                        let sinc = if x.abs() < f64::EPSILON {
                            1.0
                        } else {
                            x.sin() / x
                        };
                        cutoff * sinc * window
                    })
                    .collect();
                let gain: f64 = coefficients.iter().sum();
                for coefficient in &mut coefficients {
                    *coefficient /= gain;
                }
                coefficients.into_iter().map(|value| value as f32).collect()
            })
            .collect();
        Self {
            input_rate,
            position: 0,
            phases,
            history: vec![0.0; radius * 2 + 1],
            next: 0,
            len: 0,
        }
    }

    fn push(&mut self, sample: f32, output: &mut Vec<f32>) {
        self.history[self.next] = sample;
        self.next = (self.next + 1) % self.history.len();
        self.len = (self.len + 1).min(self.history.len());

        // An integer phase accumulator avoids drift over long recordings. The causal
        // FIR delays audio by ceil(32 * max(input_rate / 16000, 1)) input frames.
        while self.position < SAMPLE_RATE {
            let phase = self.position as usize * (self.phases.len() - 1);
            let low = phase / SAMPLE_RATE as usize;
            let blend = (phase % SAMPLE_RATE as usize) as f32 / SAMPLE_RATE as f32;
            let coefficients = &self.phases[low];
            let following = &self.phases[low + 1];
            let mut index = self.next;
            let mut value = 0.0_f64;
            for tap in 0..self.len {
                index = if index == 0 {
                    self.history.len()
                } else {
                    index
                } - 1;
                let coefficient = coefficients[tap] + blend * (following[tap] - coefficients[tap]);
                value += f64::from(self.history[index]) * f64::from(coefficient);
            }
            output.push(sanitize(value as f32));
            self.position += self.input_rate;
        }
        self.position -= SAMPLE_RATE;
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

pub(crate) struct RmsMeter {
    phase: u32,
    samples: u32,
    sum_squares: f64,
}

impl RmsMeter {
    pub(crate) fn new() -> Self {
        Self {
            phase: 0,
            samples: 0,
            sum_squares: 0.0,
        }
    }

    pub(crate) fn push(&mut self, sample: f32) -> Option<f32> {
        let value = f64::from(sanitize(sample));
        self.sum_squares += value * value;
        self.samples += 1;
        self.phase += 30;
        if self.phase < SAMPLE_RATE {
            return None;
        }
        self.phase -= SAMPLE_RATE;
        let level = (self.sum_squares / f64::from(self.samples)).sqrt() as f32;
        self.samples = 0;
        self.sum_squares = 0.0;
        Some(level.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(rate: u32, frequency: f32, amplitude: f32) -> Vec<f32> {
        (0..rate)
            .map(|index| amplitude * (TAU * frequency * index as f32 / rate as f32).sin())
            .collect()
    }

    fn convert(rate: u32, samples: &[f32], chunk: usize) -> Vec<f32> {
        let mut converter = Converter::new(rate, 1).unwrap();
        let mut output = Vec::new();
        for block in samples.chunks(chunk) {
            converter.process(block, &mut output);
        }
        output
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample.powi(2)).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn ring_keeps_the_newest_samples_in_chronological_order() {
        let mut ring = RingBuffer::new(4);
        assert!(ring.snapshot().is_empty());
        for index in 0..3 {
            ring.push(index as f32);
        }
        assert_eq!(ring.snapshot(), [0.0, 1.0, 2.0]);
        for index in 3..10 {
            ring.push(index as f32);
        }
        assert_eq!(ring.snapshot(), [6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn ring_capacity_is_exactly_half_a_second() {
        assert_eq!(PRE_ROLL_SAMPLES, SAMPLE_RATE as usize / 2);
        let mut ring = RingBuffer::new(PRE_ROLL_SAMPLES);
        for index in 0..20_000 {
            ring.push(index as f32);
        }
        let snapshot = ring.snapshot();
        assert_eq!(snapshot.len(), 8_000);
        assert_eq!(snapshot[0], 12_000.0);
        assert_eq!(snapshot[7_999], 19_999.0);
        ring.clear();
        assert!(ring.snapshot().is_empty());
        ring.push(0.25);
        assert_eq!(ring.snapshot(), [0.25]);
    }

    #[test]
    fn zero_capacity_ring_is_empty() {
        let mut ring = RingBuffer::new(0);
        ring.push(0.5);
        assert!(ring.snapshot().is_empty());
    }

    #[test]
    fn invalid_formats_are_rejected() {
        assert!(Converter::new(0, 1).is_err());
        assert!(Converter::new(48_000, 0).is_err());
        assert!(Converter::new(u32::MAX, 1).is_err());
        assert!(Converter::new(48_000, usize::MAX).is_err());
    }

    #[test]
    fn mono_at_target_rate_is_unchanged_without_filter_delay() {
        let input = sine(SAMPLE_RATE, 1_000.0, 0.8);
        assert_eq!(convert(SAMPLE_RATE, &input, 97), input);
    }

    #[test]
    fn stereo_downmix_handles_incomplete_frames_between_callbacks() {
        let input = [0.2, 0.6, -0.2, 0.2, 0.8, -0.4];
        let mut converter = Converter::new(SAMPLE_RATE, 2).unwrap();
        let mut output = vec![0.7];
        converter.process(&input[..1], &mut output);
        assert_eq!(output, [0.7]);
        converter.process(&input[1..5], &mut output);
        assert_eq!(output.len(), 3);
        converter.process(&input[5..], &mut output);
        for (actual, expected) in output.iter().zip([0.7, 0.4, 0.0, 0.2]) {
            assert!((actual - expected).abs() < 0.000_001);
        }
    }

    #[test]
    fn resampled_sines_preserve_frequency_level_and_duration() {
        for rate in [8_000, 44_100, 48_000, 96_000] {
            let output = convert(rate, &sine(rate, 1_000.0, 0.8), 211);
            assert_eq!(output.len(), SAMPLE_RATE as usize, "rate {rate}");
            let settled = &output[800..15_200];
            let expected_rms = 0.8 / 2.0_f32.sqrt();
            assert!((rms(settled) - expected_rms).abs() < 0.002, "rate {rate}");
            let crossings = settled
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count();
            assert!((899..=901).contains(&crossings), "rate {rate}");
            // The causal FIR has a fixed delay; compare the interior to that timeline.
            let delay = (32.0 * (rate as f64 / SAMPLE_RATE as f64).max(1.0)).ceil() / rate as f64;
            let error = output[800..15_200]
                .iter()
                .enumerate()
                .map(|(index, actual)| {
                    let time = (index + 800) as f64 / SAMPLE_RATE as f64 - delay;
                    let expected = 0.8 * (std::f64::consts::TAU * 1_000.0 * time).sin();
                    (f64::from(*actual) - expected).powi(2)
                })
                .sum::<f64>()
                / settled.len() as f64;
            assert!(error.sqrt() < 0.001, "rate {rate}");
        }
    }

    #[test]
    fn resampling_is_independent_of_callback_boundaries() {
        for rate in [8_000, 44_100, 48_000, 47_999] {
            let input = sine(rate, 731.0, 0.6);
            let whole = convert(rate, &input, input.len());
            for chunk in [1, 137, 4_096] {
                assert_eq!(convert(rate, &input, chunk), whole);
            }
        }
    }

    #[test]
    fn above_nyquist_signal_is_rejected_before_downsampling() {
        for rate in [44_100, 48_000, 96_000] {
            let output = convert(rate, &sine(rate, 12_000.0, 0.8), 173);
            assert!(rms(&output[800..]) < 0.001, "rate {rate}");
        }
    }

    #[test]
    fn silence_and_non_finite_input_stay_finite_and_bounded() {
        for rate in [SAMPLE_RATE, 48_000] {
            let silence = convert(rate, &vec![0.0; rate as usize], 53);
            assert!(silence.iter().all(|sample| *sample == 0.0));
            let samples = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -2.0, 2.0];
            let output = convert(rate, &samples.repeat(1_000), 7);
            assert!(
                output
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
            );
        }
    }

    #[test]
    fn rms_reports_thirty_times_each_second_without_rounding_drift() {
        let mut meter = RmsMeter::new();
        let mut reports = Vec::new();
        for index in 0..SAMPLE_RATE * 10 {
            if let Some(level) = meter.push(0.5) {
                reports.push(index);
                assert!((level - 0.5).abs() < 0.000_001);
            }
        }
        assert_eq!(reports.len(), 300);
        assert!(
            reports
                .windows(2)
                .all(|pair| (533..=534).contains(&(pair[1] - pair[0])))
        );
    }

    #[test]
    fn rms_matches_a_sine_and_clamps_invalid_input() {
        let mut meter = RmsMeter::new();
        let levels: Vec<_> = sine(SAMPLE_RATE, 300.0, 0.8)
            .into_iter()
            .filter_map(|sample| meter.push(sample))
            .collect();
        assert_eq!(levels.len(), 30);
        assert!(
            levels
                .iter()
                .all(|level| (*level - 0.8 / 2.0_f32.sqrt()).abs() < 0.001)
        );
        for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 2.0, -2.0] {
            for _ in 0..1_000 {
                if let Some(level) = meter.push(sample) {
                    assert!(level.is_finite() && (0.0..=1.0).contains(&level));
                }
            }
        }
    }
}
