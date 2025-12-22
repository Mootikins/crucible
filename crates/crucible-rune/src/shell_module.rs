//! Shell module for Rune
//!
//! Provides command execution for Rune scripts.
//!
//! # Example
//!
//! ```rune
//! use shell::exec;
//!
//! // Simple command
//! let result = exec("echo", ["hello"], #{})?;
//! // result = #{ stdout: "hello\n", stderr: "", exit_code: 0 }
//!
//! // With options
//! let result = exec("cargo", ["build"], #{
//!     timeout: 60000,
//!     cwd: "/path/to/project",
//!     env: #{ RUST_LOG: "debug" }
//! })?;
//! ```

use rune::alloc::Vec as RuneVec;
use rune::runtime::VmResult;
use rune::{Any, ContextError, Module};
use std::collections::HashMap;

/// Options for command execution
#[derive(Debug, Default)]
pub struct ExecOptions {
    /// Timeout in milliseconds
    pub timeout: Option<u64>,
    /// Working directory
    pub cwd: Option<String>,
    /// Environment variables
    pub env: Option<HashMap<String, String>>,
}

/// Output from command execution
#[derive(Debug)]
pub struct ExecOutput {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code
    pub exit_code: i32,
}

/// Execute a command synchronously (blocking)
///
/// This is the core implementation that will be wrapped for Rune.
pub fn exec_impl(cmd: &str, args: &[&str], options: ExecOptions) -> Result<ExecOutput, String> {
    use std::process::Command;

    let mut command = Command::new(cmd);
    command.args(args);

    if let Some(cwd) = options.cwd {
        command.current_dir(cwd);
    }

    if let Some(env) = options.env {
        for (key, value) in env {
            command.env(key, value);
        }
    }

    // TODO: timeout support requires async or threads
    // For now, we ignore timeout in sync version

    let output = command.output().map_err(|e| e.to_string())?;

    Ok(ExecOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Output from command execution (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::shell, name = ExecResult)]
pub struct RuneExecResult {
    /// Standard output
    #[rune(get)]
    pub stdout: String,
    /// Standard error
    #[rune(get)]
    pub stderr: String,
    /// Exit code
    #[rune(get)]
    pub exit_code: i64,
}

/// Error type for shell execution (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::shell, name = ExecError)]
pub struct RuneExecError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RuneExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Execute a command from Rune
///
/// Arguments:
/// - cmd: Command to execute
/// - args: Arguments as a vector of strings
/// - options: Object with optional timeout, cwd, env fields
///
/// Returns Result<ExecResult, ExecError> which supports the ? operator
#[rune::function]
fn exec(
    cmd: String,
    args: RuneVec<String>,
    _options: rune::Value,
) -> Result<RuneExecResult, RuneExecError> {
    use std::process::Command;

    let args_vec: Vec<String> = args.into_iter().collect();
    let args_ref: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    let mut command = Command::new(&cmd);
    command.args(&args_ref);

    // TODO: Parse options for cwd, env, timeout

    match command.output() {
        Ok(output) => Ok(RuneExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1) as i64,
        }),
        Err(e) => Err(RuneExecError {
            message: format!("Command execution failed: {}", e),
        }),
    }
}

/// Create the shell module for Rune
pub fn shell_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("shell")?;

    // Register the result type
    module.ty::<RuneExecResult>()?;

    // Register the error type
    module.ty::<RuneExecError>()?;

    // Register the exec function
    module.function_meta(exec)?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_module_creation() {
        let module = shell_module();
        assert!(module.is_ok(), "Should create shell module");
    }

    /// Test that exec can be called from Rune script
    /// This test should FAIL until we register the exec function
    #[test]
    fn test_exec_callable_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        // Create context with shell module
        let mut context = Context::with_default_modules().unwrap();
        context.install(shell_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Rune script that calls shell::exec
        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("echo", ["hello"], #{})?;
                result.stdout
            }
        "#;

        // Compile
        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile script with shell::exec");
        let unit = Arc::new(unit);

        // Execute
        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        assert!(output.contains("hello"), "Should capture stdout");
    }

    /// Test that exec can be used with ? operator in Rune
    /// This test should FAIL until we return a proper Result type
    #[test]
    fn test_exec_supports_try_operator() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(shell_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Script using ? operator
        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("echo", ["test"], #{})?;
                result.stdout
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm
            .call(rune::Hash::type_hash(["main"]), ())
            .expect("Should execute with ? operator");
        let output: String = rune::from_value(output).unwrap();

        assert!(output.contains("test"));
    }

    #[test]
    fn test_exec_simple_command() {
        let result = exec_impl("echo", &["hello"], ExecOptions::default());
        assert!(result.is_ok(), "exec should succeed: {:?}", result);

        let output = result.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn test_exec_captures_stderr() {
        let result = exec_impl("sh", &["-c", "echo error >&2"], ExecOptions::default());
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.stderr.contains("error"));
    }

    #[test]
    fn test_exec_returns_exit_code() {
        let result = exec_impl("sh", &["-c", "exit 42"], ExecOptions::default());
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.exit_code, 42);
    }

    #[test]
    fn test_exec_with_cwd() {
        let options = ExecOptions {
            cwd: Some("/tmp".to_string()),
            ..Default::default()
        };

        let result = exec_impl("pwd", &[], options);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.stdout.trim() == "/tmp");
    }

    #[test]
    fn test_exec_with_env() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "my_value".to_string());

        let options = ExecOptions {
            env: Some(env),
            ..Default::default()
        };

        let result = exec_impl("sh", &["-c", "echo $MY_VAR"], options);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.stdout.contains("my_value"));
    }

    #[test]
    fn test_exec_command_not_found() {
        let result = exec_impl("nonexistent_command_12345", &[], ExecOptions::default());
        assert!(result.is_err());
    }
}
