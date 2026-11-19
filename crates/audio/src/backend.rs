use std::sync::{Arc, atomic::AtomicBool};

use crossbeam_channel::Receiver;

use crate::{AudioError, DeviceInfo, DeviceSelection};

pub(crate) const PACKET_SAMPLES: usize = 2048;

pub(crate) struct InputPacket {
    pub samples: [f32; PACKET_SAMPLES],
    pub len: usize,
}

pub(crate) struct OpenedInput<S> {
    pub stream: S,
    pub device: DeviceInfo,
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: Receiver<InputPacket>,
    pub errors: Receiver<AudioError>,
    pub notices: Receiver<AudioError>,
    pub overflow: Arc<AtomicBool>,
}

pub(crate) trait Backend {
    type Stream;

    fn open(
        &mut self,
        selection: &DeviceSelection,
    ) -> Result<OpenedInput<Self::Stream>, AudioError>;
}

#[cfg(any(windows, test))]
fn choose_named_device<T>(
    devices: impl Iterator<Item = Result<(String, T), AudioError>>,
    name: &str,
) -> Result<T, AudioError> {
    let mut selected = None;
    for entry in devices {
        let (device_name, device) = entry?;
        if device_name == name {
            if selected.is_some() {
                return Err(AudioError::AmbiguousDeviceName(name.to_owned()));
            }
            selected = Some(device);
        }
    }
    selected.ok_or_else(|| AudioError::DeviceNotFound(name.to_owned()))
}

#[cfg(windows)]
pub(crate) use windows::{CpalBackend, default_input_device, input_devices};

#[cfg(windows)]
mod windows {
    use std::{sync::atomic::Ordering, time::Duration};

    use cpal::{
        FromSample, Sample, SampleFormat, SizedSample,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    };
    use crossbeam_channel::{Sender, TrySendError, bounded};

    use super::*;

    const QUEUE_CAPACITY: usize = 64;

    pub(crate) struct CpalBackend {
        host: cpal::Host,
    }

    impl CpalBackend {
        pub(crate) fn new() -> Result<Self, AudioError> {
            let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(backend_error)?;
            Ok(Self { host })
        }

        fn select(&self, selection: &DeviceSelection) -> Result<cpal::Device, AudioError> {
            match selection {
                DeviceSelection::Default => self
                    .host
                    .default_input_device()
                    .ok_or(AudioError::NoInputDevice),
                DeviceSelection::Named(name) => choose_named_device(
                    self.host
                        .input_devices()
                        .map_err(backend_error)?
                        .map(|device| {
                            let info = device_info(&device)?;
                            Ok((info.name, device))
                        }),
                    name,
                ),
            }
        }
    }

    impl Backend for CpalBackend {
        type Stream = cpal::Stream;

        fn open(
            &mut self,
            selection: &DeviceSelection,
        ) -> Result<OpenedInput<Self::Stream>, AudioError> {
            let device = self.select(selection)?;
            let info = device_info(&device)?;
            let supported = device.default_input_config().map_err(backend_error)?;
            let sample_format = supported.sample_format();
            let config = supported.config();
            if config.sample_rate == 0 || config.channels == 0 {
                return Err(AudioError::Backend(
                    "Das Mikrofon meldet ein ungültiges Audioformat.".to_owned(),
                ));
            }
            let sample_rate = config.sample_rate;
            let channels = usize::from(config.channels);
            let (samples_tx, samples) = bounded(QUEUE_CAPACITY);
            let (errors_tx, errors) = bounded(1);
            let (notices_tx, notices) = bounded(2);
            let overflow = Arc::new(AtomicBool::new(false));

            macro_rules! build {
                ($sample:ty) => {
                    build_stream::<$sample>(
                        &device,
                        config,
                        samples_tx,
                        errors_tx,
                        notices_tx,
                        Arc::clone(&overflow),
                    )
                };
            }
            let stream = match sample_format {
                SampleFormat::I8 => build!(i8),
                SampleFormat::I16 => build!(i16),
                SampleFormat::I24 => build!(cpal::I24),
                SampleFormat::I32 => build!(i32),
                SampleFormat::I64 => build!(i64),
                SampleFormat::U8 => build!(u8),
                SampleFormat::U16 => build!(u16),
                SampleFormat::U24 => build!(cpal::U24),
                SampleFormat::U32 => build!(u32),
                SampleFormat::U64 => build!(u64),
                SampleFormat::F32 => build!(f32),
                SampleFormat::F64 => build!(f64),
                _ => {
                    return Err(AudioError::Backend(
                        "Das Mikrofon liefert ein nicht unterstütztes Audioformat.".to_owned(),
                    ));
                }
            }?;
            stream.play().map_err(backend_error)?;
            Ok(OpenedInput {
                stream,
                device: info,
                sample_rate,
                channels,
                samples,
                errors,
                notices,
                overflow,
            })
        }
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        samples: Sender<InputPacket>,
        errors: Sender<AudioError>,
        notices: Sender<AudioError>,
        overflow: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, AudioError>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        device
            .build_input_stream::<T, _, _>(
                config,
                move |data, _| send_samples(data, &samples, &overflow),
                move |error| route_stream_error(error, &errors, &notices),
                Some(Duration::from_secs(2)),
            )
            .map_err(backend_error)
    }

    pub(super) fn route_stream_error(
        error: cpal::Error,
        errors: &Sender<AudioError>,
        notices: &Sender<AudioError>,
    ) {
        match error.kind() {
            // A rerouted stream remains valid; WASAPI invalidation is a different kind.
            cpal::ErrorKind::DeviceChanged => {}
            cpal::ErrorKind::Xrun => {
                let _ = notices.try_send(AudioError::StreamGlitch);
            }
            cpal::ErrorKind::RealtimeDenied => {
                let _ = notices.try_send(AudioError::RealtimeDenied);
            }
            _ => {
                // A saturated notice queue must never suppress a device failure.
                let _ = errors.try_send(backend_error(error));
            }
        }
    }

    pub(super) fn send_samples<T>(data: &[T], sender: &Sender<InputPacket>, overflow: &AtomicBool)
    where
        T: Sample,
        f32: FromSample<T>,
    {
        for chunk in data.chunks(PACKET_SAMPLES) {
            let mut packet = InputPacket {
                samples: [0.0; PACKET_SAMPLES],
                len: chunk.len(),
            };
            for (output, input) in packet.samples.iter_mut().zip(chunk) {
                *output = input.to_sample::<f32>();
            }
            match sender.try_send(packet) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    overflow.store(true, Ordering::Release);
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }

    fn device_info(device: &cpal::Device) -> Result<DeviceInfo, AudioError> {
        Ok(DeviceInfo {
            name: device
                .description()
                .map_err(backend_error)?
                .name()
                .to_owned(),
        })
    }

    fn backend_error(error: cpal::Error) -> AudioError {
        AudioError::Backend(format!("Mikrofonzugriff fehlgeschlagen: {error}"))
    }

    pub(crate) fn input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
        CpalBackend::new()?
            .host
            .input_devices()
            .map_err(backend_error)?
            .map(|device| device_info(&device))
            .collect()
    }

    pub(crate) fn default_input_device() -> Result<Option<DeviceInfo>, AudioError> {
        CpalBackend::new()?
            .host
            .default_input_device()
            .as_ref()
            .map(device_info)
            .transpose()
    }
}

#[cfg(not(windows))]
pub(crate) struct CpalBackend;

#[cfg(not(windows))]
impl CpalBackend {
    pub(crate) fn new() -> Result<Self, AudioError> {
        Err(AudioError::UnsupportedPlatform)
    }
}

#[cfg(not(windows))]
impl Backend for CpalBackend {
    type Stream = ();

    fn open(&mut self, _: &DeviceSelection) -> Result<OpenedInput<Self::Stream>, AudioError> {
        Err(AudioError::UnsupportedPlatform)
    }
}

#[cfg(not(windows))]
pub(crate) fn input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    Err(AudioError::UnsupportedPlatform)
}

#[cfg(not(windows))]
pub(crate) fn default_input_device() -> Result<Option<DeviceInfo>, AudioError> {
    Err(AudioError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::sync::atomic::Ordering;

    fn named_devices(names: &[&str]) -> impl Iterator<Item = Result<(String, usize), AudioError>> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| Ok(((*name).to_owned(), index)))
    }

    #[test]
    fn device_selection_matches_the_complete_name() {
        let selected =
            choose_named_device(named_devices(&["Mikrofon USB", "Mikrofon"]), "Mikrofon").unwrap();
        assert_eq!(selected, 1);
    }

    #[test]
    fn absent_device_is_not_silently_replaced_in_the_backend() {
        assert!(matches!(
            choose_named_device(named_devices(&["Mikrofon USB"]), "Mikrofon"),
            Err(AudioError::DeviceNotFound(name)) if name == "Mikrofon"
        ));
    }

    #[test]
    fn duplicate_names_require_an_unambiguous_selection() {
        assert!(matches!(
            choose_named_device(named_devices(&["USB", "USB"]), "USB"),
            Err(AudioError::AmbiguousDeviceName(name)) if name == "USB"
        ));
    }

    #[test]
    fn failed_device_description_does_not_hide_a_possible_duplicate() {
        let entries = [
            Ok(("USB".to_owned(), 0)),
            Err(AudioError::Backend(
                "Gerätebeschreibung nicht verfügbar".to_owned(),
            )),
        ];
        assert!(matches!(
            choose_named_device(entries.into_iter(), "USB"),
            Err(AudioError::Backend(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn recoverable_stream_notices_do_not_enter_the_fatal_error_queue() {
        let (errors_tx, errors) = crossbeam_channel::bounded(1);
        let (notices_tx, notices) = crossbeam_channel::bounded(2);
        windows::route_stream_error(cpal::ErrorKind::Xrun.into(), &errors_tx, &notices_tx);
        windows::route_stream_error(
            cpal::ErrorKind::RealtimeDenied.into(),
            &errors_tx,
            &notices_tx,
        );
        assert!(errors.is_empty());
        assert!(matches!(notices.try_recv(), Ok(AudioError::StreamGlitch)));
        assert!(matches!(notices.try_recv(), Ok(AudioError::RealtimeDenied)));
    }

    #[cfg(windows)]
    #[test]
    fn a_full_notice_queue_cannot_hide_device_loss() {
        let (errors_tx, errors) = crossbeam_channel::bounded(1);
        let (notices_tx, notices) = crossbeam_channel::bounded(2);
        for _ in 0..3 {
            windows::route_stream_error(cpal::ErrorKind::Xrun.into(), &errors_tx, &notices_tx);
        }
        windows::route_stream_error(
            cpal::ErrorKind::DeviceNotAvailable.into(),
            &errors_tx,
            &notices_tx,
        );
        assert_eq!(notices.len(), 2);
        assert!(matches!(errors.try_recv(), Ok(AudioError::Backend(_))));
    }

    #[cfg(windows)]
    #[test]
    fn automatically_rerouted_streams_continue_without_a_restart() {
        let (errors_tx, errors) = crossbeam_channel::bounded(1);
        let (notices_tx, notices) = crossbeam_channel::bounded(2);
        windows::route_stream_error(
            cpal::ErrorKind::DeviceChanged.into(),
            &errors_tx,
            &notices_tx,
        );
        assert!(errors.is_empty());
        assert!(notices.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn integer_samples_are_normalized_to_float() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let overflow = AtomicBool::new(false);
        windows::send_samples(&[i16::MIN, 0, i16::MAX], &tx, &overflow);
        let packet = rx.recv().unwrap();
        assert_eq!(packet.len, 3);
        assert_eq!(packet.samples[0], -1.0);
        assert_eq!(packet.samples[1], 0.0);
        assert!((packet.samples[2] - 32767.0 / 32768.0).abs() < 1e-6);
        windows::send_samples(&[0_u8, 128, 255], &tx, &overflow);
        let packet = rx.recv().unwrap();
        assert_eq!(&packet.samples[..3], &[-1.0, 0.0, 127.0 / 128.0]);
        assert!(!overflow.load(Ordering::Relaxed));
    }

    #[cfg(windows)]
    #[test]
    fn packets_preserve_samples_across_arbitrary_chunk_boundaries() {
        let (tx, rx) = crossbeam_channel::bounded(3);
        let overflow = AtomicBool::new(false);
        let input = (0..PACKET_SAMPLES * 2 + 3)
            .map(|index| index as f32 / 8192.0)
            .collect::<Vec<_>>();
        windows::send_samples(&input, &tx, &overflow);
        let output = rx
            .try_iter()
            .flat_map(|packet| packet.samples[..packet.len].to_vec())
            .collect::<Vec<_>>();
        assert_eq!(output, input);
        assert!(!overflow.load(Ordering::Relaxed));
    }

    #[cfg(windows)]
    #[test]
    fn a_full_queue_sets_a_loss_flag_without_waiting() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let overflow = AtomicBool::new(false);
        windows::send_samples(&[0.5_f32; PACKET_SAMPLES + 1], &tx, &overflow);
        assert!(overflow.load(Ordering::Relaxed));
        assert_eq!(rx.len(), 1);
        assert_eq!(rx.recv().unwrap().len, PACKET_SAMPLES);
    }
}
