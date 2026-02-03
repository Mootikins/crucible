//! Shell execution module for Lua scripts
//!
//! Provides safe command execution with policy enforcement.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local result = shell.exec("cargo", {"build", "--release"}, {
//!     cwd = "/project",
//!     env = { RUST_LOG = "debug" }
//! })
//!
//! if result.success then
//!     print(result.stdout)
//! else
//!     print("Error: " .. result.stderr)
//! end
//! ```

use crate::error::LuaError;
use mlua::{Lua, Table, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
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

/// Execute a shell command (async)
pub async fn exec_command(
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
    policy: &ShellPolicy,
) -> Result<ExecResult, LuaError> {
    // Check policy
    if !policy.is_allowed(cmd) {
        return Err(LuaError::Runtime(format!(
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
        LuaError::Runtime(format!(
            "Command '{}' timed out after {} seconds",
            cmd, policy.timeout_secs
        ))
    })?
    .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?;

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

/// Register the shell module with a Lua state
pub fn register_shell_module(lua: &Lua, policy: ShellPolicy) -> Result<(), LuaError> {
    let shell = lua.create_table()?;

    // Wrap policy in Arc for sharing with async closures
    let policy = Arc::new(policy);

    // shell.exec(cmd, args, options) -> result table
    let policy_clone = policy.clone();
    let exec_fn = lua.create_async_function(
        move |lua, (cmd, args, options): (String, Vec<String>, Option<Table>)| {
            let policy = policy_clone.clone();
            async move {
                // Parse options
                let mut cwd = None;
                let mut env = None;

                if let Some(opts) = options {
                    if let Ok(dir) = opts.get::<String>("cwd") {
                        cwd = Some(dir);
                    }

                    if let Ok(env_table) = opts.get::<Table>("env") {
                        let mut env_map = HashMap::new();
                        for (k, v) in env_table.pairs::<String, String>().flatten() {
                            env_map.insert(k, v);
                        }
                        env = Some(env_map);
                    }
                }

                // Execute command
                let result = exec_command(&cmd, &args, cwd.as_deref(), env.as_ref(), &policy)
                    .await
                    .map_err(mlua::Error::external)?;

                // Build result table
                let result_table = lua.create_table()?;
                result_table.set("success", result.success)?;
                result_table.set("exit_code", result.exit_code)?;
                result_table.set("stdout", result.stdout)?;
                result_table.set("stderr", result.stderr)?;

                Ok(result_table)
            }
        },
    )?;
    shell.set("exec", exec_fn)?;

    // shell.which(cmd) -> path or nil (simple PATH lookup)
    let which_fn = lua.create_function(|lua, cmd: String| {
        if let Ok(path) = std::env::var("PATH") {
            let sep = if cfg!(windows) { ';' } else { ':' };
            for dir in path.split(sep) {
                let full_path = PathBuf::from(dir).join(&cmd);
                if full_path.exists() {
                    return Ok(Value::String(
                        lua.create_string(full_path.to_string_lossy().as_ref())?,
                    ));
                }
                // Check with .exe on Windows
                #[cfg(windows)]
                {
                    let exe_path = full_path.with_extension("exe");
                    if exe_path.exists() {
                        return Ok(Value::String(
                            lua.create_string(exe_path.to_string_lossy().as_ref())?,
                        ));
                    }
                }
            }
        }
        Ok(Value::Nil)
    })?;
    shell.set("which", which_fn)?;

    // Register shell module globally
    lua.globals().set("shell", shell.clone())?;
    crate::lua_util::register_in_namespaces(lua, "shell", shell)?;

    Ok(())
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
}
