use super::*;
use thiserror::Error;

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
    /// Holds the daemon open while this job runs. Removing the entry from
    /// `running` drops it, which is the same instant the job ends.
    #[allow(dead_code)] // a guard: its Drop is the whole point
    pub(crate) work: crate::activity::WorkGuard,
}

pub(super) enum BashError {
    Cancelled,
    Timeout,
    Failed {
        message: String,
        exit_code: Option<i32>,
    },
}
