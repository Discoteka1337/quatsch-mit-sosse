use super::{Mixer, Session, Volume};
use sha2::{Digest, Sha256};
use std::io;
use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, S_OK},
        Media::Audio::{
            AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2,
            IAudioSessionEnumerator, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
    },
    core::{Interface, PWSTR},
};

impl Volume for ISimpleAudioVolume {
    fn level(&self) -> io::Result<f32> {
        unsafe { self.GetMasterVolume() }.map_err(io::Error::other)
    }

    fn set(&self, level: f32) -> io::Result<()> {
        unsafe { self.SetMasterVolume(level, std::ptr::null()) }.map_err(io::Error::other)
    }
}

pub(super) fn opaque_id(units: &[u16]) -> String {
    // Core Audio identifiers can include executable paths. Persist only a stable
    // digest of the exact UTF-16 code units, including unpaired surrogates.
    let mut hash = Sha256::new();
    for unit in units {
        hash.update(unit.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn take_id(raw: PWSTR) -> windows::core::Result<String> {
    // Core Audio returns caller-owned, null-terminated strings allocated by COM.
    if raw.is_null() {
        return Err(windows::core::Error::from_hresult(E_INVALIDARG));
    }
    let id = opaque_id(unsafe { raw.as_wide() });
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    Ok(id)
}

fn enumerate() -> windows::core::Result<Mixer<ISimpleAudioVolume>> {
    let mut mixer = Mixer::default();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for index in 0..devices.GetCount()? {
            if devices
                .Item(index)
                .and_then(|device| enumerate_device(&device, &mut mixer))
                .is_err()
            {
                mixer.complete = false;
            }
        }
    }
    Ok(mixer)
}

fn enumerate_device(
    device: &IMMDevice,
    mixer: &mut Mixer<ISimpleAudioVolume>,
) -> windows::core::Result<()> {
    unsafe {
        let id = take_id(device.GetId()?)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        for index in 0..sessions.GetCount()? {
            match read_session(&sessions, index, &id) {
                Ok(Some((instance, session))) => {
                    mixer.sessions.insert(instance, session);
                }
                Ok(None) => {}
                Err(_) => mixer.complete = false,
            }
        }
    }
    Ok(())
}

fn read_session(
    sessions: &IAudioSessionEnumerator,
    index: i32,
    device: &str,
) -> windows::core::Result<Option<(String, Session<ISimpleAudioVolume>)>> {
    unsafe {
        let session = sessions
            .GetSession(index)?
            .cast::<IAudioSessionControl2>()?;
        if session.GetState()? == AudioSessionStateExpired {
            return Ok(None);
        }
        let system = session.IsSystemSoundsSession();
        system.ok()?;
        if system == S_OK {
            return Ok(None);
        }
        let pid = session.GetProcessId()?;
        if pid == 0 || pid == std::process::id() {
            return Ok(None);
        }
        Ok(Some((
            take_id(session.GetSessionInstanceIdentifier()?)?,
            Session {
                device: device.to_owned(),
                group: take_id(session.GetSessionIdentifier()?)?,
                volume: session.cast()?,
            },
        )))
    }
}

pub(super) fn with_mixer(
    apply: impl FnOnce(&Mixer<ISimpleAudioVolume>) -> io::Result<()>,
) -> io::Result<()> {
    // The dictation worker owns this apartment. Release all interface references
    // there before balancing CoInitializeEx, including on enumeration failures.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(io::Error::other)?;
    let result = enumerate()
        .map_err(io::Error::other)
        .and_then(|m| apply(&m));
    unsafe { CoUninitialize() };
    result
}
