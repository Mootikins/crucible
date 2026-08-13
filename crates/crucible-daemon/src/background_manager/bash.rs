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

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        // Wait and both pipes CONCURRENTLY. Awaiting `wait()` first deadlocks:
        // a pipe holds 64 KiB on Linux, and a child that writes past that blocks
        // in `write()` until someone reads — so `wait()` never returns and the
        // job ends only when its timeout fires. Any verbose command hit this
        // (`cargo build`, a test run), and it looked like a slow command rather
        // than a stall. Reading stdout to EOF before touching stderr has the same
        // flaw one pipe over. tokio's own `Child::wait_with_output` uses
        // `try_join3` for exactly this reason, which is what this is.
        let wait_and_collect = async {
            use tokio::io::AsyncReadExt;
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            let (status, (), ()) = tokio::try_join!(
                child.wait(),
                async {
                    match stdout_handle.as_mut() {
                        Some(h) => h.read_to_end(&mut stdout_buf).await.map(|_| ()),
                        None => Ok(()),
                    }
                },
                async {
                    match stderr_handle.as_mut() {
                        Some(h) => h.read_to_end(&mut stderr_buf).await.map(|_| ()),
                        None => Ok(()),
                    }
                },
            )?;
            Ok::<_, std::io::Error>((
                status,
                String::from_utf8_lossy(&stdout_buf).to_string(),
                String::from_utf8_lossy(&stderr_buf).to_string(),
            ))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Output larger than a pipe buffer must come back whole, and promptly.
    ///
    /// The regression: `wait()` was awaited before either pipe was read. A pipe
    /// holds 64 KiB on Linux, so a child writing past that blocks in `write()`
    /// waiting for a reader that does not exist yet, while we wait for an exit
    /// that cannot happen — deadlocked until the job's timeout. 256 KiB is four
    /// buffers' worth, so this hangs pre-fix on any plausible pipe size.
    ///
    /// The timeout is the assertion: `BashError::Timeout` is what the deadlock
    /// produced, so a regression fails here rather than hanging the suite.
    #[tokio::test]
    async fn output_past_the_pipe_buffer_comes_back_whole() {
        const BYTES: usize = 256 * 1024;
        let (_cancel_tx, cancel_rx) = oneshot::channel();

        // Matched rather than `expect`ed: `BashError` derives nothing, and this
        // test is not a reason to widen a production type.
        let (stdout, exit_code) = match BackgroundJobManager::execute_bash_with_cancellation(
            format!("printf 'x%.0s' $(seq 1 {BYTES})"),
            None,
            Duration::from_secs(20),
            cancel_rx,
        )
        .await
        {
            Ok(out) => out,
            Err(BashError::Timeout) => {
                panic!("deadlocked: the child blocked writing while we waited for its exit")
            }
            Err(BashError::Cancelled) => panic!("nothing cancelled this job"),
            Err(BashError::Failed { message, .. }) => panic!("command failed: {message}"),
        };

        assert_eq!(exit_code, 0);
        assert_eq!(
            stdout.len(),
            BYTES,
            "the whole of stdout must survive, not one pipe buffer's worth"
        );
    }

    /// The same for stderr, which the old code read only after draining stdout —
    /// so a chatty stderr deadlocked against a quiet stdout's EOF.
    #[tokio::test]
    async fn a_chatty_stderr_does_not_block_on_stdout() {
        const BYTES: usize = 256 * 1024;
        let (_cancel_tx, cancel_rx) = oneshot::channel();

        match BackgroundJobManager::execute_bash_with_cancellation(
            format!("printf 'e%.0s' $(seq 1 {BYTES}) >&2; exit 3"),
            None,
            Duration::from_secs(20),
            cancel_rx,
        )
        .await
        {
            Err(BashError::Failed { exit_code, message }) => {
                assert_eq!(exit_code, Some(3));
                assert!(
                    message.contains(&"e".repeat(1024)),
                    "stderr must be captured, not truncated at a pipe buffer"
                );
            }
            Err(BashError::Timeout) => {
                panic!("deadlocked: stderr filled its pipe while stdout was drained first")
            }
            Err(BashError::Cancelled) => panic!("nothing cancelled this job"),
            Ok(_) => panic!("exit 3 must surface as a failure"),
        }
    }
}
