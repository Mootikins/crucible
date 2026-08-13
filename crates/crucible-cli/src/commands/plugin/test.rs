use anyhow::Result;
use colored::Colorize;
use crucible_daemon::LuaRunPluginTestsRequest;

use super::TestArgs;
use crate::config::CliConfig;

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
        std::process::exit(2);
    }

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
