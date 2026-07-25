use anyhow::Result;
use colored::Colorize;
use crucible_daemon::LuaRunPluginTestsRequest;

use super::TestArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: TestArgs) -> Result<()> {
    if !args.path.exists() {
        eprintln!("{} Path does not exist: {}", "✗".red(), args.path.display());
        std::process::exit(2);
    }

    // Connect to daemon
    let client = crate::common::daemon_client().await?;

    // Run plugin tests via daemon RPC
    let response = client
        .lua_run_plugin_tests(LuaRunPluginTestsRequest {
            test_path: args.path.to_string_lossy().to_string(),
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
        let where_ = failure
            .line
            .as_deref()
            .map(|l| format!(" (line {l})"))
            .unwrap_or_default();
        eprintln!(
            "{} {}{}\n    {}",
            "✗".red(),
            failure.name,
            where_,
            failure.error
        );
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
