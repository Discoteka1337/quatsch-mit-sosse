use std::{f64::consts::TAU, sync::LazyLock};
use windows::{
    Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCSTR,
};

// Original 90 ms sine sweeps. Generate the samples locally with a soft envelope.
pub(super) fn wave(start: bool) -> Vec<u8> {
    const RATE: u32 = 22_050;
    const FRAMES: u32 = RATE * 90 / 1_000;
    const DATA_SIZE: u32 = FRAMES * 2;
    let mut bytes = Vec::with_capacity((44 + DATA_SIZE) as usize);
    bytes.extend(b"RIFF");
    bytes.extend((36 + DATA_SIZE).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes()); // PCM
    bytes.extend(1u16.to_le_bytes()); // Mono
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * 2).to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(DATA_SIZE.to_le_bytes());

    let (from, to) = if start {
        (495.0, 742.5)
    } else {
        (660.0, 440.0)
    };
    let mut phase: f64 = 0.0;
    for i in 0..FRAMES {
        let progress = f64::from(i) / f64::from(FRAMES - 1);
        let envelope = 0.5 - 0.5 * (TAU * progress).cos();
        bytes.extend(((3_800.0 * envelope * phase.sin()) as i16).to_le_bytes());
        phase += TAU * (from + (to - from) * progress) / f64::from(RATE);
    }
    bytes
}

pub(super) fn play(start: bool) {
    static START: LazyLock<Vec<u8>> = LazyLock::new(|| wave(true));
    static END: LazyLock<Vec<u8>> = LazyLock::new(|| wave(false));
    let cue = if start { &START } else { &END };
    // PlaySound keeps a pointer for asynchronous playback. Both buffers are static.
    unsafe {
        let _ = PlaySoundA(
            PCSTR(cue.as_ptr()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}
