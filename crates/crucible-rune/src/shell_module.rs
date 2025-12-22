//! Shell module for Rune
//!
//! Provides command execution for Rune scripts with security policies.
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

use crucible_config::ShellPolicy;
use rune::alloc::Vec as RuneVec;
use rune::{Any, ContextError, Module};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Core implementation of command execution with optional policy enforcement
///
/// Arguments:
/// - cmd: Command to execute
/// - args: Arguments as a vector of strings
/// - options: Object with optional timeout, cwd, env fields
/// - policy: Optional security policy to enforce
///
/// Returns Result<ExecResult, ExecError> which supports the ? operator
fn exec_impl_with_policy(
    cmd: String,
    args: RuneVec<String>,
    options: rune::runtime::Object,
    policy: Option<&ShellPolicy>,
) -> Result<RuneExecResult, RuneExecError> {
    use std::process::Command;

    let args_vec: Vec<String> = args.into_iter().collect();
    let args_ref: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    // Check policy if provided
    if let Some(policy) = policy {
        if !policy.is_allowed(&cmd, &args_ref) {
            return Err(RuneExecError {
                message: format!(
                    "Command '{}' is not whitelisted by security policy",
                    if args_ref.is_empty() {
                        cmd.clone()
                    } else {
                        format!("{} {}", cmd, args_ref.join(" "))
                    }
                ),
            });
        }
    }

    let mut command = Command::new(&cmd);
    command.args(&args_ref);

    // Parse cwd option
    if let Some(cwd_value) = options.get("cwd") {
        if let Ok(cwd) = rune::from_value::<String>(cwd_value.clone()) {
            command.current_dir(cwd);
        }
    }

    // Parse env option
    if let Some(env_value) = options.get("env") {
        if let Ok(env_obj) = rune::from_value::<rune::runtime::Object>(env_value.clone()) {
            for (key, value) in env_obj.iter() {
                if let Ok(val_str) = rune::from_value::<String>(value.clone()) {
                    command.env(key.as_str(), val_str);
                }
            }
        }
    }

    // TODO: timeout support requires async or threads

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

/// Wrapper struct to hold policy for Rune exec function
#[derive(Clone)]
struct ExecPolicyWrapper {
    policy: Arc<ShellPolicy>,
}

impl ExecPolicyWrapper {
    /// Execute with policy check
    fn exec(
        &self,
        cmd: String,
        args: RuneVec<String>,
        options: rune::runtime::Object,
    ) -> Result<RuneExecResult, RuneExecError> {
        exec_impl_with_policy(cmd, args, options, Some(&self.policy))
    }
}

/// Create the shell module for Rune with a specific security policy
///
/// # Arguments
///
/// * `policy` - Security policy defining which commands are allowed
///
/// # Example
///
/// ```rust
/// use crucible_config::ShellPolicy;
/// use crucible_rune::shell_module_with_policy;
///
/// let mut policy = ShellPolicy::default();
/// policy.whitelist.push("git".to_string());
/// let module = shell_module_with_policy(policy).unwrap();
/// ```
pub fn shell_module_with_policy(policy: ShellPolicy) -> Result<Module, ContextError> {
    let mut module = Module::with_crate("shell")?;

    // Register the result type
    module.ty::<RuneExecResult>()?;

    // Register the error type
    module.ty::<RuneExecError>()?;

    // Wrap policy in wrapper for method-based registration
    let wrapper = ExecPolicyWrapper {
        policy: Arc::new(policy),
    };

    // Register the exec function with a closure that captures the wrapper
    module.function(
        "exec",
        move |cmd: String, args: RuneVec<String>, options: rune::runtime::Object| {
            wrapper.exec(cmd, args, options)
        },
    );

    Ok(module)
}

/// Create the shell module for Rune with default security policy
///
/// Uses `ShellPolicy::with_defaults()` which includes:
/// - Safe development commands (git, cargo, npm, etc.)
/// - Blocks dangerous commands (sudo, rm -rf /, etc.)
///
/// # Example
///
/// ```rust
/// use crucible_rune::shell_module;
///
/// let module = shell_module().unwrap();
/// ```
pub fn shell_module() -> Result<Module, ContextError> {
    shell_module_with_policy(ShellPolicy::with_defaults())
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

    /// Test that exec accepts cwd option from Rune
    #[test]
    fn test_exec_with_cwd_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(shell_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("pwd", [], #{ cwd: "/tmp" })?;
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
            .expect("Should execute");
        let output: String = rune::from_value(output).unwrap();

        assert!(
            output.trim() == "/tmp",
            "cwd should change to /tmp, got: {}",
            output.trim()
        );
    }

    /// Test that exec accepts env option from Rune
    #[test]
    fn test_exec_with_env_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(shell_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("sh", ["-c", "echo $TEST_VAR"], #{ env: #{ TEST_VAR: "hello_from_rune" } })?;
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
            .expect("Should execute");
        let output: String = rune::from_value(output).unwrap();

        assert!(
            output.contains("hello_from_rune"),
            "env should be set, got: {}",
            output
        );
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

    /// Test that shell_module_with_policy blocks non-whitelisted commands
    /// This test should FAIL until we implement shell_module_with_policy
    #[test]
    fn test_shell_module_with_policy_blocks_non_whitelisted() {
        use crucible_config::ShellPolicy;
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        // Create restrictive policy - only allow 'echo'
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("echo".to_string());

        // Create context with restricted shell module
        let mut context = Context::with_default_modules().unwrap();
        context
            .install(shell_module_with_policy(policy).unwrap())
            .unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Script tries to run 'rm' which is not whitelisted
        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("rm", ["test.txt"], #{})?;
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

        // Execute - should fail with policy error
        let mut vm = Vm::new(runtime, unit);
        let result = vm.call(rune::Hash::type_hash(["main"]), ());

        assert!(result.is_err(), "Should fail when command is not whitelisted");
        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(
            err_msg.contains("not whitelisted") || err_msg.contains("not allowed"),
            "Error should mention policy violation, got: {}",
            err_msg
        );
    }

    /// Test that shell_module_with_policy allows whitelisted commands
    /// This test should FAIL until we implement shell_module_with_policy
    #[test]
    fn test_shell_module_with_policy_allows_whitelisted() {
        use crucible_config::ShellPolicy;
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        // Create policy that allows echo
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("echo".to_string());

        let mut context = Context::with_default_modules().unwrap();
        context
            .install(shell_module_with_policy(policy).unwrap())
            .unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("echo", ["hello"], #{})?;
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
            .expect("Should execute when command is whitelisted");
        let output: String = rune::from_value(output).unwrap();

        assert!(
            output.contains("hello"),
            "Should execute whitelisted command"
        );
    }

    /// Test that default shell_module uses default policy
    /// This test should FAIL until we update shell_module to use ShellPolicy::with_defaults()
    #[test]
    fn test_shell_module_default_uses_defaults() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(shell_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Test that cargo is allowed (in default whitelist)
        let script_allowed = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("cargo", ["--version"], #{})?;
                result.stdout
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script_allowed).unwrap())
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

        let mut vm = Vm::new(runtime.clone(), unit);
        let output = vm
            .call(rune::Hash::type_hash(["main"]), ())
            .expect("Should allow cargo (in default whitelist)");
        let output: String = rune::from_value(output).unwrap();
        assert!(
            output.contains("cargo"),
            "Should execute cargo from default whitelist"
        );

        // Test that sudo is blocked (in default blacklist)
        let script_blocked = r#"
            use shell::exec;

            pub fn main() {
                let result = exec("sudo", ["ls"], #{})?;
                result.stdout
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script_blocked).unwrap())
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
        let result = vm.call(rune::Hash::type_hash(["main"]), ());

        assert!(
            result.is_err(),
            "Should block sudo (in default blacklist)"
        );
        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(
            err_msg.contains("not whitelisted") || err_msg.contains("not allowed"),
            "Error should mention policy violation for sudo, got: {}",
            err_msg
        );
    }
}
