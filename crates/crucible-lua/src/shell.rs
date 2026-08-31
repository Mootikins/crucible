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

/// What `cru.shell` may run from inside a plugin.
///
/// **Not** [`crucible_core::config::ShellPolicy`], and the two must not be
/// merged: their defaults are opposite. This one is **fail-open** — an empty
/// `allowed_commands` allows everything not explicitly blocked, because a
/// plugin runs builds and tools nobody can enumerate ahead of time. The core
/// one is **fail-closed** — an empty whitelist denies everything, because it
/// governs the agent's `bash` tool. Adopting either default on the other side
/// is a behaviour change, not a refactor.
///
/// The matching rules differ too: this one compares a whole command name, or a
/// path ending in `/<name>`; the core one prefix-matches `cmd` joined with its
/// arguments, so `rm -rf` blocks `rm -rf /` but not `rm`.
/// Shell execution policy
#[derive(Debug, Clone)]
pub struct PluginShellPolicy {
    /// Allowed commands (empty = allow all)
    pub allowed_commands: Vec<String>,
    /// Blocked commands (checked first)
    pub blocked_commands: Vec<String>,
    /// Default working directory
    pub default_cwd: Option<PathBuf>,
    /// Maximum execution time in seconds; `None` (the default) imposes no
    /// deadline — plugins run builds and servers whose duration the policy
    /// cannot predict, and a default cap silently killed them.
    pub timeout_secs: Option<u64>,
    /// Whether to capture stderr
    pub capture_stderr: bool,
}

impl Default for PluginShellPolicy {
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
            timeout_secs: None,
            capture_stderr: true,
        }
    }
}

impl PluginShellPolicy {
    /// The deadline for one call: what the caller asked for, but never longer
    /// than the policy allows.
    ///
    /// A plugin may shorten its own deadline and may not lengthen the
    /// sandbox's. `None` on both sides means no deadline, which is the
    /// default — plugins run builds and servers whose duration the policy
    /// cannot predict.
    pub fn deadline_for(&self, requested: Option<u64>) -> Option<u64> {
        match (requested, self.timeout_secs) {
            (Some(asked), Some(cap)) => Some(asked.min(cap)),
            (Some(asked), None) => Some(asked),
            (None, cap) => cap,
        }
    }

    /// Create a permissive policy (for trusted scripts)
    pub fn permissive() -> Self {
        Self {
            allowed_commands: Vec::new(),
            blocked_commands: Vec::new(),
            default_cwd: None,
            timeout_secs: None,
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

/// Check `cmd` against `policy`, then build the command with `args`, `cwd`
/// and `env` applied. The caller sets the stdio pipes.
fn prepare_command(
    policy: &PluginShellPolicy,
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
) -> Result<Command, LuaError> {
    if !policy.is_allowed(cmd) {
        return Err(LuaError::Runtime(format!(
            "Command '{}' is not allowed by shell policy",
            cmd
        )));
    }

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
    Ok(command)
}

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
    policy: &PluginShellPolicy,
    on_line: LineSink<'_>,
    timeout_secs: Option<u64>,
) -> Result<ExecResult, LuaError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut command = prepare_command(policy, cmd, args, cwd, env)?;
    debug!("Streaming: {} {:?}", cmd, args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // The timeout below drops the pump future while the child is still running.
    // Without this the process outlives the call — a `podman build` that
    // overran its timeout would keep building for the daemon's lifetime, with
    // nothing left holding a handle to stop it.
    command.kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut out_reader = stdout.map(|s| BufReader::new(s).lines());
    let mut err_reader = stderr.map(|s| BufReader::new(s).lines());

    let mut stdout_buf = String::new();
    let mut stderr_buf = String::new();

    let pump = async {
        // Both streams are drained concurrently so a chatty stderr cannot
        // block stdout (or the reverse) — sequential draining deadlocks the
        // moment the other pipe's buffer fills.
        loop {
            // Checked FIRST, because the branches below are disabled one by one
            // as the streams end and `select!` panics with every branch disabled.
            if out_reader.is_none() && err_reader.is_none() {
                return Ok(());
            }
            // `if` preconditions, not `None => Ok(None)` arms. An exhausted
            // reader used to leave a branch that completed INSTANTLY on every
            // iteration, so once either stream hit EOF while the other was still
            // open this loop stopped ever returning `Pending` — a hot spin. The
            // timeout below then could not save it: on a current-thread runtime
            // (`#[tokio::test]`, and any single-threaded caller) a task that
            // never yields starves the timer driver that would fire it, so a
            // `timeout_secs = 2` call ran until something killed it, burning a
            // core. `select!` skips a disabled branch without evaluating its
            // future, which is what makes the `expect`s below unreachable.
            tokio::select! {
                line = async { out_reader.as_mut().expect("guarded by is_some").next_line().await },
                    if out_reader.is_some() => match line {
                    Ok(Some(line)) => {
                        on_line("stdout", &line);
                        stdout_buf.push_str(&line);
                        stdout_buf.push('\n');
                    }
                    Ok(None) => { out_reader = None; }
                    Err(e) => return Err(LuaError::Runtime(format!("stdout read failed: {e}"))),
                },
                line = async { err_reader.as_mut().expect("guarded by is_some").next_line().await },
                    if err_reader.is_some() => match line {
                    Ok(Some(line)) => {
                        on_line("stderr", &line);
                        stderr_buf.push_str(&line);
                        stderr_buf.push('\n');
                    }
                    Ok(None) => { err_reader = None; }
                    Err(e) => return Err(LuaError::Runtime(format!("stderr read failed: {e}"))),
                },
            }
        }
    };

    match policy.deadline_for(timeout_secs) {
        Some(secs) => {
            let deadline = std::time::Duration::from_secs(secs);
            match tokio::time::timeout(deadline, pump).await {
                Ok(result) => result?,
                Err(_) => {
                    // `kill_on_drop` alone reaps only when the handle drops,
                    // which is at the end of this scope; killing here stops the
                    // work at the deadline the caller asked for rather than
                    // whenever the error finishes unwinding.
                    let _ = child.start_kill();
                    return Err(LuaError::Runtime(format!(
                        "Command '{}' timed out after {} seconds",
                        cmd, secs
                    )));
                }
            }
        }
        None => pump.await?,
    }

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
    policy: &PluginShellPolicy,
    timeout_secs: Option<u64>,
) -> Result<ExecResult, LuaError> {
    let deadline = policy.deadline_for(timeout_secs);
    let mut command = prepare_command(policy, cmd, args, cwd, env)?;
    debug!("Executing: {} {:?}", cmd, args);

    command.stdout(Stdio::piped());
    if policy.capture_stderr {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::inherit());
    }

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    // Await a child's output, honoring the policy deadline when one is set.
    async fn output_with_deadline<F>(
        fut: F,
        timeout_secs: Option<u64>,
        cmd: &str,
    ) -> Result<std::process::Output, LuaError>
    where
        F: std::future::Future<Output = std::io::Result<std::process::Output>>,
    {
        let io_result = match timeout_secs {
            Some(secs) => tokio::time::timeout(std::time::Duration::from_secs(secs), fut)
                .await
                .map_err(|_| {
                    LuaError::Runtime(format!(
                        "Command '{}' timed out after {} seconds",
                        cmd, secs
                    ))
                })?,
            None => fut.await,
        };
        io_result.map_err(|e| LuaError::Runtime(format!("Failed to execute '{}': {}", cmd, e)))
    }

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

        output_with_deadline(child.wait_with_output(), deadline, cmd).await?
    } else {
        output_with_deadline(command.output(), deadline, cmd).await?
    };

    Ok(ExecResult {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// The table both `exec` and `spawn` answer with.
///
/// Read off the closures below, which build exactly these four keys.
const SHELL_RESULT: &str =
    "{ success: boolean, exit_code: number, stdout: string, stderr: string }";

/// Register the shell module with a Lua state.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
pub fn register_shell_module(lua: &Lua, policy: PluginShellPolicy) -> Result<(), LuaError> {
    let mut shell = crate::host_registry::Ns::new(lua, "cru.shell")?;

    // Wrap policy in Arc for sharing with async closures
    let policy = Arc::new(policy);

    // `args` is REQUIRED, not optional: the closure takes `Vec<String>`, and
    // mlua refuses to build one from nil — `cru.shell.exec("git")` raises
    // "error converting Lua nil to Vec<String>".
    let policy_clone = policy.clone();
    shell.async_func(
        "exec",
        &format!(
            "(command: string, args: {{ string }}, \
             options: {{ cwd: string?, env: table<string, string>?, stdin: string?, \
             timeout: number? }}?) \
             -> {SHELL_RESULT}"
        ),
        move |lua, (cmd, args, options): (String, Vec<String>, Option<Table>)| {
            let policy = policy_clone.clone();
            async move {
                let mut cwd = None;
                let mut env = None;
                let mut stdin_data = None;
                // SECONDS. Bounded by the policy: a plugin may shorten its own
                // deadline and may not lengthen the sandbox's.
                let mut timeout_secs = None;

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
                    if let Ok(secs) = opts.get::<u64>("timeout") {
                        timeout_secs = Some(secs);
                    }
                }

                let result = exec_command(
                    &cmd,
                    &args,
                    cwd.as_deref(),
                    env.as_ref(),
                    stdin_data.as_deref(),
                    &policy,
                    timeout_secs,
                )
                .await?;

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

    // Same result shape as `exec`, plus `options.on_line(stream, line)` called
    // as output arrives. A plugin building an image can report progress
    // instead of going silent for minutes.
    //
    // `stdin` is NOT read here — `spawn_command` takes no stdin — so the
    // options shape is `exec`'s with `stdin` replaced by `on_line`.
    let policy_clone = policy.clone();
    shell.async_func(
        "spawn",
        &format!(
            "(command: string, args: {{ string }}, \
             options: {{ cwd: string?, env: table<string, string>?, \
             on_line: ((stream: string, line: string) -> ())?, timeout: number? }}?) \
             -> {SHELL_RESULT}"
        ),
        move |lua, (cmd, args, options): (String, Vec<String>, Option<Table>)| {
            let policy = policy_clone.clone();
            async move {
                let mut cwd = None;
                let mut env = None;
                let mut on_line: Option<mlua::Function> = None;
                // SECONDS, and bounded by the policy — see `deadline_for`.
                let mut timeout_secs = None;

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
                    if let Ok(secs) = opts.get::<u64>("timeout") {
                        timeout_secs = Some(secs);
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
                        timeout_secs,
                    )
                    .await?
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

    // A simple PATH lookup. Answers nil rather than raising when the command
    // is not on PATH, and when `$PATH` is unset at all.
    shell.func(
        "which",
        "(command: string) -> string?",
        |lua, cmd: String| {
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
        },
    )?;

    shell.publish()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default policy imposes no deadline: plugins legitimately run
    /// commands with no natural end (dev servers) or unbounded duration
    /// (builds, PDF extraction of large files), and a default cap silently
    /// killed them. A policy can still opt into a deadline with `Some(_)`.
    /// A per-call `timeout` is honoured, and a policy cap still wins.
    ///
    /// It used to be read by nothing. `cru.shell.exec(cmd, args, { timeout = 30 })`
    /// parsed, typechecked and was discarded, so every deadline a plugin set
    /// was silently the policy's — which defaults to none. The `oci` plugin
    /// passes one on every container call and on every build, so
    /// `[plugins.oci] build_timeout` had never once applied.
    #[test]
    fn a_per_call_deadline_is_honoured_and_the_policy_caps_it() {
        let uncapped = PluginShellPolicy::default();
        assert_eq!(uncapped.timeout_secs, None, "no default cap");
        assert_eq!(
            uncapped.deadline_for(Some(30)),
            Some(30),
            "with no policy cap, the caller's deadline is the deadline"
        );
        assert_eq!(
            uncapped.deadline_for(None),
            None,
            "and asking for none still means none"
        );

        let capped = PluginShellPolicy {
            timeout_secs: Some(10),
            ..PluginShellPolicy::default()
        };
        assert_eq!(
            capped.deadline_for(Some(30)),
            Some(10),
            "a plugin may not lengthen the sandbox's deadline"
        );
        assert_eq!(
            capped.deadline_for(Some(5)),
            Some(5),
            "but it may shorten its own"
        );
        assert_eq!(
            capped.deadline_for(None),
            Some(10),
            "and saying nothing leaves the policy's in force"
        );
    }

    #[test]
    fn default_policy_has_no_timeout() {
        assert_eq!(PluginShellPolicy::default().timeout_secs, None);
        assert_eq!(PluginShellPolicy::permissive().timeout_secs, None);
    }

    /// With no timeout configured, a command outliving the old 30s-default
    /// window's *shape* (represented by a multi-second sleep) completes
    /// normally instead of being killed at a deadline.
    #[tokio::test]
    async fn exec_without_timeout_lets_slow_commands_finish() {
        let policy = PluginShellPolicy {
            blocked_commands: vec![],
            timeout_secs: None,
            ..Default::default()
        };
        let result = exec_command(
            "sh",
            &["-c".to_string(), "sleep 2; echo done".to_string()],
            None,
            None,
            None,
            &policy,
            None,
        )
        .await
        .expect("exec should not time out");
        assert!(result.success);
        assert_eq!(result.stdout.trim(), "done");
    }

    /// An explicit deadline still enforces.
    #[tokio::test]
    async fn exec_with_explicit_timeout_still_kills() {
        let policy = PluginShellPolicy {
            blocked_commands: vec![],
            timeout_secs: Some(1),
            ..Default::default()
        };
        let err = exec_command(
            "sh",
            &["-c".to_string(), "sleep 5".to_string()],
            None,
            None,
            None,
            &policy,
            None,
        )
        .await
        .expect_err("should time out");
        assert!(err.to_string().contains("timed out"));
    }

    /// Streaming exists so long-running commands can report progress. `exec`
    /// buffers everything and returns at completion, so a five-minute image
    /// build emits nothing until it is over — no status API can fix that from
    /// the outside, which is why this is the prerequisite for progress
    /// reporting rather than a nicety.
    #[tokio::test]
    async fn spawn_streams_lines_as_they_arrive_and_still_returns_the_whole_output() {
        let policy = PluginShellPolicy {
            blocked_commands: vec![],
            timeout_secs: Some(30),
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
            None,
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
        let policy = PluginShellPolicy {
            blocked_commands: vec![],
            timeout_secs: Some(30),
            ..Default::default()
        };
        let result = spawn_command(
            "sh",
            &["-c".to_string(), "echo partial; exit 3".to_string()],
            None,
            None,
            &policy,
            &mut |_, _| {},
            None,
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
            PluginShellPolicy {
                blocked_commands: vec![],
                timeout_secs: Some(30),
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
        let policy = PluginShellPolicy::default();
        let err = spawn_command("rm", &[], None, None, &policy, &mut |_, _| {}, None)
            .await
            .expect_err("the policy must gate streaming exactly as it gates exec");
        assert!(err.to_string().contains("not allowed"), "{err}");
    }

    #[test]
    fn test_policy_default_blocked() {
        let policy = PluginShellPolicy::default();
        assert!(!policy.is_allowed("rm"));
        assert!(!policy.is_allowed("sudo"));
        assert!(policy.is_allowed("echo"));
        assert!(policy.is_allowed("cargo"));
    }

    #[test]
    fn test_policy_permissive() {
        let policy = PluginShellPolicy::permissive();
        assert!(policy.is_allowed("rm"));
        assert!(policy.is_allowed("sudo"));
        assert!(policy.is_allowed("anything"));
    }

    #[test]
    fn test_policy_allowed_list() {
        let policy = PluginShellPolicy {
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
        let policy = PluginShellPolicy::permissive();
        let result = exec_command("echo", &["hello".to_string()], None, None, None, &policy, None)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_exec_blocked_command() {
        let policy = PluginShellPolicy::default();
        let result = exec_command(
            "rm",
            &["-rf".to_string(), "/".to_string()],
            None,
            None,
            None,
            &policy,
            None,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_exec_with_env() {
        let policy = PluginShellPolicy::permissive();
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "test_value".to_string());

        let result = exec_command(
            "sh",
            &["-c".to_string(), "echo $MY_VAR".to_string()],
            None,
            Some(&env),
            None,
            &policy,
            None,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_exec_with_stdin() {
        let policy = PluginShellPolicy::permissive();
        let result = exec_command("cat", &[], None, None, Some("hello world"), &policy, None)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_exec_with_stdin_multiline() {
        let policy = PluginShellPolicy::permissive();
        let content = "line1\nline2\nline3";
        let result = exec_command("cat", &[], None, None, Some(content), &policy, None)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout, content);
    }

    #[tokio::test]
    async fn test_exec_without_stdin_does_not_hang() {
        let policy = PluginShellPolicy::permissive();
        let result = exec_command("echo", &["no-stdin".to_string()], None, None, None, &policy, None)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.stdout.trim(), "no-stdin");
    }

    /// A stream that ends while the other stays open must not spin the pump.
    ///
    /// The regression: once one reader hit EOF its `select!` branch completed
    /// instantly on every iteration, so the pump never returned `Pending`. That
    /// starved the timer driver on a current-thread runtime, so
    /// `tokio::time::timeout(policy.timeout_secs, …)` could never fire and the
    /// call ran until something else killed it — observed as a 120s nextest
    /// timeout on an unrelated test in the same binary, with a core pegged.
    ///
    /// Driven from an OS thread with a std-channel deadline rather than
    /// `#[tokio::test]` + `tokio::time::timeout`, because the bug's whole shape is
    /// a starved runtime: an in-runtime deadline is exactly the thing that cannot
    /// fire, so a regression would hang the suite instead of failing this test.
    #[test]
    fn a_stream_that_ends_early_does_not_spin_the_pump() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            // stdout closes immediately; stderr is held open by a sleep that far
            // outlives the 1s policy timeout and never writes. So one reader is
            // exhausted while the other is genuinely pending — the interleaving
            // that used to spin.
            let result = rt.block_on(spawn_command(
                "sh",
                &[
                    "-c".to_string(),
                    "echo done; exec 1>&-; sleep 60".to_string(),
                ],
                None,
                None,
                &PluginShellPolicy {
                    blocked_commands: vec![],
                    timeout_secs: Some(1),
                    ..Default::default()
                },
                &mut |_stream, _line| {},
                None,
            ));
            let _ = tx.send(result.is_err());
        });

        let timed_out_at_the_policy_deadline =
            rx.recv_timeout(std::time::Duration::from_secs(20)).expect(
                "spawn_command never returned: the pump spun without yielding, so the policy \
                 timeout could not fire",
            );
        assert!(
            timed_out_at_the_policy_deadline,
            "the call must end as a policy timeout, since the child outlives it"
        );
    }
}
