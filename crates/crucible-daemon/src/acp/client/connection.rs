use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{
    CloseSessionRequest, ErrorCode, InitializeRequest, McpServer, McpServerHttp, McpServerStdio,
    NewSessionRequest, ResumeSessionRequest, SessionId,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::Lines;
use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use tokio::process::Command;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::recording::{Direction, Recorder};
use super::{CrucibleAcpClient, PermissionRequestHandler};
use crate::acp::session::{AcpSession, ResumeDisposition};
use crate::acp::{ClientError, Result};

impl CrucibleAcpClient {
    /// Start the agent process in `config` and connect to it over its stdio.
    ///
    /// With `CRUCIBLE_ACP_RECORD_DIR` set, every line in each direction goes
    /// to a fixture file (see `recording.rs`).
    pub async fn spawn(
        config: super::ClientConfig,
        agent_name: impl Into<String>,
        permission: Option<PermissionRequestHandler>,
    ) -> Result<Self> {
        let agent_name = agent_name.into();
        tracing::info!(agent = %agent_name, path = %config.agent_path.display(), "Spawning ACP agent process");

        let mut cmd = Command::new(&config.agent_path);
        // Closing the pipes only sends EOF, and a hung agent ignores it.
        cmd.kill_on_drop(true);
        // A group of its own lets the drop kill the children of a launcher
        // too. The SDK's `AcpAgent` does the same.
        #[cfg(unix)]
        cmd.process_group(0);
        if let Some(args) = &config.agent_args {
            cmd.args(args);
        }
        if let Some(dir) = &config.working_dir {
            cmd.current_dir(dir);
        }
        for (key, value) in config.env_vars.iter().flatten() {
            cmd.env(key, value);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| ClientError::Connection(format!("Failed to spawn agent: {e}")))?;

        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(ClientError::Connection(
                "Failed to capture the agent stdio".into(),
            ));
        };
        if let Some(stderr) = child.stderr.take() {
            let name = agent_name.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt as _;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        tracing::debug!(agent = %name, "[agent stderr] {}", line.trim());
                    }
                }
            });
        }

        let recorder = Recorder::from_env(&agent_name).map(|r| Arc::new(Mutex::new(r)));
        let transport = recorded_lines(stdin, stdout, recorder);
        let mut client = Self::connect(config, transport, agent_name, permission).await?;
        client._child = Some(AgentProcess(child));
        Ok(client)
    }

    /// Run the handshake and open a session.
    ///
    /// 1. `initialize` reads the agent capabilities.
    /// 2. The Crucible MCP server goes to the agent over Streamable HTTP when
    ///    `mcp_url` is given and the agent takes HTTP. Otherwise it goes over
    ///    stdio, which every ACP agent must take. `McpServer::Sse` is the
    ///    legacy SSE transport, which our server does not speak.
    /// 3. With `resume_session_id`, `session/resume` continues that agent
    ///    session. The attempt does not wait for `sessionCapabilities.resume`,
    ///    because an agent can answer the method without the flag. A `-32601`
    ///    or `-32002` answer falls back to `session/new`.
    pub async fn handshake(
        &mut self,
        mcp_url: Option<&str>,
        resume_session_id: Option<&str>,
    ) -> Result<AcpSession> {
        let init = self
            .handshake_request(InitializeRequest::new(ProtocolVersion::V1))
            .await?;
        tracing::info!(
            agent = %self.agent_name,
            protocol_version = %init.protocol_version,
            http_mcp = init.agent_capabilities.mcp_capabilities.http,
            agent_info = ?init.agent_info,
            "ACP initialization complete"
        );
        self.caps = init.agent_capabilities;

        let mcp_server = match mcp_url {
            Some(url) if self.agent_supports_http_mcp() => {
                tracing::info!(agent = %self.agent_name, %url, "Offering the MCP server over Streamable HTTP");
                McpServer::Http(McpServerHttp::new("crucible", url))
            }
            _ => {
                tracing::info!(agent = %self.agent_name, "Offering the MCP server over stdio");
                stdio_mcp_server()
            }
        };

        // Must be absolute: the agent runs in `working_dir`, so a relative
        // cwd would resolve twice.
        let cwd = self
            .config
            .working_dir
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));

        let mut resume = ResumeDisposition::NotAttempted;
        if let Some(prior) = resume_session_id {
            let request =
                ResumeSessionRequest::new(SessionId::from(prior.to_string()), cwd.clone())
                    .mcp_servers(vec![mcp_server.clone()]);
            match self.handshake_call(request).await? {
                Ok(response) => {
                    tracing::info!(agent = %self.agent_name, session_id = %prior, "ACP agent resumed its session");
                    return Ok(AcpSession::new(
                        prior.to_string(),
                        response.modes,
                        response.config_options,
                        ResumeDisposition::Resumed,
                    ));
                }
                // `-32601`: the agent does not speak the method. `-32002`: it
                // no longer knows the session; claude-agent-acp and codex-acp
                // answer this for a session that never kept a turn. Another
                // error has a cause that a fallback would hide.
                Err(error)
                    if matches!(
                        error.code,
                        ErrorCode::MethodNotFound | ErrorCode::ResourceNotFound
                    ) =>
                {
                    tracing::warn!(agent = %self.agent_name, code = ?error.code, "session/resume refused; falling back to session/new");
                    resume = ResumeDisposition::FellBackToNew;
                }
                Err(error) => return Err(super::request_error("session/resume", &error)),
            }
        }

        let response = self
            .handshake_request(NewSessionRequest::new(cwd).mcp_servers(vec![mcp_server]))
            .await?;
        tracing::info!(agent = %self.agent_name, session_id = %response.session_id, "ACP agent connected with session");
        Ok(AcpSession::new(
            response.session_id.to_string(),
            response.modes,
            response.config_options,
            resume,
        ))
    }

    /// Send `session/close` so the agent frees the session. A `-32601`
    /// answer is not an error: the agent exits on the pipe close after it.
    pub async fn close(&self, session_id: &str) -> Result<()> {
        let request = CloseSessionRequest::new(SessionId::from(session_id.to_string()));
        match self.cx.send_request(request).block_task().await {
            Ok(_) => Ok(()),
            Err(error) if error.code == ErrorCode::MethodNotFound => Ok(()),
            Err(error) => Err(super::request_error("session/close", &error)),
        }
    }
}

/// The agent process. A drop kills its whole process group: an agent that a
/// launcher starts (`npx`, `uvx`, a sandbox prefix) is a child of the
/// launcher, and a kill of the launcher alone leaves it alive.
/// `kill_on_drop` then kills and reaps the launcher itself.
pub(super) struct AgentProcess(tokio::process::Child);

impl Drop for AgentProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.id().and_then(|pid| libc::pid_t::try_from(pid).ok()) {
            // SAFETY: `killpg` takes two integers and touches no memory. The
            // group id is the child pid, because the spawn used
            // `process_group(0)`. `id()` is `None` after the child is reaped,
            // so the pid is not a reused one.
            unsafe {
                libc::killpg(pid, libc::SIGKILL);
            }
        }
    }
}

/// A line transport over the agent stdio. A recorder, when present, gets
/// each line in each direction.
fn recorded_lines(
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    recorder: Option<Arc<Mutex<Recorder>>>,
) -> Lines<
    impl futures::Sink<String, Error = std::io::Error> + Send + 'static,
    impl futures::Stream<Item = std::io::Result<String>> + Send + 'static,
> {
    let record = move |dir: Direction, line: &str| {
        if let Some(recorder) = &recorder {
            recorder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record_line(dir, line);
        }
    };
    let record_in = record.clone();
    let incoming = futures::io::BufReader::new(stdout.compat())
        .lines()
        .inspect(move |line| {
            if let Ok(line) = line {
                record_in(Direction::In, line);
            }
        });
    let outgoing = futures::sink::unfold(
        Box::pin(stdin.compat_write()),
        move |mut writer, line: String| {
            record(Direction::Out, &line);
            async move {
                writer.write_all(format!("{line}\n").as_bytes()).await?;
                writer.flush().await?;
                Ok::<_, std::io::Error>(writer)
            }
        },
    );
    Lines::new(outgoing, Box::pin(incoming))
}

/// The stdio MCP server entry: `cru mcp` beside the running binary. Every
/// ACP agent must take a stdio server.
pub(super) fn stdio_mcp_server() -> McpServer {
    let cru = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("cru")))
        .unwrap_or_else(|| PathBuf::from("cru"));
    McpServer::Stdio(McpServerStdio::new("crucible", cru).args(vec![
        "mcp".to_string(),
        "--stdio".to_string(),
        "--standalone".to_string(),
    ]))
}
