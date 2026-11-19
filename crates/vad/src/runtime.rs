use std::ffi::{c_char, c_void};
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::OnceLock;

use ort_sys::{OrtApi, OrtEnv, OrtLoggingLevel, OrtStatusPtr};

/// The native runtime cannot be used unless its privacy settings succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    ApiUnavailable,
    EnvironmentCreation,
    TelemetryUnavailable,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ApiUnavailable => "Die erforderliche lokale ONNX-Laufzeit ist nicht verfügbar.",
            Self::EnvironmentCreation => "Die lokale ONNX-Laufzeit konnte nicht gestartet werden.",
            Self::TelemetryUnavailable => {
                "Die ONNX-Telemetrie konnte nicht deaktiviert werden; die Verarbeitung wurde gesperrt."
            }
        })
    }
}

impl std::error::Error for RuntimeError {}

static ENVIRONMENT: OnceLock<Result<NativeEnvironment, RuntimeError>> = OnceLock::new();

/// Initialize the shared ONNX Runtime environment before any sherpa model loads.
///
/// The environment stays alive until process exit, without models or worker
/// pools. ORT reuses it for sherpa's subsequent `CreateEnv` calls. This prevents
/// model unloading/reloading from replacing the privacy configuration.
pub fn initialize_runtime() -> Result<(), RuntimeError> {
    ENVIRONMENT
        .get_or_init(|| initialize_with(&NativeBackend))
        .as_ref()
        .map(|_| ())
        .map_err(|error| *error)
}

trait RuntimeBackend {
    type Environment;

    fn create_environment(&self) -> Result<Self::Environment, RuntimeError>;
    fn disable_telemetry(&self, environment: &Self::Environment) -> Result<(), RuntimeError>;
}

fn initialize_with<B: RuntimeBackend>(backend: &B) -> Result<B::Environment, RuntimeError> {
    let environment = backend.create_environment()?;
    backend.disable_telemetry(&environment)?;
    Ok(environment)
}

struct NativeBackend;

struct NativeEnvironment {
    pointer: NonNull<OrtEnv>,
    api: &'static OrtApi,
}

// SAFETY: ORT environments are process-shared reference-counted resources.
// This wrapper never exposes the pointer or mutates the environment after
// OnceLock initialization. The API table is immutable for the library lifetime.
unsafe impl Send for NativeEnvironment {}
unsafe impl Sync for NativeEnvironment {}

impl Drop for NativeEnvironment {
    fn drop(&mut self) {
        // SAFETY: The pointer is one successful CreateEnv reference and is
        // released exactly once. The successful singleton is retained forever.
        unsafe { (self.api.ReleaseEnv)(self.pointer.as_ptr()) };
    }
}

impl RuntimeBackend for NativeBackend {
    type Environment = NativeEnvironment;

    fn create_environment(&self) -> Result<Self::Environment, RuntimeError> {
        let base = runtime_api_base()?;
        // SAFETY: GetApi returns a process-lifetime immutable table, or null
        // for an incompatible library. API 17 matches sherpa's ORT 1.17.1.
        let api = unsafe { (base.GetApi)(17).as_ref() }.ok_or(RuntimeError::ApiUnavailable)?;
        let mut pointer = ptr::null_mut();
        // SAFETY: The callback never dereferences or retains native buffers.
        // All arguments remain valid for this call; ORT copies the log ID.
        let status = unsafe {
            (api.CreateEnvWithCustomLogger)(
                discard_runtime_log,
                ptr::null_mut(),
                OrtLoggingLevel::ORT_LOGGING_LEVEL_FATAL,
                c"quatsch-mit-sosse".as_ptr(),
                &mut pointer,
            )
        };
        check_status(api, status, RuntimeError::EnvironmentCreation)?;
        let pointer = NonNull::new(pointer).ok_or(RuntimeError::EnvironmentCreation)?;
        Ok(NativeEnvironment { pointer, api })
    }

    fn disable_telemetry(&self, environment: &Self::Environment) -> Result<(), RuntimeError> {
        // SAFETY: The environment is live and the API belongs to its runtime.
        let status =
            unsafe { (environment.api.DisableTelemetryEvents)(environment.pointer.as_ptr()) };
        check_status(environment.api, status, RuntimeError::TelemetryUnavailable)
    }
}

#[cfg(windows)]
fn runtime_api_base() -> Result<&'static ort_sys::OrtApiBase, RuntimeError> {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW, GetProcAddress,
    };
    use windows::core::{s, w};

    // sherpa-onnx-c-api.dll imports onnxruntime.dll, so Windows has already
    // loaded the matching runtime before entering Rust. Look up only that
    // loaded module: do not search disk or load a second runtime by filename.
    let mut module = HMODULE::default();
    // SAFETY: The output handle is writable; the module name is static UTF-16.
    // PIN guarantees that the API table and retained environment stay valid.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_PIN,
            w!("onnxruntime.dll"),
            &mut module,
        )
    }
    .map_err(|_| RuntimeError::ApiUnavailable)?;
    // SAFETY: The module is pinned and the export name is static and terminated.
    let symbol = unsafe { GetProcAddress(module, s!("OrtGetApiBase")) }
        .ok_or(RuntimeError::ApiUnavailable)?;
    // SAFETY: ONNX Runtime's public C ABI specifies exactly this signature.
    // The Windows release omits its import library, so resolve its export from
    // the very DLL already used by sherpa rather than linking another runtime.
    let get_api: unsafe extern "system" fn() -> *const ort_sys::OrtApiBase =
        unsafe { std::mem::transmute(symbol) };
    // SAFETY: The returned table is immutable and lives as long as the module.
    unsafe { get_api().as_ref() }.ok_or(RuntimeError::ApiUnavailable)
}

#[cfg(not(windows))]
fn runtime_api_base() -> Result<&'static ort_sys::OrtApiBase, RuntimeError> {
    // SAFETY: sherpa-rs-sys links the matching ONNX Runtime library on these
    // targets; ort-sys supplies only API-17 bindings without extra downloads.
    unsafe { ort_sys::OrtGetApiBase().as_ref() }.ok_or(RuntimeError::ApiUnavailable)
}

fn check_status(
    api: &OrtApi,
    status: OrtStatusPtr,
    error: RuntimeError,
) -> Result<(), RuntimeError> {
    if status.0.is_null() {
        return Ok(());
    }
    // SAFETY: Non-null statuses returned by ORT must be released with this
    // function. Error strings are intentionally neither read nor logged.
    unsafe { (api.ReleaseStatus)(status.0) };
    Err(error)
}

unsafe extern "system" fn discard_runtime_log(
    _parameter: *mut c_void,
    _severity: OrtLoggingLevel,
    _category: *const c_char,
    _log_id: *const c_char,
    _code_location: *const c_char,
    _message: *const c_char,
) {
    // Runtime diagnostics can contain model inputs; do not forward them.
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    struct FakeEnvironment(Rc<RefCell<Vec<&'static str>>>);

    impl Drop for FakeEnvironment {
        fn drop(&mut self) {
            self.0.borrow_mut().push("release");
        }
    }

    struct FakeBackend {
        events: Rc<RefCell<Vec<&'static str>>>,
        fail_create: bool,
        fail_disable: bool,
    }

    impl RuntimeBackend for FakeBackend {
        type Environment = FakeEnvironment;

        fn create_environment(&self) -> Result<Self::Environment, RuntimeError> {
            self.events.borrow_mut().push("create");
            if self.fail_create {
                return Err(RuntimeError::EnvironmentCreation);
            }
            Ok(FakeEnvironment(Rc::clone(&self.events)))
        }

        fn disable_telemetry(&self, _: &Self::Environment) -> Result<(), RuntimeError> {
            self.events.borrow_mut().push("disable");
            if self.fail_disable {
                Err(RuntimeError::TelemetryUnavailable)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn environment_is_ready_only_after_telemetry_is_disabled() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let backend = FakeBackend {
            events: Rc::clone(&events),
            fail_create: false,
            fail_disable: false,
        };
        let environment = initialize_with(&backend).unwrap();
        assert_eq!(*events.borrow(), ["create", "disable"]);
        drop(environment);
        assert_eq!(*events.borrow(), ["create", "disable", "release"]);
    }

    #[test]
    fn telemetry_failure_releases_environment_and_fails_closed() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let backend = FakeBackend {
            events: Rc::clone(&events),
            fail_create: false,
            fail_disable: true,
        };
        assert!(matches!(
            initialize_with(&backend),
            Err(RuntimeError::TelemetryUnavailable)
        ));
        assert_eq!(*events.borrow(), ["create", "disable", "release"]);
    }

    #[test]
    fn creation_failure_never_attempts_to_use_environment() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let backend = FakeBackend {
            events: Rc::clone(&events),
            fail_create: true,
            fail_disable: false,
        };
        assert!(matches!(
            initialize_with(&backend),
            Err(RuntimeError::EnvironmentCreation)
        ));
        assert_eq!(*events.borrow(), ["create"]);
    }

    #[test]
    fn native_runtime_reuses_private_environment_without_models() {
        initialize_runtime().unwrap();
        initialize_runtime().unwrap();
        let retained = ENVIRONMENT.get().unwrap().as_ref().unwrap();
        let another = NativeBackend.create_environment().unwrap();
        assert_eq!(retained.pointer, another.pointer);
    }
}
