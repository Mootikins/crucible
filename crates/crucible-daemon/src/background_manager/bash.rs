use super::*;

impl BackgroundJobManager {
    pub async fn spawn_bash(
        &self,
        session_id: &str,
        command: String,
        workdir: Option<PathBuf>,
        timeout: Option<Duration>,
    ) -> Result<JobId, BackgroundError> {
        let kind = JobKind::Bash {
            command: command.clone(),
            workdir: workdir.clone(),
        };
        let info = JobInfo::new(session_id.to_string(), kind);
        let job_id = info.id.clone();
        let timeout = timeout.unwrap_or(DEFAULT_BASH_TIMEOUT);
        let (cancel_tx, cancel_rx) = oneshot::channel();

        if !emit_event(
            &self.event_tx,
            SessionEventMessage::new(
                session_id,
                events::BASH_SPAWNED,
                serde_json::json!({
                    "job_id": job_id,
                    "command": command,
                }),
            ),
        ) {
            tracing::debug!("Failed to emit BASH_SPAWNED event (no subscribers)");
        }

        info!(
            job_id = %job_id,
            session_id = %session_id,
            command = %command,
            "Spawning background bash job"
        );

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let job_id = job_id.clone();
            let session_id = session_id.to_string();
            let max_history = self.max_history;
            let command = command.clone();

            tokio::spawn(async move {
                let result = Self::execute_bash_with_cancellation(
                    command.clone(),
                    workdir,
                    timeout,
                    cancel_rx,
                )
                .await;

                // Extract original JobInfo to preserve started_at timestamp
                let info = running
                    .remove(&job_id)
                    .map(|(_, rt)| rt.info)
                    .unwrap_or_else(|| {
                        // Fallback: job was already removed (shouldn't happen)
                        JobInfo::new(
                            session_id.clone(),
                            JobKind::Bash {
                                command: command.clone(),
                                workdir: None,
                            },
                        )
                    });

                let job_result = Self::build_job_result(info, result);
                Self::emit_completion_events(
                    &event_tx,
                    &session_id,
                    &job_result.info.id.clone(),
                    &job_result,
                );
                Self::add_to_history(&history, &session_id, job_result, max_history);

                debug!(job_id = %job_id, "Background bash job completed");
            })
        };

        self.running.insert(
            job_id.clone(),
            RunningJob {
                info,
                is_delegation: false,
                parent_session_id: None,
                cancel_tx,
                task_handle,
            },
        );

        Ok(job_id)
    }

    fn build_job_result(mut info: JobInfo, result: Result<(String, i32), BashError>) -> JobResult {
        match result {
            Ok((output, exit_code)) => {
                info.mark_completed();
                JobResult::success_with_exit_code(info, output, exit_code)
            }
            Err(BashError::Cancelled) => {
                info.mark_cancelled();
                JobResult::failure(info, "Job cancelled".to_string())
            }
            Err(BashError::Timeout) => {
                info.mark_failed();
                JobResult::failure(info, "Job timed out".to_string())
            }
            Err(BashError::Failed { message, exit_code }) => {
                info.mark_failed();
                match exit_code {
                    Some(code) => JobResult::failure_with_exit_code(info, message, code),
                    None => JobResult::failure(info, message),
                }
            }
        }
    }

    fn emit_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
    ) {
        let (event_type, event_data) = if result.is_success() {
            let output = result.output.as_deref().unwrap_or("");
            (
                events::BASH_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "output": truncate(output, 1000),
                    "exit_code": result.exit_code,
                }),
            )
        } else {
            let error = result.error.as_deref().unwrap_or("Unknown error");
            (
                events::BASH_FAILED,
                serde_json::json!({
                    "job_id": job_id,
                    "error": error,
                    "exit_code": result.exit_code,
                }),
            )
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(session_id, event_type, event_data),
        ) {
            warn!(job_id = %job_id, "No subscribers for bash completion event");
        }
        Self::emit_background_completed(event_tx, session_id, job_id, result, "bash");
    }

    pub(super) fn emit_background_completed(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
        kind: &str,
    ) {
        let summary = result.truncated_output(500);
        let summary = if summary.is_empty() {
            result
                .error
                .clone()
                .unwrap_or_else(|| "completed".to_string())
        } else {
            summary
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(
                session_id,
                events::BACKGROUND_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "kind": kind,
                    "summary": summary,
                }),
            ),
        ) {
            warn!(job_id = %job_id, kind = %kind, "No subscribers for background completion event");
        }
    }

    async fn execute_bash_with_cancellation(
        command: String,
        workdir: Option<PathBuf>,
        timeout: Duration,
        cancel_rx: oneshot::Receiver<()>,
    ) -> Result<(String, i32), BashError> {
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&command);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| BashError::Failed {
            message: format!("Spawn error: {e}"),
            exit_code: None,
        })?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let wait_and_collect = async {
            let status = child.wait().await?;

            let stdout = if let Some(mut h) = stdout_handle {
                use tokio::io::AsyncReadExt;
                let mut buf = Vec::new();
                h.read_to_end(&mut buf).await?;
                String::from_utf8_lossy(&buf).to_string()
            } else {
                String::new()
            };

            let stderr = if let Some(mut h) = stderr_handle {
                use tokio::io::AsyncReadExt;
                let mut buf = Vec::new();
                h.read_to_end(&mut buf).await?;
                String::from_utf8_lossy(&buf).to_string()
            } else {
                String::new()
            };

            Ok::<_, std::io::Error>((status, stdout, stderr))
        };

        tokio::select! {
            _ = cancel_rx => {
                let _ = child.kill().await;
                Err(BashError::Cancelled)
            }
            result = tokio::time::timeout(timeout, wait_and_collect) => {
                match result {
                    Ok(Ok((status, stdout, stderr))) => {
                        let exit_code = status.code().unwrap_or(-1);

                        if status.success() {
                            Ok((stdout, exit_code))
                        } else {
                            Err(BashError::Failed {
                                message: format!("Exit code: {exit_code}\nStdout:\n{stdout}\nStderr:\n{stderr}"),
                                exit_code: Some(exit_code),
                            })
                        }
                    }
                    Ok(Err(e)) => {
                        Err(BashError::Failed {
                            message: format!("Exec error: {e}"),
                            exit_code: None,
                        })
                    }
                    Err(_) => {
                        let _ = child.kill().await;
                        Err(BashError::Timeout)
                    }
                }
            }
        }
    }
}
