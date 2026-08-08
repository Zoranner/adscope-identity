use crate::{LoggingTarget, run_configured_connector};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::service_dispatcher;

pub const SERVICE_NAME: &str = "AdscopeConnector";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
static SERVICE_RUNTIME_DIR: OnceLock<PathBuf> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub fn run_service_dispatcher(runtime_dir: PathBuf) -> anyhow::Result<()> {
    SERVICE_RUNTIME_DIR
        .set(runtime_dir)
        .map_err(|_| anyhow::anyhow!("service runtime directory is already configured"))?;
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

fn service_main(_arguments: Vec<OsString>) {
    let _ = run_service();
}

fn run_service() -> anyhow::Result<()> {
    let runtime_dir = SERVICE_RUNTIME_DIR
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("service runtime directory is not configured"))?;
    let (stop_tx, stop_rx) = watch::channel(false);
    let status_slot = Arc::new(Mutex::new(None::<ServiceStatusHandle>));
    let handler_status_slot = Arc::clone(&status_slot);
    let event_handler = move |control| match control {
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        ServiceControl::Stop => {
            if let Ok(slot) = handler_status_slot.lock()
                && let Some(handle) = *slot
            {
                let _ = handle.set_service_status(service_status(
                    ServiceState::StopPending,
                    ServiceControlAccept::empty(),
                    1,
                    Duration::from_secs(10),
                    ServiceExitCode::Win32(0),
                ));
            }
            let _ = stop_tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
    *status_slot
        .lock()
        .map_err(|_| anyhow::anyhow!("service status lock is poisoned"))? = Some(status_handle);
    status_handle.set_service_status(service_status(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        1,
        Duration::from_secs(10),
        ServiceExitCode::Win32(0),
    ))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run_configured_connector(
        runtime_dir,
        LoggingTarget::File,
        stop_rx,
        || {
            status_handle.set_service_status(service_status(
                ServiceState::Running,
                ServiceControlAccept::STOP,
                0,
                Duration::ZERO,
                ServiceExitCode::Win32(0),
            ))?;
            Ok(())
        },
    ));
    let exit_code = if result.is_ok() { 0 } else { 1 };
    status_handle.set_service_status(service_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        0,
        Duration::ZERO,
        ServiceExitCode::Win32(exit_code),
    ))?;
    result
}

fn service_status(
    state: ServiceState,
    controls_accepted: ServiceControlAccept,
    checkpoint: u32,
    wait_hint: Duration,
    exit_code: ServiceExitCode,
) -> ServiceStatus {
    ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: state,
        controls_accepted,
        exit_code,
        checkpoint,
        wait_hint,
        process_id: None,
    }
}
