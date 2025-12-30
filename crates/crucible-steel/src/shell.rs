//! Shell execution module for Steel scripts
//!
//! Provides safe command execution with policy enforcement.
//!
//! ## Steel Usage
//!
//! ```scheme
//! ;; Execute a command with arguments
//! (shell-exec "cargo" '("build" "--release"))
//!
//! ;; With options (cwd, env)
//! (shell-exec "cargo" '("build") (hash 'cwd "/project" 'env (hash 'RUST_LOG "debug")))
//!
//! ;; Check result
//! (let ([result (shell-exec "echo" '("hello"))])
//!   (if (hash-ref result 'success)
//!       (hash-ref result 'stdout)
//!       (error (hash-ref result 'stderr))))
//!
//! ;; Find command in PATH
//! (shell-which "cargo")  ; => "/home/user/.cargo/bin/cargo" or #f
//! ```

use crate::error::SteelError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::debug;

/// Shell execution policy
#[derive(Debug, Clone)]
pub struct ShellPolicy {
    /// Allowed commands (empty = allow all)
    pub allowed_commands: Vec<String>,
    /// Blocked commands (checked first)
    pub blocked_commands: Vec<String>,
    /// Default working directory
    pub default_cwd: Option<PathBuf>,
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    /// Whether to capture stderr
    pub capture_stderr: bool,
}

impl Default for ShellPolicy {
    fn default() -> Self {
        Self {
            allowed_commands: Vec::new(),
            blocked_commands: vec![
                "rm".to_string(),
                "sudo".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
            ],
            default_cwd: None,
            timeout_secs: 30,
            capture_stderr: true,
        }
    }
}

impl ShellPolicy {
    /// Create a permissive policy (for trusted scripts)
    pub fn permissive() -> Self {
        Self {
            allowed_commands: Vec::new(),
            blocked_commands: Vec::new(),
            default_cwd: None,
            timeout_secs: 300,
            capture_stderr: true,
        }
    }

    /// Check if a command is allowed
    pub fn is_allowed(&self, cmd: &str) -> bool {
        // Check blocked list first
        if self
            .blocked_commands
            .iter()
            .any(|b| cmd == b || cmd.ends_with(&format!("/{}", b)))
        {
            return false;
        }

        // If allowed list is empty, allow all (except blocked)
        if self.allowed_commands.is_empty() {
            return true;
        }

        // Check allowed list
        self.allowed_commands
            .iter()
            .any(|a| cmd == a || cmd.ends_with(&format!("/{}", a)))
    }
}

/// Result of shell command execution
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ExecResult {
    /// Convert to JSON for Steel consumption
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "success": self.success,
            "exit_code": self.exit_code,
            "stdout": self.stdout,
            "stderr": self.stderr
        })
    }
}

/// Execute a shell command (async)
pub async fn exec_command(
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
    policy: &ShellPolicy,
) -> Result<ExecResult, SteelError> {
    // Check policy
    if !policy.is_allowed(cmd) {
        return Err(SteelError::PolicyViolation(format!(
            "Command '{}' is not allowed by shell policy",
            cmd
        )));
    }

    debug!("Executing: {} {:?}", cmd, args);

    let mut command = Command::new(cmd);
    command.args(args);

    // Set working directory
    if let Some(dir) = cwd {
        command.current_dir(dir);
    } else if let Some(default) = &policy.default_cwd {
        command.current_dir(default);
    }

    // Set environment variables
    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            command.env(key, value);
        }
    }

    // Configure I/O
    command.stdout(Stdio::piped());
    if policy.capture_stderr {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::inherit());
    }

    // Execute with timeout
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(policy.timeout_secs),
        command.output(),
    )
    .await
    .map_err(|_| {
        SteelError::Execution(format!(
            "Command '{}' timed out after {} seconds",
            cmd, policy.timeout_secs
        ))
    })?
    .map_err(|e| SteelError::Execution(format!("Failed to execute '{}': {}", cmd, e)))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ExecResult {
        success: output.status.success(),
        exit_code,
        stdout,
        stderr,
    })
}

/// Find a command in PATH
pub fn which(cmd: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path.split(sep) {
            let full_path = PathBuf::from(dir).join(cmd);
            if full_path.exists() {
                return Some(full_path);
            }
            // Check with .exe on Windows
            #[cfg(windows)]
            {
                let exe_path = full_path.with_extension("exe");
                if exe_path.exists() {
                    return Some(exe_path);
                }
            }
        }
    }
    None
}

/// Shell module for Steel scripts
///
/// Provides `shell-exec` and `shell-which` functions.
pub struct ShellModule {
    policy: ShellPolicy,
}

impl ShellModule {
    /// Create a new shell module with the given policy
    pub fn new(policy: ShellPolicy) -> Self {
        Self { policy }
    }

    /// Create with default policy
    pub fn with_default_policy() -> Self {
        Self::new(ShellPolicy::default())
    }

    /// Create with permissive policy
    pub fn with_permissive_policy() -> Self {
        Self::new(ShellPolicy::permissive())
    }

    /// Get the policy (for modification)
    pub fn policy(&self) -> &ShellPolicy {
        &self.policy
    }

    /// Get mutable policy
    pub fn policy_mut(&mut self) -> &mut ShellPolicy {
        &mut self.policy
    }

    /// Execute a command
    pub async fn exec(
        &self,
        cmd: &str,
        args: Vec<String>,
        cwd: Option<String>,
        env: Option<HashMap<String, String>>,
    ) -> Result<ExecResult, SteelError> {
        exec_command(cmd, &args, cwd.as_deref(), env.as_ref(), &self.policy).await
    }

    /// Find command in PATH
    pub fn which(&self, cmd: &str) -> Option<PathBuf> {
        which(cmd)
    }

    /// Generate Steel code for shell-exec and shell-which stub functions
    pub fn steel_stubs() -> &'static str {
        r#"
;; Shell execution functions (stubs - replaced by Rust)
;; These provide shell access with policy enforcement.

(define (shell-exec cmd args . opts)
  (error "shell-exec not available: no shell module registered"))

(define (shell-which cmd)
  (error "shell-which not available: no shell module registered"))
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_default_blocked() {
        let policy = ShellPolicy::default();
        assert!(!policy.is_allowed("rm"));
        assert!(!policy.is_allowed("sudo"));
        assert!(policy.is_allowed("echo"));
        assert!(policy.is_allowed("cargo"));
    }

    #[test]
    fn test_policy_permissive() {
        let policy = ShellPolicy::permissive();
        assert!(policy.is_allowed("rm"));
        assert!(policy.is_allowed("sudo"));
        assert!(policy.is_allowed("anything"));
    }

    #[test]
    fn test_policy_allowed_list() {
        let policy = ShellPolicy {
            allowed_commands: vec!["echo".to_string(), "cat".to_string()],
            blocked_commands: Vec::new(),
            ..Default::default()
        };
        assert!(policy.is_allowed("echo"));
        assert!(policy.is_allowed("cat"));
        assert!(!policy.is_allowed("rm"));
        assert!(!policy.is_allowed("ls"));
    }

    #[test]
    fn test_policy_blocks_full_path() {
        let policy = ShellPolicy::default();
        assert!(!policy.is_allowed("/bin/rm"));
        assert!(!policy.is_allowed("/usr/bin/sudo"));
    }

    #[tokio::test]
    async fn test_exec_echo() {
        let policy = ShellPolicy::permissive();
        let result = exec_command("echo", &["hello".to_string()], None, None, &policy)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_exec_blocked_command() {
        let policy = ShellPolicy::default();
        let result = exec_command(
            "rm",
            &["-rf".to_string(), "/".to_string()],
            None,
            None,
            &policy,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_exec_with_env() {
        let policy = ShellPolicy::permissive();
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "test_value".to_string());

        let result = exec_command(
            "sh",
            &["-c".to_string(), "echo $MY_VAR".to_string()],
            None,
            Some(&env),
            &policy,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_exec_result_to_json() {
        let result = ExecResult {
            success: true,
            exit_code: 0,
            stdout: "hello\n".to_string(),
            stderr: "".to_string(),
        };

        let json = result.to_json();
        assert_eq!(json["success"], true);
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["stdout"], "hello\n");
        assert_eq!(json["stderr"], "");
    }

    #[test]
    fn test_which_finds_echo() {
        // echo should exist on all Unix systems
        let result = which("echo");
        assert!(result.is_some(), "echo should be found in PATH");
    }

    #[test]
    fn test_which_not_found() {
        let result = which("nonexistent_command_12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_shell_module_creation() {
        let module = ShellModule::with_default_policy();
        assert!(!module.policy().is_allowed("rm"));

        let permissive = ShellModule::with_permissive_policy();
        assert!(permissive.policy().is_allowed("rm"));
    }
}
