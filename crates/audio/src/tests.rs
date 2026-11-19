use super::*;
use crate::backend::{Backend, InputPacket, OpenedInput, PACKET_SAMPLES};
use crate::capture::CaptureBuffer;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn recording_prepends_only_the_latest_half_second_once() {
    let mut capture = CaptureBuffer::new();
    capture.accept(&(0..12_000).map(|i| i as f32).collect::<Vec<_>>());
    capture.begin().unwrap();
    capture.accept(&[12_000.0, 12_001.0]);
    assert_eq!(
        capture.finish().unwrap().samples(),
        &(4_000..12_002).map(|i| i as f32).collect::<Vec<_>>()
    );
    assert!(capture.finish().is_none());
}

#[test]
fn cold_start_and_duplicate_start_preserve_real_audio() {
    let mut capture = CaptureBuffer::new();
    capture.accept(&[0.1, 0.2]);
    capture.begin().unwrap();
    assert_eq!(capture.begin(), Err(AudioError::AlreadyRecording));
    capture.accept(&[0.3]);
    assert_eq!(capture.finish().unwrap().samples(), &[0.1, 0.2, 0.3]);
}

#[test]
fn device_reset_preserves_recording_but_clears_old_preroll() {
    let mut capture = CaptureBuffer::new();
    capture.accept(&[0.1]);
    capture.begin().unwrap();
    capture.reset_preroll();
    capture.accept(&[0.2]);
    assert_eq!(capture.finish().unwrap().samples(), &[0.1, 0.2]);
    capture.begin().unwrap();
    assert_eq!(capture.finish().unwrap().samples(), &[0.2]);
}

#[test]
fn recording_debug_never_contains_audio() {
    let recording = Recording::new(vec![0.1234567; 16_000]);
    assert_eq!(recording.duration(), Duration::from_secs(1));
    assert!(!format!("{recording:?}").contains("0.1234567"));
}

struct FakeStream(Arc<AtomicUsize>);
impl Drop for FakeStream {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
struct FakeConnection {
    data: Sender<InputPacket>,
    errors: Sender<AudioError>,
    notices: Sender<AudioError>,
    overflow: Arc<AtomicBool>,
}
struct FakeBackend {
    online: Arc<AtomicBool>,
    live: Arc<AtomicUsize>,
    attempts: Arc<AtomicUsize>,
    connections: Sender<(DeviceSelection, FakeConnection)>,
}
impl Backend for FakeBackend {
    type Stream = FakeStream;
    fn open(
        &mut self,
        selection: &DeviceSelection,
    ) -> Result<OpenedInput<Self::Stream>, AudioError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if !self.online.load(Ordering::SeqCst) {
            return Err(AudioError::NoInputDevice);
        }
        if selection == &DeviceSelection::Named("unbekannt".into()) {
            return Err(AudioError::DeviceNotFound("unbekannt".into()));
        }
        let (data, samples) = bounded(64);
        let (errors_tx, errors) = bounded(1);
        let (notices_tx, notices) = bounded(2);
        let overflow = Arc::new(AtomicBool::new(false));
        self.live.fetch_add(1, Ordering::SeqCst);
        self.connections
            .send((
                selection.clone(),
                FakeConnection {
                    data,
                    errors: errors_tx,
                    notices: notices_tx,
                    overflow: Arc::clone(&overflow),
                },
            ))
            .unwrap();
        Ok(OpenedInput {
            stream: FakeStream(Arc::clone(&self.live)),
            device: DeviceInfo {
                name: match selection {
                    DeviceSelection::Default => "Standard".into(),
                    DeviceSelection::Named(name) => name.clone(),
                },
            },
            sample_rate: SAMPLE_RATE,
            channels: 1,
            samples,
            errors,
            notices,
            overflow,
        })
    }
}
struct Harness {
    audio: AudioCapture,
    online: Arc<AtomicBool>,
    live: Arc<AtomicUsize>,
    attempts: Arc<AtomicUsize>,
    connections: Receiver<(DeviceSelection, FakeConnection)>,
    events: Receiver<AudioEvent>,
    levels: Receiver<f32>,
}
impl Harness {
    fn new(online: bool, selection: DeviceSelection) -> Self {
        let online = Arc::new(AtomicBool::new(online));
        let live = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::new(AtomicUsize::new(0));
        let (connections_tx, connections) = unbounded();
        let backend = FakeBackend {
            online: Arc::clone(&online),
            live: Arc::clone(&live),
            attempts: Arc::clone(&attempts),
            connections: connections_tx,
        };
        let (events_tx, events) = unbounded();
        let (levels_tx, levels) = unbounded();
        let audio = AudioCapture::spawn(
            move || Ok(backend),
            selection,
            move |level| {
                let _ = levels_tx.send(level);
            },
            move |event| {
                let _ = events_tx.send(event);
            },
        )
        .unwrap();
        Self {
            audio,
            online,
            live,
            attempts,
            connections,
            events,
            levels,
        }
    }
    fn connection(&self) -> (DeviceSelection, FakeConnection) {
        self.connections
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
    }
}
fn send_samples(connection: &FakeConnection, value: f32, count: usize) {
    for start in (0..count).step_by(PACKET_SAMPLES) {
        connection
            .data
            .send(InputPacket {
                samples: [value; PACKET_SAMPLES],
                len: (count - start).min(PACKET_SAMPLES),
            })
            .unwrap();
    }
}

#[test]
fn worker_prepends_audio_reports_rms_and_stop_releases_stream() {
    let mut h = Harness::new(true, DeviceSelection::Default);
    let (_, connection) = h.connection();
    send_samples(&connection, 0.25, 16_000);
    h.audio.start_recording().unwrap();
    send_samples(&connection, 0.5, 1_600);
    let recording = h.audio.finish_recording().unwrap();
    assert_eq!(recording.samples().len(), 9_600);
    assert!(recording.samples()[..8_000].iter().all(|s| *s == 0.25));
    assert!(recording.samples()[8_000..].iter().all(|s| *s == 0.5));
    let levels: Vec<_> = h.levels.try_iter().collect();
    assert!((32..=33).contains(&levels.len()));
    assert!(levels.iter().all(|level| (0.0..=1.0).contains(level)));
    assert!(h.audio.stop_ready().unwrap().is_none());
    assert_eq!(h.live.load(Ordering::SeqCst), 0);
    assert_eq!(h.audio.start_recording(), Err(AudioError::NotReady));
    assert!(h.audio.stop_ready().unwrap().is_none());
}

#[test]
fn unplug_falls_back_and_late_old_errors_cannot_break_new_stream() {
    let mut h = Harness::new(true, DeviceSelection::Named("USB".into()));
    let (_, first) = h.connection();
    h.audio.start_recording().unwrap();
    send_samples(&first, 0.1, 800);
    first
        .errors
        .send(AudioError::Backend("Gerät entfernt".into()))
        .unwrap();
    let (selection, second) = h.connection();
    assert_eq!(selection, DeviceSelection::Default);
    assert_eq!(h.live.load(Ordering::SeqCst), 1);
    assert!(first.errors.send(AudioError::NoInputDevice).is_err());
    send_samples(&second, 0.2, 800);
    let recording = h.audio.stop_ready().unwrap().unwrap();
    assert_eq!(
        recording.samples(),
        &[vec![0.1; 800], vec![0.2; 800]].concat()
    );
    assert!(
        h.events
            .try_iter()
            .any(|e| matches!(e, AudioEvent::DeviceChanged { fallback: true, .. }))
    );
    assert_eq!(h.live.load(Ordering::SeqCst), 0);
}

#[test]
fn changing_device_keeps_recording_and_resets_preroll() {
    let mut h = Harness::new(true, DeviceSelection::Default);
    let (_, first) = h.connection();
    send_samples(&first, 0.1, 800);
    h.audio.start_recording().unwrap();
    h.audio
        .select_device(DeviceSelection::Named("USB".into()))
        .unwrap();
    let (_, second) = h.connection();
    send_samples(&second, 0.2, 800);
    assert_eq!(
        h.audio.finish_recording().unwrap().samples(),
        &[vec![0.1; 800], vec![0.2; 800]].concat()
    );
    h.audio.start_recording().unwrap();
    assert_eq!(
        h.audio.finish_recording().unwrap().samples(),
        &vec![0.2; 800]
    );
    h.audio.stop_ready().unwrap();
}

#[test]
fn missing_device_retries_then_recovers_and_drop_joins_worker() {
    let h = Harness::new(false, DeviceSelection::Default);
    h.audio.start_recording().unwrap();
    h.online.store(true, Ordering::SeqCst);
    let (_, connection) = h.connection();
    send_samples(&connection, 0.3, 100);
    assert_eq!(h.audio.finish_recording().unwrap().samples().len(), 100);
    assert!(h.attempts.load(Ordering::SeqCst) >= 2);
    let live = Arc::clone(&h.live);
    drop(h);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn missing_named_device_falls_back_with_event() {
    let mut h = Harness::new(true, DeviceSelection::Named("unbekannt".into()));
    let (selection, _connection) = h.connection();
    assert_eq!(selection, DeviceSelection::Default);
    h.audio.stop_ready().unwrap();
    assert!(
        h.events
            .try_iter()
            .any(|e| matches!(e, AudioEvent::DeviceChanged { fallback: true, .. }))
    );
}

#[test]
fn queue_overflow_is_reported() {
    let mut h = Harness::new(true, DeviceSelection::Default);
    let (_, connection) = h.connection();
    connection.overflow.store(true, Ordering::SeqCst);
    send_samples(&connection, 0.1, 100);
    h.audio.stop_ready().unwrap();
    assert!(
        h.events
            .try_iter()
            .any(|e| matches!(e, AudioEvent::InputOverflow))
    );
}

#[test]
fn stalled_device_falls_back_without_callback_error() {
    let mut h = Harness::new(true, DeviceSelection::Named("USB".into()));
    let (_, _first) = h.connection();
    let (selection, _second) = h.connection();
    assert_eq!(selection, DeviceSelection::Default);
    h.audio.stop_ready().unwrap();
}

#[test]
fn transient_glitch_reopens_the_selected_device_without_fallback() {
    let mut h = Harness::new(true, DeviceSelection::Named("USB".into()));
    let (_, first) = h.connection();
    h.audio.start_recording().unwrap();
    send_samples(&first, 0.1, 800);
    first.notices.send(AudioError::StreamGlitch).unwrap();
    let (selection, second) = h.connection();
    assert_eq!(selection, DeviceSelection::Named("USB".into()));
    send_samples(&second, 0.2, 800);
    assert_eq!(
        h.audio.stop_ready().unwrap().unwrap().samples(),
        &[vec![0.1; 800], vec![0.2; 800]].concat()
    );
    assert!(
        h.events
            .try_iter()
            .any(|e| matches!(e, AudioEvent::InputDiscontinuity))
    );
}

#[test]
fn denied_realtime_priority_reports_a_notice_without_reopening() {
    let mut h = Harness::new(true, DeviceSelection::Named("USB".into()));
    let (_, connection) = h.connection();
    connection.notices.send(AudioError::RealtimeDenied).unwrap();
    loop {
        if h.events.recv_timeout(Duration::from_secs(2)).unwrap() == AudioEvent::RealtimeUnavailable
        {
            break;
        }
    }
    assert_eq!(h.attempts.load(Ordering::SeqCst), 1);
    h.audio.start_recording().unwrap();
    send_samples(&connection, 0.1, 100);
    assert_eq!(
        h.audio.stop_ready().unwrap().unwrap().samples(),
        &vec![0.1; 100]
    );
}

#[test]
fn stopping_without_a_device_joins_the_retry_worker() {
    let mut h = Harness::new(false, DeviceSelection::Default);
    h.audio.stop_ready().unwrap();
    assert!(h.audio.worker.is_none());
    assert_eq!(h.live.load(Ordering::SeqCst), 0);
    assert_eq!(h.attempts.load(Ordering::SeqCst), 1);
}
