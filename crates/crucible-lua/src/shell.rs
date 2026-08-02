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

/// Sink for streamed output lines, called as `(stream, line)`.
///
/// The `Send` bound tracks the crate's `send` feature, mirroring `mlua`'s own
/// `MaybeSend`. With the feature on (how the daemon builds it) `mlua` requires
/// async-function futures to be `Send` and makes its own handles `Send` to
/// match; without it, neither holds — and a `Send` bound here would then be
/// unsatisfiable by the very Lua callback this exists to carry.
#[cfg(feature = "send")]
pub type LineSink<'a> = &'a mut (dyn FnMut(&str, &str) + Send);

/// See [`LineSink`].
#[cfg(not(feature = "send"))]
pub type LineSink<'a> = &'a mut dyn FnMut(&str, &str);

/// Run a command, delivering each output line as it arrives.
///
/// `exec_command` buffers everything and returns at completion, so a long
/// build reports nothing until it is over. That is not a status-API problem —
/// there is genuinely nothing to report until the process exits — which makes
/// streaming the prerequisite for any progress reporting over shell work.
///
/// `on_line` is called with `("stdout" | "stderr", line)` as lines arrive,
/// interleaved in real time. The returned [`ExecResult`] still carries the
/// complete output, so a caller that only wants the whole thing does not need
/// a second API.
///
/// The callback runs inline on the reader task rather than being an async fn,
/// so lines are delivered in order and a slow callback applies backpressure
/// instead of queueing without bound. See [`LineSink`] for its `Send` bound.
pub async fn spawn_command(
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
    policy: &ShellPolicy,
    on_line: LineSink<'_>,
) -> Result<ExecResult, LuaError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    if !policy.is_allowed(cmd) {
        return Err(LuaError::Runtime(format!(
            "Command '{}' is not allowed by shell policy",
            cmd
        )));
    }

    debug!("Streaming: {} {:?}", cmd, args);

    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    } else if let Some(default) = &policy.default_cwd {
        command.current_dir(default);
    }
    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            command.env(key, value);
        }
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut out_reader = stdout.map(|s| BufReader::new(s).lines());
    let mut err_reader = stderr.map(|s| BufReader::new(s).lines());

    let mut stdout_buf = String::new();
    let mut stderr_buf = String::new();

    let timeout = std::time::Duration::from_secs(policy.timeout_secs);
    let pump = async {
        // Both streams are drained concurrently so a chatty stderr cannot
        // block stdout (or the reverse) — sequential draining deadlocks the
        // moment the other pipe's buffer fills.
        loop {
            let out_next = async {
                match out_reader.as_mut() {
                    Some(r) => r.next_line().await,
                    None => Ok(None),
                }
            };
            let err_next = async {
                match err_reader.as_mut() {
                    Some(r) => r.next_line().await,
                    None => Ok(None),
                }
            };
            tokio::select! {
                line = out_next => match line {
                    Ok(Some(line)) => {
                        on_line("stdout", &line);
                        stdout_buf.push_str(&line);
                        stdout_buf.push('\n');
                    }
                    Ok(None) => { out_reader = None; }
                    Err(e) => return Err(LuaError::Runtime(format!("stdout read failed: {e}"))),
                },
                line = err_next => match line {
                    Ok(Some(line)) => {
                        on_line("stderr", &line);
                        stderr_buf.push_str(&line);
                        stderr_buf.push('\n');
                    }
                    Ok(None) => { err_reader = None; }
                    Err(e) => return Err(LuaError::Runtime(format!("stderr read failed: {e}"))),
                },
            }
            if out_reader.is_none() && err_reader.is_none() {
                return Ok(());
            }
        }
    };

    tokio::time::timeout(timeout, pump).await.map_err(|_| {
        LuaError::Runtime(format!(
            "Command '{}' timed out after {} seconds",
            cmd, policy.timeout_secs
        ))
    })??;

    let status = child
        .wait()
        .await
        .map_err(|e| LuaError::Runtime(format!("Failed to await '{}': {}", cmd, e)))?;

    Ok(ExecResult {
        success: status.success(),
        exit_code: status.code().unwrap_or(-1),
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}

/// Execute a shell command (async)
pub async fn exec_command(
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
    stdin_data: Option<&str>,
    policy: &ShellPolicy,
) -> Result<ExecResult, LuaError> {
    if !policy.is_allowed(cmd) {
        return Err(LuaError::Runtime(format!(
            "Command '{}' is not allowed by shell policy",
            cmd
        )));
    }

    debug!("Executing: {} {:?}", cmd, args);

    let mut command = Command::new(cmd);
    command.args(args);

    if let Some(dir) = cwd {
        command.current_dir(dir);
    } else if let Some(default) = &policy.default_cwd {
        command.current_dir(default);
    }

    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            command.env(key, value);
        }
    }

    command.stdout(Stdio::piped());
    if policy.capture_stderr {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::inherit());
    }

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    let timeout = std::time::Duration::from_secs(policy.timeout_secs);

    // If stdin data is provided, spawn the process and pipe it
    let output = if let Some(data) = stdin_data {
        let mut child = command
            .spawn()
            .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(data.as_bytes())
                .await
                .map_err(|e| LuaError::Runtime(format!("stdin write failed: {}", e)))?;
            drop(stdin);
        }

        tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                LuaError::Runtime(format!(
                    "Command '{}' timed out after {} seconds",
                    cmd, policy.timeout_secs
                ))
            })?
            .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?
    } else {
        tokio::time::timeout(timeout, command.output())
            .await
            .map_err(|_| {
                LuaError::Runtime(format!(
                    "Command '{}' timed out after {} seconds",
                    cmd, policy.timeout_secs
                ))
            })?
            .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?
    };

    Ok(ExecResult {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
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
                let mut cwd = None;
                let mut env = None;
                let mut stdin_data = None;

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
                    if let Ok(data) = opts.get::<String>("stdin") {
                        stdin_data = Some(data);
                    }
                }

                let result = exec_command(
                    &cmd,
                    &args,
                    cwd.as_deref(),
                    env.as_ref(),
                    stdin_data.as_deref(),
                    &policy,
                )
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

    // shell.spawn(cmd, args, options) -> result table
    //
    // Same result shape as `exec`, plus `options.on_line(stream, line)` called
    // as output arrives. A plugin building an image can report progress
    // instead of going silent for minutes.
    let policy_clone = policy.clone();
    let spawn_fn = lua.create_async_function(
        move |lua, (cmd, args, options): (String, Vec<String>, Option<Table>)| {
            let policy = policy_clone.clone();
            async move {
                let mut cwd = None;
                let mut env = None;
                let mut on_line: Option<mlua::Function> = None;

                if let Some(opts) = &options {
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
                    if let Ok(f) = opts.get::<mlua::Function>("on_line") {
                        on_line = Some(f);
                    }
                }

                // A callback that raises must not be swallowed: the plugin
                // asked to see every line, and silently dropping the error
                // would leave it believing it did.
                //
                // Held as a String rather than the `mlua::Error`: this crate
                // also builds without the `send` feature, where that type is
                // neither Send nor Sync and would make the whole future
                // non-Send. The message is what a plugin author reads anyway.
                let mut callback_error: Option<String> = None;
                let result = {
                    let mut sink = |stream: &str, line: &str| {
                        if callback_error.is_some() {
                            return;
                        }
                        if let Some(f) = &on_line {
                            if let Err(e) = f.call::<()>((stream, line)) {
                                callback_error = Some(e.to_string());
                            }
                        }
                    };
                    spawn_command(
                        &cmd,
                        &args,
                        cwd.as_deref(),
                        env.as_ref(),
                        &policy,
                        &mut sink,
                    )
                    .await
                    .map_err(mlua::Error::external)?
                };
                if let Some(e) = callback_error {
                    return Err(mlua::Error::runtime(e));
                }

                let result_table = lua.create_table()?;
                result_table.set("success", result.success)?;
                result_table.set("exit_code", result.exit_code)?;
                result_table.set("stdout", result.stdout)?;
                result_table.set("stderr", result.stderr)?;
                Ok(result_table)
            }
        },
    )?;
    shell.set("spawn", spawn_fn)?;

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

    /// Streaming exists so long-running commands can report progress. `exec`
    /// buffers everything and returns at completion, so a five-minute image
    /// build emits nothing until it is over — no status API can fix that from
    /// the outside, which is why this is the prerequisite for progress
    /// reporting rather than a nicety.
    #[tokio::test]
    async fn spawn_streams_lines_as_they_arrive_and_still_returns_the_whole_output() {
        let policy = ShellPolicy {
            blocked_commands: vec![],
            timeout_secs: 30,
            ..Default::default()
        };
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let sink = seen.clone();

        let result = spawn_command(
            "sh",
            &[
                "-c".to_string(),
                "echo one; echo two; echo err >&2".to_string(),
            ],
            None,
            None,
            &policy,
            &mut |stream: &str, line: &str| {
                sink.lock()
                    .unwrap()
                    .push((stream.to_string(), line.to_string()));
            },
        )
        .await
        .expect("spawn");

        let seen = seen.lock().unwrap().clone();
        let stdout_lines: Vec<_> = seen
            .iter()
            .filter(|(s, _)| s == "stdout")
            .map(|(_, l)| l.clone())
            .collect();
        assert_eq!(
            stdout_lines,
            vec!["one", "two"],
            "lines arrive individually"
        );
        assert!(
            seen.iter().any(|(s, l)| s == "stderr" && l == "err"),
            "stderr is streamed too and labelled: {seen:?}"
        );

        // ...and the buffered result still matches `exec`'s shape, so a caller
        // that only wants the whole output does not need a second API.
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("one") && result.stdout.contains("two"));
        assert!(result.stderr.contains("err"));
    }

    #[tokio::test]
    async fn spawn_reports_a_failing_command_without_losing_its_output() {
        let policy = ShellPolicy {
            blocked_commands: vec![],
            timeout_secs: 30,
            ..Default::default()
        };
        let result = spawn_command(
            "sh",
            &["-c".to_string(), "echo partial; exit 3".to_string()],
            None,
            None,
            &policy,
            &mut |_, _| {},
        )
        .await
        .expect("spawn");

        assert!(!result.success);
        assert_eq!(result.exit_code, 3);
        assert!(
            result.stdout.contains("partial"),
            "output produced before the failure must survive it"
        );
    }

    /// The binding has to actually be registered, and a callback that raises
    /// must surface rather than being swallowed — a plugin that asked to see
    /// every line would otherwise believe it had.
    #[tokio::test]
    async fn lua_spawn_is_registered_and_propagates_a_failing_callback() {
        let lua = Lua::new();
        register_shell_module(
            &lua,
            ShellPolicy {
                blocked_commands: vec![],
                timeout_secs: 30,
                ..Default::default()
            },
        )
        .expect("register");

        let shell: Table = lua
            .globals()
            .get::<Table>("cru")
            .expect("cru")
            .get("shell")
            .expect("cru.shell");
        assert!(
            shell.contains_key("spawn").unwrap(),
            "cru.shell.spawn missing"
        );

        let err = lua
            .load(
                r#"
                local lines = {}
                local r = cru.shell.spawn("sh", {"-c", "echo a; echo b"}, {
                  on_line = function(stream, line) lines[#lines+1] = stream .. ":" .. line end,
                })
                assert(r.success, "command should succeed")
                assert(#lines == 2, "expected 2 lines, got " .. #lines)
                assert(lines[1] == "stdout:a", "got " .. lines[1])

                cru.shell.spawn("sh", {"-c", "echo x"}, {
                  on_line = function() error("callback blew up") end,
                })
                "#,
            )
            .exec_async()
            .await
            .expect_err("a raising callback must not be swallowed");
        assert!(err.to_string().contains("callback blew up"), "{err}");
    }

    #[tokio::test]
    async fn spawn_refuses_a_command_the_policy_blocks() {
        let policy = ShellPolicy::default();
        let err = spawn_command("rm", &[], None, None, &policy, &mut |_, _| {})
            .await
            .expect_err("the policy must gate streaming exactly as it gates exec");
        assert!(err.to_string().contains("not allowed"), "{err}");
    }

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
        let result = exec_command("echo", &["hello".to_string()], None, None, None, &policy)
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
            None,
            &policy,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_exec_with_stdin() {
        let policy = ShellPolicy::permissive();
        let result = exec_command("cat", &[], None, None, Some("hello world"), &policy)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_exec_with_stdin_multiline() {
        let policy = ShellPolicy::permissive();
        let content = "line1\nline2\nline3";
        let result = exec_command("cat", &[], None, None, Some(content), &policy)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout, content);
    }

    #[tokio::test]
    async fn test_exec_without_stdin_does_not_hang() {
        let policy = ShellPolicy::permissive();
        let result = exec_command("echo", &["no-stdin".to_string()], None, None, None, &policy)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "no-stdin");
    }
}
