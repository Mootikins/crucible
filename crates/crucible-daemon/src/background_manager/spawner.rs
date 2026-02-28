use super::*;

#[async_trait]
impl BackgroundSpawner for BackgroundJobManager {
    async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<JobId, JobError> {
        self.spawn_bash(session_id, command, workdir, timeout)
            .await
            .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }

    fn list_jobs(&self, session_id: &str) -> Vec<JobInfo> {
        BackgroundJobManager::list_jobs(self, session_id)
    }

    fn get_job_result(&self, job_id: &JobId) -> Option<JobResult> {
        BackgroundJobManager::get_job_result(self, job_id)
    }

    async fn cancel_job(&self, job_id: &JobId) -> bool {
        BackgroundJobManager::cancel_job(self, job_id).await
    }

    async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<JobId, JobError> {
        BackgroundJobManager::spawn_subagent(self, session_id, prompt, context)
            .await
            .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }

    async fn spawn_subagent_blocking(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
        config: SubagentBlockingConfig,
        cancel_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<JobResult, JobError> {
        BackgroundJobManager::spawn_subagent_blocking(
            self, session_id, prompt, context, config, cancel_rx,
        )
        .await
        .map_err(|e| JobError::SpawnFailed(e.to_string()))
    }
}
