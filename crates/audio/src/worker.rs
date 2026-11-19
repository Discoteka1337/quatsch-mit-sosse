use crate::{
    AudioError, AudioEvent, DeviceSelection, Recording,
    backend::{Backend, InputPacket, OpenedInput},
    capture::CaptureBuffer,
    dsp::{Converter, RmsMeter},
};
use crossbeam_channel::{Receiver, Sender, never, select_biased};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const RETRY_INTERVAL: Duration = Duration::from_millis(500);
const STALL_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) enum Command {
    Start(Sender<Result<(), AudioError>>),
    Finish(Sender<Result<Recording, AudioError>>),
    Select(DeviceSelection, Sender<Result<(), AudioError>>),
    Stop(Sender<Result<Option<Recording>, AudioError>>),
}

pub(crate) struct Worker<B: Backend> {
    backend: B,
    selection: DeviceSelection,
    fallback: bool,
    input: Option<OpenedInput<B::Stream>>,
    converter: Option<Converter>,
    capture: CaptureBuffer,
    meter: RmsMeter,
    output: Vec<f32>,
    last_packet: Instant,
    last_error: Option<AudioError>,
    on_level: Box<dyn FnMut(f32) + Send>,
    on_event: Box<dyn FnMut(AudioEvent) + Send>,
}

impl<B: Backend> Worker<B> {
    pub(crate) fn new(
        backend: B,
        selection: DeviceSelection,
        on_level: impl FnMut(f32) + Send + 'static,
        on_event: impl FnMut(AudioEvent) + Send + 'static,
    ) -> Self {
        Self {
            backend,
            selection,
            fallback: false,
            input: None,
            converter: None,
            capture: CaptureBuffer::new(),
            meter: RmsMeter::new(),
            output: Vec::with_capacity(4096),
            last_packet: Instant::now(),
            last_error: None,
            on_level: Box::new(on_level),
            on_event: Box::new(on_event),
        }
    }

    pub(crate) fn run(
        mut self,
        commands: Receiver<Command>,
        started: Sender<Result<(), AudioError>>,
    ) {
        let _ = self.connect();
        if started.send(Ok(())).is_err() {
            return;
        }
        loop {
            // Each stream owns separate channels. Late packets/errors from a closed
            // stream can never enter a replacement stream's processing pipeline.
            let samples = self
                .input
                .as_ref()
                .map(|input| input.samples.clone())
                .unwrap_or_else(never);
            let errors = self
                .input
                .as_ref()
                .map(|input| input.errors.clone())
                .unwrap_or_else(never);
            let notices = self
                .input
                .as_ref()
                .map(|input| input.notices.clone())
                .unwrap_or_else(never);
            let timeout = if self.input.is_some() {
                STALL_TIMEOUT.saturating_sub(self.last_packet.elapsed())
            } else {
                RETRY_INTERVAL
            };
            select_biased! {
                recv(commands) -> command => {
                    match command {
                        Ok(Command::Stop(reply)) => {
                            self.close_input();
                            let recording = self.capture.finish();
                            (self.on_event)(AudioEvent::Stopped);
                            let _ = reply.send(Ok(recording));
                            return;
                        }
                        Ok(command) => {
                            self.drain_pending();
                            match command {
                                Command::Start(reply) => { let _ = reply.send(self.capture.begin()); }
                                Command::Finish(reply) => {
                                    let _ = reply.send(self.capture.finish().ok_or(AudioError::NotRecording));
                                }
                                Command::Select(selection, reply) => {
                                    self.close_input();
                                    self.selection = selection;
                                    self.fallback = false;
                                    let result = self.connect();
                                    let _ = reply.send(result);
                                }
                                Command::Stop(_) => unreachable!(),
                            }
                        }
                        Err(_) => { self.close_input(); return; }
                    }
                }
                recv(errors) -> error => {
                    // Drop these clones before opening another stream.
                    drop(samples);
                    drop(errors);
                    drop(notices);
                    self.recover(error.unwrap_or_else(|_| AudioError::Backend("Der Mikrofonstream wurde beendet.".into())));
                }
                recv(notices) -> notice => {
                    match notice {
                        Ok(AudioError::StreamGlitch) => {
                            (self.on_event)(AudioEvent::InputDiscontinuity);
                            self.close_input();
                            let _ = self.connect();
                        }
                        Ok(AudioError::RealtimeDenied) => (self.on_event)(AudioEvent::RealtimeUnavailable),
                        Ok(error) => self.recover(error),
                        Err(_) => self.recover(AudioError::Backend("Der Mikrofonstream wurde beendet.".into())),
                    }
                }
                recv(samples) -> packet => {
                    match packet {
                        Ok(packet) => {
                            if self.overflowed() { self.recover_overflow(); }
                            else { self.process(packet); }
                        }
                        Err(_) => self.recover(AudioError::Backend("Das Mikrofon liefert keine Daten mehr.".into())),
                    }
                }
                default(timeout) => {
                    if self.input.is_some() {
                        self.recover(AudioError::Backend("Das Mikrofon liefert seit zwei Sekunden keine Daten.".into()));
                    } else { let _ = self.connect(); }
                }
            }
        }
    }

    fn connect(&mut self) -> Result<(), AudioError> {
        match self.open_selected() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.report_unavailable(error.clone());
                if matches!(self.selection, DeviceSelection::Named(_)) {
                    self.selection = DeviceSelection::Default;
                    self.fallback = true;
                    if let Err(fallback_error) = self.open_selected() {
                        self.report_unavailable(fallback_error);
                    }
                }
                Err(error)
            }
        }
    }

    fn open_selected(&mut self) -> Result<(), AudioError> {
        let input = self.backend.open(&self.selection)?;
        let converter = Converter::new(input.sample_rate, input.channels)
            .map_err(|message| AudioError::Backend(message.into()))?;
        let device = input.device.clone();
        self.input = Some(input);
        self.converter = Some(converter);
        self.meter = RmsMeter::new();
        self.last_packet = Instant::now();
        self.last_error = None;
        (self.on_event)(AudioEvent::DeviceChanged {
            device,
            fallback: self.fallback,
        });
        Ok(())
    }

    fn report_unavailable(&mut self, error: AudioError) {
        if self.last_error.as_ref() != Some(&error) {
            self.last_error = Some(error.clone());
            (self.on_event)(AudioEvent::DeviceUnavailable { error });
        }
    }

    fn recover(&mut self, error: AudioError) {
        let device = self.input.as_ref().map(|input| input.device.clone());
        self.close_input();
        if let Some(device) = device {
            (self.on_event)(AudioEvent::DeviceLost { device, error });
        }
        self.fallback |= matches!(self.selection, DeviceSelection::Named(_));
        self.selection = DeviceSelection::Default;
        let _ = self.connect();
    }

    fn overflowed(&self) -> bool {
        self.input
            .as_ref()
            .is_some_and(|input| input.overflow.load(Ordering::Acquire))
    }

    fn recover_overflow(&mut self) {
        self.close_input();
        let _ = self.connect();
    }

    fn drain_pending(&mut self) {
        if self.overflowed() {
            self.recover_overflow();
            return;
        }
        if let Some(input) = &self.input {
            let receiver = input.samples.clone();
            // A finite snapshot keeps commands responsive under continuous input.
            for _ in 0..receiver.len() {
                if let Ok(packet) = receiver.try_recv() {
                    self.process(packet);
                }
            }
        }
    }

    fn close_input(&mut self) {
        if let Some(input) = self.input.take() {
            // CPAL's destructor joins the callback thread. No locks or borrowed
            // callback state are held here. Drain only after the producer stops.
            drop(input.stream);
            if input.overflow.load(Ordering::Acquire) {
                (self.on_event)(AudioEvent::InputOverflow);
            } else {
                for packet in input.samples.try_iter() {
                    self.process(packet);
                }
            }
        }
        self.converter = None;
        self.capture.reset_preroll();
        self.meter = RmsMeter::new();
        (self.on_level)(0.0);
    }

    fn process(&mut self, packet: InputPacket) {
        let Some(converter) = &mut self.converter else {
            return;
        };
        self.output.clear();
        converter.process(&packet.samples[..packet.len], &mut self.output);
        self.capture.accept(&self.output);
        for sample in &self.output {
            if let Some(level) = self.meter.push(*sample) {
                (self.on_level)(level);
            }
        }
        self.last_packet = Instant::now();
    }
}
