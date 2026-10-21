use crate::{SyncerProcessConfig, SyncerRunSummary};
use std::path::Path;
use tracing::Dispatch;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

pub struct SyncerLogger {
    dispatch: Dispatch,
    _worker_guard: Option<WorkerGuard>,
}

impl SyncerLogger {
    pub fn console() -> Self {
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .finish();
        Self {
            dispatch: Dispatch::new(subscriber),
            _worker_guard: None,
        }
    }

    pub fn file(log_directory: impl AsRef<Path>) -> anyhow::Result<Self> {
        std::fs::create_dir_all(log_directory.as_ref())?;
        let appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("adscope-syncer.log")
            .max_log_files(14)
            .build(log_directory)?;
        let (writer, worker_guard) = tracing_appender::non_blocking(appender);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(writer)
            .finish();
        Ok(Self {
            dispatch: Dispatch::new(subscriber),
            _worker_guard: Some(worker_guard),
        })
    }

    pub fn log_startup(&self, config: &SyncerProcessConfig) {
        tracing::dispatcher::with_default(&self.dispatch, || {
            tracing::info!(config = ?config, "syncer starting");
        });
    }

    pub fn log_run_result(&self, result: &anyhow::Result<SyncerRunSummary>) {
        tracing::dispatcher::with_default(&self.dispatch, || match result {
            Ok(summary) => tracing::info!(summary = ?summary, "syncer sync completed"),
            Err(error) => {
                tracing::error!(error = %format_args!("{error:#}"), "syncer sync failed")
            }
        });
    }

    pub fn log_process_error(&self, error: &anyhow::Error) {
        tracing::dispatcher::with_default(&self.dispatch, || {
            tracing::error!(error = %format_args!("{error:#}"), "syncer process failed");
        });
    }
}
