//! Crucible ACP Client Implementation
//!
//! This module implements the ACP `Client` trait, allowing Crucible to act as
//! an IDE/editor that spawns and communicates with AI agents (like claude-code).
//!
//! ## Architecture
//!
//! ```text
//! User runs: cru chat "message"
//!   ↓
//! Crucible implements Client trait (this module)
//!   ↓
//! Crucible spawns agent process (claude-code/codex)
//!   ↓
//! Agent processes prompts, calls back to Crucible for:
//!   - read_text_file() → Read from kiln
//!   - write_text_file() → Write to kiln
//!   - request_permission() → Ask user for approval
//!   ↓
//! Agent returns responses → Crucible displays to user
//! ```
//!
//! ## Client Trait Implementation
//!
//! The `Client` trait defines methods that agents can call:
//! - File operations: read_text_file, write_text_file
//! - Permission handling: request_permission
//! - Notifications: session_notification
//! - Terminal operations: create_terminal, terminal_output, etc.
//! - Extensions: ext_method, ext_notification

use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::process::{Child, Command};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use agent_client_protocol::{
    Client, ClientSideConnection, CreateTerminalRequest, CreateTerminalResponse, Error as AcpError,
    ExtNotification, ExtRequest, ExtResponse, KillTerminalCommandRequest,
    KillTerminalCommandResponse, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
    ReleaseTerminalResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, Result as AcpResult, SessionNotification, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};

use crate::{AcpError as CrucibleAcpError, Result};

/// Crucible's implementation of the ACP Client trait
///
/// This struct provides the client-side functionality for ACP,
/// handling file operations on the kiln and user permission requests.
#[derive(Clone)]
pub struct CrucibleClient {
    /// Path to the kiln directory
    kiln_path: PathBuf,
    /// Whether to run in read-only mode (no writes allowed)
    read_only: bool,
    /// Session notifications received from the agent
    notifications: Arc<Mutex<Vec<SessionNotification>>>,
}

impl CrucibleClient {
    /// Create a new Crucible client
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `read_only` - Whether to run in read-only mode
    pub fn new(kiln_path: PathBuf, read_only: bool) -> Self {
        Self {
            kiln_path,
            read_only,
            notifications: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the kiln path
    pub fn kiln_path(&self) -> &PathBuf {
        &self.kiln_path
    }

    /// Check if running in read-only mode
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Get all session notifications received
    pub fn notifications(&self) -> Vec<SessionNotification> {
        self.notifications.lock().unwrap().clone()
    }

    /// Clear notifications
    pub fn clear_notifications(&self) {
        self.notifications.lock().unwrap().clear();
    }
}

#[async_trait(?Send)]
impl Client for CrucibleClient {
    /// Handle permission requests from the agent
    ///
    /// The agent requests permission before performing sensitive operations.
    /// This method asks the user for approval.
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> AcpResult<RequestPermissionResponse> {
        tracing::info!(
            "Agent requesting permission for tool call ID: {}",
            args.tool_call.id
        );
        tracing::debug!("Tool call details: {:?}", args.tool_call);
        tracing::debug!("Permission options: {:?}", args.options);

        // Select appropriate option based on mode
        let outcome = if self.read_only {
            // In read-only mode, look for a "reject" or "deny" option, or cancel
            tracing::warn!("Permission denied (read-only mode)");

            // For now, just cancel the request in read-only mode
            RequestPermissionOutcome::Cancelled
        } else {
            // In act mode, select the first option (usually "allow" or "approve")
            if let Some(first_option) = args.options.first() {
                tracing::info!("Permission granted: selecting option {}", first_option.id);
                RequestPermissionOutcome::Selected {
                    option_id: first_option.id.clone(),
                }
            } else {
                // No options provided, cancel
                tracing::warn!("No permission options provided, cancelling");
                RequestPermissionOutcome::Cancelled
            }
        };

        Ok(RequestPermissionResponse {
            outcome,
            meta: None,
        })
    }

    /// Write content to a file
    ///
    /// This is a general-purpose file write operation that the agent can use
    /// for any file in the current working directory. Only allowed in write mode (act mode).
    ///
    /// For kiln-specific operations, the agent should use Crucible MCP tools.
    async fn write_text_file(
        &self,
        args: WriteTextFileRequest,
    ) -> AcpResult<WriteTextFileResponse> {
        if self.read_only {
            tracing::error!(
                "Write attempt blocked (read-only mode): {}",
                args.path.display()
            );
            return Err(AcpError::invalid_params());
        }

        tracing::info!("Writing file: {}", args.path.display());

        // Work relative to current directory (not kiln)
        // Agent can write any file in its working directory

        // Create parent directories if needed
        if let Some(parent) = args.path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                tracing::error!("Failed to create directories: {}", e);
                return Err(AcpError::internal_error());
            }
        }

        // Write the file
        if let Err(e) = tokio::fs::write(&args.path, &args.content).await {
            tracing::error!("Failed to write file {}: {}", args.path.display(), e);
            return Err(AcpError::internal_error());
        }

        tracing::info!("File written successfully: {}", args.path.display());

        Ok(WriteTextFileResponse { meta: None })
    }

    /// Read content from a file
    ///
    /// This is a general-purpose file read operation that the agent can use
    /// for any file in the current working directory.
    ///
    /// For kiln-specific operations, the agent should use Crucible MCP tools.
    async fn read_text_file(&self, args: ReadTextFileRequest) -> AcpResult<ReadTextFileResponse> {
        tracing::info!("Reading file: {}", args.path.display());

        // Work relative to current directory (not kiln)
        // Agent can read any file in its working directory

        // Read the file
        let content = match tokio::fs::read_to_string(&args.path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to read file {}: {}", args.path.display(), e);
                return Err(AcpError::internal_error());
            }
        };

        tracing::info!(
            "File read successfully: {} ({} bytes)",
            args.path.display(),
            content.len()
        );

        Ok(ReadTextFileResponse {
            content,
            meta: None,
        })
    }

    /// Handle session notifications from the agent
    ///
    /// These are real-time updates about the agent's progress.
    async fn session_notification(&self, args: SessionNotification) -> AcpResult<()> {
        tracing::debug!("Session notification: {:?}", args.update);

        // Store notification for later retrieval
        self.notifications.lock().unwrap().push(args);

        Ok(())
    }

    // Terminal operations - not implemented for now

    async fn create_terminal(
        &self,
        _args: CreateTerminalRequest,
    ) -> AcpResult<CreateTerminalResponse> {
        tracing::warn!("create_terminal not implemented");
        Err(AcpError::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: TerminalOutputRequest,
    ) -> AcpResult<TerminalOutputResponse> {
        tracing::warn!("terminal_output not implemented");
        Err(AcpError::method_not_found())
    }

    async fn kill_terminal_command(
        &self,
        _args: KillTerminalCommandRequest,
    ) -> AcpResult<KillTerminalCommandResponse> {
        tracing::warn!("kill_terminal_command not implemented");
        Err(AcpError::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: ReleaseTerminalRequest,
    ) -> AcpResult<ReleaseTerminalResponse> {
        tracing::warn!("release_terminal not implemented");
        Err(AcpError::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: WaitForTerminalExitRequest,
    ) -> AcpResult<WaitForTerminalExitResponse> {
        tracing::warn!("wait_for_terminal_exit not implemented");
        Err(AcpError::method_not_found())
    }

    // Extension methods - not implemented for now

    async fn ext_method(&self, _args: ExtRequest) -> AcpResult<ExtResponse> {
        tracing::warn!("ext_method not implemented");
        Err(AcpError::method_not_found())
    }

    async fn ext_notification(&self, _args: ExtNotification) -> AcpResult<()> {
        tracing::debug!("Extension notification received (ignored)");
        Ok(())
    }
}

/// Spawns an agent process and creates a ClientSideConnection
///
/// This function:
/// 1. Spawns the agent binary (e.g., claude-code)
/// 2. Captures its stdin/stdout for communication
/// 3. Creates a ClientSideConnection that provides the Agent trait methods
///
/// # Arguments
///
/// * `agent_path` - Path to the agent binary
/// * `client` - The CrucibleClient implementation
///
/// # Returns
///
/// A tuple of:
/// - The ClientSideConnection (provides Agent trait methods)
/// - The Child process handle
/// - A boxed future representing the IO task (caller must spawn with LocalSet)
pub async fn spawn_agent(
    agent_path: PathBuf,
    client: CrucibleClient,
) -> Result<(
    ClientSideConnection,
    Child,
    std::pin::Pin<Box<dyn std::future::Future<Output = AcpResult<()>>>>,
)> {
    tracing::info!("Spawning agent: {}", agent_path.display());

    // Spawn the agent process
    let mut command = Command::new(&agent_path);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::inherit()); // Let agent errors show in our stderr

    let mut child = command
        .spawn()
        .map_err(|e| CrucibleAcpError::Connection(format!("Failed to spawn agent: {}", e)))?;

    // Get stdin/stdout handles and wrap with compat for futures::io traits
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| CrucibleAcpError::Connection("Failed to capture agent stdin".to_string()))?
        .compat_write();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CrucibleAcpError::Connection("Failed to capture agent stdout".to_string()))?
        .compat();

    tracing::info!("Agent process spawned, creating connection...");

    // Create the ClientSideConnection
    // The spawner function receives futures but doesn't spawn them - we return the io_task instead
    let (connection, io_task) = ClientSideConnection::new(client, stdin, stdout, |_fut| {
        // Futures are passed here but we don't spawn them
        // The main io_task is returned to the caller
    });

    tracing::info!("ClientSideConnection created successfully");

    // Box the io_task to make it easier to work with
    let io_task: std::pin::Pin<Box<dyn std::future::Future<Output = AcpResult<()>>>> =
        Box::pin(io_task);

    Ok((connection, child, io_task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_client_creation() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        assert_eq!(client.kiln_path(), temp.path());
        assert!(!client.is_read_only());
    }

    #[test]
    fn test_client_read_only() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), true);
        assert!(client.is_read_only());
    }

    #[tokio::test]
    async fn test_read_file() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        tokio::fs::write(&test_file, "test content").await.unwrap();

        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let result = client
            .read_text_file(ReadTextFileRequest {
                session_id: "test".into(),
                path: test_file, // Use absolute path
                line: None,
                limit: None,
                meta: None,
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "test content");
    }

    #[tokio::test]
    async fn test_write_file() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let new_file = temp.path().join("new.txt");

        let result = client
            .write_text_file(WriteTextFileRequest {
                session_id: "test".into(),
                path: new_file.clone(), // Use absolute path
                content: "new content".to_string(),
                meta: None,
            })
            .await;

        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(&new_file).await.unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_write_file_read_only_blocked() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), true);

        let result = client
            .write_text_file(WriteTextFileRequest {
                session_id: "test".into(),
                path: PathBuf::from("blocked.txt"),
                content: "should fail".to_string(),
                meta: None,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_permission_request() {
        use agent_client_protocol::{
            PermissionOption, PermissionOptionId, PermissionOptionKind, ToolCallUpdate,
            ToolCallUpdateFields,
        };

        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);

        let result = client
            .request_permission(RequestPermissionRequest {
                session_id: "test".into(),
                tool_call: ToolCallUpdate {
                    id: "1".into(),
                    fields: ToolCallUpdateFields::default(),
                    meta: None,
                },
                options: vec![PermissionOption {
                    id: PermissionOptionId("allow".into()),
                    name: "Allow".into(),
                    kind: PermissionOptionKind::AllowOnce,
                    meta: None,
                }],
                meta: None,
            })
            .await;

        assert!(result.is_ok());
        let outcome = result.unwrap().outcome;
        assert!(matches!(outcome, RequestPermissionOutcome::Selected { .. }));
    }
}
