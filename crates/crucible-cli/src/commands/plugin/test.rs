use anyhow::Result;
use colored::Colorize;
use crucible_daemon::{LuaRunPluginTestsRequest, LuaRunPluginTestsResponse};

use super::TestArgs;
use crate::config::CliConfig;

/// The process exit code for a finished run.
///
/// A run that found no test files is NOT a pass. The daemon reports it as
/// `0 passed, 0 failed`, which is character-for-character what a green suite
/// reports, and it sends a `message` on that path alone.
fn exit_code(response: &LuaRunPluginTestsResponse) -> i32 {
    if response.message.is_some() || response.load_failures > 0 {
        2
    } else if response.failed > 0 {
        1
    } else {
        0
    }
}

pub async fn execute(_config: CliConfig, args: TestArgs) -> Result<()> {
    // Resolved HERE, before it crosses the RPC boundary. The daemon re-checks
    // existence against its OWN working directory, which is wherever it was
    // spawned — `%h` for the systemd unit, the repo root for a shell-started
    // one. So a relative path validated in this process and then sent verbatim
    // means two different things at the two ends, and `just test plugins` passed
    // or failed depending on where the developer's daemon happened to start:
    //   Error: RPC error: Test path does not exist: runtime/plugins/discord
    // with `runtime/plugins/discord` sitting right there in the repo.
    let test_path = match args.path.canonicalize() {
        Ok(path) => path,
        Err(err) => {
            eprintln!(
                "{} Path does not exist: {} ({err})",
                "✗".red(),
                args.path.display()
            );
            std::process::exit(2);
        }
    };

    // Connect to daemon
    let client = crate::common::daemon_client().await?;

    // Run plugin tests via daemon RPC
    let response = client
        .lua_run_plugin_tests(LuaRunPluginTestsRequest {
            test_path: test_path.to_string_lossy().to_string(),
            filter: args.filter,
        })
        .await?;

    let passed = response.passed;
    let failed = response.failed;
    let load_failures = response.load_failures;

    // The daemon says here that it found nothing to run. Without this the user
    // reads `0 passed, 0 failed` and a zero exit, and a plugin whose suite the
    // discovery missed looks exactly like a plugin whose suite is green.
    if let Some(message) = &response.message {
        eprintln!("{} {}", "✗".red(), message);
    }

    // The runner's own per-test output goes to the *daemon's* stdout, so
    // without printing these the user sees a bare count and nothing to act on.
    for failure in &response.load_failure_details {
        eprintln!(
            "{} could not load {}\n    {}",
            "✗".red(),
            failure.file,
            failure.error
        );
    }

    for failure in &response.failures {
        let title = match &failure.suite {
            Some(suite) if !suite.is_empty() => format!("{suite} / {}", failure.name),
            _ => failure.name.clone(),
        };
        let location = match (&failure.file, &failure.line) {
            (Some(file), Some(line)) => format!("\n    at {file}:{line}"),
            (None, Some(line)) => format!("\n    at line {line}"),
            _ => String::new(),
        };
        // Indent continuation lines: assertion messages are multi-line
        // ("Expected: ...\nActual: ...") and unindented they run into the
        // next failure.
        let message = failure.error.replace('\n', "\n    ");
        eprintln!("{} {}\n    {}{}", "✗".red(), title, message, location);
    }

    println!(
        "{}, {}",
        format!("{} passed", passed).green(),
        format!("{} failed", failed).red()
    );

    if load_failures > 0 {
        eprintln!(
            "{} {} test file(s) failed to load",
            "✗".red(),
            load_failures
        );
    }

    match exit_code(&response) {
        0 => Ok(()),
        code => std::process::exit(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(
        passed: usize,
        failed: usize,
        load_failures: usize,
        message: Option<&str>,
    ) -> LuaRunPluginTestsResponse {
        LuaRunPluginTestsResponse {
            passed,
            failed,
            load_failures,
            failures: Vec::new(),
            load_failure_details: Vec::new(),
            message: message.map(str::to_string),
        }
    }

    #[test]
    fn a_run_that_found_no_test_files_does_not_exit_green() {
        let response = response(0, 0, 0, Some("No test files found"));

        assert_eq!(exit_code(&response), 2);
    }

    #[test]
    fn a_green_suite_exits_zero() {
        assert_eq!(exit_code(&response(7, 0, 0, None)), 0);
    }

    #[test]
    fn a_failed_test_exits_one() {
        assert_eq!(exit_code(&response(7, 1, 0, None)), 1);
    }

    #[test]
    fn a_file_that_could_not_load_exits_two() {
        assert_eq!(exit_code(&response(7, 0, 1, None)), 2);
    }
}
