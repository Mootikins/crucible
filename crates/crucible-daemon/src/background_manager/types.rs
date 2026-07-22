use super::*;
use thiserror::Error;

pub(super) mod events {
    pub const BASH_SPAWNED: &str = "bash_job_spawned";
    pub const BASH_COMPLETED: &str = "bash_job_completed";
    pub const BASH_FAILED: &str = "bash_job_failed";
    pub const BACKGROUND_COMPLETED: &str = "background_job_completed";
}

#[derive(Error, Debug)]
pub enum BackgroundError {
    #[error("Job error: {0}")]
    Job(#[from] JobError),

    #[error("Failed to spawn job: {0}")]
    SpawnFailed(String),
}

pub(super) struct RunningJob {
    pub(crate) info: JobInfo,
    pub(crate) cancel_tx: oneshot::Sender<()>,
    #[allow(dead_code)] // stored to keep JoinHandle alive; dropping would detach the task
    pub(crate) task_handle: JoinHandle<()>,
}

pub(super) enum BashError {
    Cancelled,
    Timeout,
    Failed {
        message: String,
        exit_code: Option<i32>,
    },
}
