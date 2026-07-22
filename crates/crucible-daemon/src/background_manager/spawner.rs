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
}
