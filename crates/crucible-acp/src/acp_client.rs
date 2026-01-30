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
    RequestPermissionResponse, Result as AcpResult, SelectedPermissionOutcome, SessionNotification,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};

use crucible_core::interaction::PermRequest;
use crucible_core::traits::PermissionGate;

use crate::{ClientError, Result};

/// Information about a file write operation for diff display
#[derive(Debug, Clone)]
pub struct WriteInfo {
    /// Path to the file that was written
    pub path: PathBuf,
    /// Content before the write (None if file didn't exist)
    pub old_content: Option<String>,
    /// New content written to the file
    pub new_content: String,
}

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
    /// Information about the last write operation (for diff display)
    last_write: Arc<Mutex<Option<WriteInfo>>>,
    /// Optional permission gate for routing permission decisions
    permission_gate: Option<Arc<dyn PermissionGate>>,
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
            last_write: Arc::new(Mutex::new(None)),
            permission_gate: None,
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

    /// Get information about the last write operation (for diff display)
    ///
    /// Returns None if no write has occurred yet in this session.
    pub fn last_write_info(&self) -> Option<WriteInfo> {
        self.last_write.lock().unwrap().clone()
    }

    /// Clear the last write info
    pub fn clear_last_write(&self) {
        *self.last_write.lock().unwrap() = None;
    }

    /// Set a permission gate for routing permission decisions.
    pub fn with_permission_gate(mut self, gate: Arc<dyn PermissionGate>) -> Self {
        self.permission_gate = Some(gate);
        self
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
            args.tool_call.tool_call_id
        );
        tracing::debug!("Tool call details: {:?}", args.tool_call);
        tracing::debug!("Permission options: {:?}", args.options);

        let outcome = if self.read_only {
            tracing::warn!("Permission denied (read-only mode)");
            RequestPermissionOutcome::Cancelled
        } else if let Some(ref gate) = self.permission_gate {
            let perm_request = PermRequest::tool(
                args.tool_call.tool_call_id.to_string(),
                serde_json::to_value(&args.tool_call).unwrap_or_default(),
            );
            let response = gate.request_permission(perm_request).await;
            if response.allowed {
                if let Some(first_option) = args.options.first() {
                    tracing::info!("Permission granted via gate: {}", first_option.option_id);
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        first_option.option_id.clone(),
                    ))
                } else {
                    RequestPermissionOutcome::Cancelled
                }
            } else {
                tracing::info!("Permission denied via gate: {:?}", response.reason);
                RequestPermissionOutcome::Cancelled
            }
        } else if let Some(first_option) = args.options.first() {
            tracing::info!(
                "Permission granted (auto-allow): {}",
                first_option.option_id
            );
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                first_option.option_id.clone(),
            ))
        } else {
            tracing::warn!("No permission options provided, cancelling");
            RequestPermissionOutcome::Cancelled
        };

        Ok(RequestPermissionResponse::new(outcome))
    }

    /// Write content to a file
    ///
    /// This is a general-purpose file write operation that the agent can use
    /// for any file in the current working directory. Only allowed in write mode (act mode).
    ///
    /// For kiln-specific operations, the agent should use Crucible MCP tools.
    ///
    /// After a successful write, call `last_write_info()` to get the old/new content for diff display.
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

        // Capture old content before write (for diff display)
        let old_content = tokio::fs::read_to_string(&args.path).await.ok();

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

        // Store write info for diff display
        *self.last_write.lock().unwrap() = Some(WriteInfo {
            path: args.path.clone(),
            old_content,
            new_content: args.content.clone(),
        });

        tracing::info!("File written successfully: {}", args.path.display());

        Ok(WriteTextFileResponse::new())
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

        Ok(ReadTextFileResponse::new(content))
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
        .map_err(|e| ClientError::Connection(format!("Failed to spawn agent: {}", e)))?;

    // Get stdin/stdout handles and wrap with compat for futures::io traits
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| ClientError::Connection("Failed to capture agent stdin".to_string()))?
        .compat_write();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ClientError::Connection("Failed to capture agent stdout".to_string()))?
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
    use agent_client_protocol::SessionId;
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
            .read_text_file(
                ReadTextFileRequest::new(SessionId::from("test"), test_file), // Use absolute path
            )
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
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                new_file.clone(),
                "new content".to_string(),
            ))
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
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                PathBuf::from("blocked.txt"),
                "should fail".to_string(),
            ))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_permission_request() {
        use agent_client_protocol::{
            PermissionOption, PermissionOptionKind, ToolCallId, ToolCallUpdate,
            ToolCallUpdateFields,
        };

        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);

        let tool_call = ToolCallUpdate::new(ToolCallId::from("1"), ToolCallUpdateFields::default());
        let option = PermissionOption::new(
            "allow",
            "Allow".to_string(),
            PermissionOptionKind::AllowOnce,
        );

        let result = client
            .request_permission(RequestPermissionRequest::new(
                SessionId::from("test"),
                tool_call,
                vec![option],
            ))
            .await;

        assert!(result.is_ok());
        let outcome = result.unwrap().outcome;
        assert!(matches!(outcome, RequestPermissionOutcome::Selected(_)));
    }

    // === WriteInfo tests ===

    #[test]
    fn test_last_write_info_initially_none() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        assert!(client.last_write_info().is_none());
    }

    #[tokio::test]
    async fn test_write_captures_write_info_for_new_file() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let new_file = temp.path().join("new.txt");

        let _ = client
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                new_file.clone(),
                "new content".to_string(),
            ))
            .await;

        let info = client.last_write_info().expect("Should have write info");
        assert_eq!(info.path, new_file);
        assert!(info.old_content.is_none()); // New file, no old content
        assert_eq!(info.new_content, "new content");
    }

    #[tokio::test]
    async fn test_write_captures_old_content_for_existing_file() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let existing_file = temp.path().join("existing.txt");

        // Create file with initial content
        tokio::fs::write(&existing_file, "original content")
            .await
            .unwrap();

        // Write new content
        let _ = client
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                existing_file.clone(),
                "updated content".to_string(),
            ))
            .await;

        let info = client.last_write_info().expect("Should have write info");
        assert_eq!(info.path, existing_file);
        assert_eq!(info.old_content, Some("original content".to_string()));
        assert_eq!(info.new_content, "updated content");
    }

    #[tokio::test]
    async fn test_clear_last_write() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let new_file = temp.path().join("test.txt");

        // Write a file
        let _ = client
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                new_file,
                "content".to_string(),
            ))
            .await;

        // Verify write info exists
        assert!(client.last_write_info().is_some());

        // Clear it
        client.clear_last_write();

        // Should be None now
        assert!(client.last_write_info().is_none());
    }

    #[tokio::test]
    async fn test_write_info_updates_on_subsequent_writes() {
        let temp = TempDir::new().unwrap();
        let client = CrucibleClient::new(temp.path().to_path_buf(), false);
        let file1 = temp.path().join("file1.txt");
        let file2 = temp.path().join("file2.txt");

        // First write
        let _ = client
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                file1.clone(),
                "content1".to_string(),
            ))
            .await;

        let info1 = client.last_write_info().expect("Should have write info");
        assert_eq!(info1.path, file1);

        // Second write should replace first
        let _ = client
            .write_text_file(WriteTextFileRequest::new(
                SessionId::from("test"),
                file2.clone(),
                "content2".to_string(),
            ))
            .await;

        let info2 = client.last_write_info().expect("Should have write info");
        assert_eq!(info2.path, file2);
        assert_eq!(info2.new_content, "content2");
    }

    // === PermissionGate integration tests ===

    mod gate_tests {
        use super::*;
        use agent_client_protocol::{
            PermissionOption, PermissionOptionKind, ToolCallId, ToolCallUpdate,
            ToolCallUpdateFields,
        };
        use crucible_core::interaction::{PermRequest, PermResponse};
        use crucible_core::traits::PermissionGate;

        struct DenyAllGate;

        #[async_trait::async_trait]
        impl PermissionGate for DenyAllGate {
            async fn request_permission(&self, _request: PermRequest) -> PermResponse {
                PermResponse::deny()
            }
        }

        struct AllowAllGate;

        #[async_trait::async_trait]
        impl PermissionGate for AllowAllGate {
            async fn request_permission(&self, _request: PermRequest) -> PermResponse {
                PermResponse::allow()
            }
        }

        fn make_permission_request() -> RequestPermissionRequest {
            let tool_call =
                ToolCallUpdate::new(ToolCallId::from("1"), ToolCallUpdateFields::default());
            let option = PermissionOption::new(
                "allow",
                "Allow".to_string(),
                PermissionOptionKind::AllowOnce,
            );
            RequestPermissionRequest::new(SessionId::from("test"), tool_call, vec![option])
        }

        #[tokio::test]
        async fn gate_deny_cancels_permission() {
            let temp = TempDir::new().unwrap();
            let client = CrucibleClient::new(temp.path().to_path_buf(), false)
                .with_permission_gate(Arc::new(DenyAllGate));

            let result = client.request_permission(make_permission_request()).await;
            assert!(result.is_ok());
            assert!(matches!(
                result.unwrap().outcome,
                RequestPermissionOutcome::Cancelled
            ));
        }

        #[tokio::test]
        async fn gate_allow_selects_first_option() {
            let temp = TempDir::new().unwrap();
            let client = CrucibleClient::new(temp.path().to_path_buf(), false)
                .with_permission_gate(Arc::new(AllowAllGate));

            let result = client.request_permission(make_permission_request()).await;
            assert!(result.is_ok());
            assert!(matches!(
                result.unwrap().outcome,
                RequestPermissionOutcome::Selected(_)
            ));
        }

        #[tokio::test]
        async fn no_gate_auto_allows() {
            let temp = TempDir::new().unwrap();
            let client = CrucibleClient::new(temp.path().to_path_buf(), false);

            let result = client.request_permission(make_permission_request()).await;
            assert!(result.is_ok());
            assert!(matches!(
                result.unwrap().outcome,
                RequestPermissionOutcome::Selected(_)
            ));
        }

        #[tokio::test]
        async fn read_only_overrides_gate() {
            let temp = TempDir::new().unwrap();
            let client = CrucibleClient::new(temp.path().to_path_buf(), true)
                .with_permission_gate(Arc::new(AllowAllGate));

            let result = client.request_permission(make_permission_request()).await;
            assert!(result.is_ok());
            assert!(matches!(
                result.unwrap().outcome,
                RequestPermissionOutcome::Cancelled
            ));
        }
    }
}
