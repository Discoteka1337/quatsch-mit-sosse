use std::ops::Range;

use quatsch_mit_sosse_vad::{SAMPLE_RATE, SpeechSegment};

use crate::{Error, Result};

pub const MAX_CHUNK_SECONDS: u32 = 20;

pub(super) fn recognition_chunks(
    segments: &[SpeechSegment],
    sample_count: usize,
) -> Result<Vec<Range<usize>>> {
    let mut chunks = split_at_pauses(segments, sample_count, SAMPLE_RATE)?;
    if let Some(first) = chunks.first_mut() {
        // Silero already retains 100 ms. Extra context must fit without a new cut.
        let extra_onset_samples = SAMPLE_RATE as usize * 400 / 1_000;
        let room = (MAX_CHUNK_SECONDS * SAMPLE_RATE) as usize - first.len();
        first.start = first.start.saturating_sub(extra_onset_samples.min(room));
    }
    Ok(chunks)
}

/// Returns half-open sample ranges, retaining short internal pauses.
/// A continuous speech interval is cut at the limit only when no pause exists.
pub fn split_at_pauses(
    segments: &[SpeechSegment],
    sample_count: usize,
    sample_rate: u32,
) -> Result<Vec<Range<usize>>> {
    let limit = usize::try_from(sample_rate)
        .ok()
        .and_then(|rate| rate.checked_mul(MAX_CHUNK_SECONDS as usize))
        .filter(|&limit| limit > 0)
        .ok_or(Error::InvalidAudio)?;
    let mut previous_end = 0;
    let mut continuous: Vec<SpeechSegment> = Vec::new();
    for segment in segments {
        if segment.start < previous_end
            || segment.start >= segment.end
            || segment.end > sample_count
        {
            return Err(Error::InvalidSegments);
        }
        previous_end = segment.end;
        if let Some(previous) = continuous.last_mut()
            && previous.end == segment.start
        {
            previous.end = segment.end;
        } else {
            continuous.push(*segment);
        }
    }

    let mut chunks = Vec::new();
    let mut pending: Option<Range<usize>> = None;
    for segment in &continuous {
        if let Some(chunk) = pending.as_mut() {
            if segment.end - chunk.start <= limit {
                chunk.end = segment.end;
                continue;
            }
            chunks.push(pending.take().expect("pending chunk exists"));
        }
        let mut start = segment.start;
        while segment.end - start > limit {
            chunks.push(start..start + limit);
            start += limit;
        }
        pending = Some(start..segment.end);
    }
    if let Some(chunk) = pending {
        chunks.push(chunk);
    }
    Ok(chunks)
}

pub(crate) fn recognize_chunks(
    samples: &[f32],
    chunks: &[Range<usize>],
    mut recognize: impl FnMut(&[f32]) -> Result<String>,
) -> Result<String> {
    let mut text = String::new();
    for chunk in chunks {
        let part = recognize(&samples[chunk.clone()])?;
        let part = part.trim();
        if !part.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(part);
        }
    }
    if crate::phantoms::is_phantom(&text) {
        text.clear();
    }
    Ok(text)
}
