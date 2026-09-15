//! One author-facing process boundary; exhaustive shipped suites run in nextest.
mod cli_e2e_helpers;

use cli_e2e_helpers::TestDaemon;
use predicates::prelude::*;

#[test]
fn plugin_suite_results_and_type_errors_reach_the_cli() {
    let daemon = TestDaemon::start();
    let tmp = tempfile::tempdir().unwrap();
    let plugin = tmp.path().join("cli-probe");
    std::fs::create_dir_all(plugin.join("tests")).unwrap();
    std::fs::write(
        plugin.join("init.luau"),
        "return { name = 'cli-probe', version = '0.1.0' }",
    )
    .unwrap();
    let suite = plugin.join("tests/probe_test.luau");
    for (body, code, message) in [
        (
            "describe('probe', function() it('passes', function() expect.equal(true, true) end) end)",
            0,
            "1 passed",
        ),
        (
            "describe('probe', function() it('named failure', function() expect.equal(false, true) end) end)",
            1,
            "named failure",
        ),
        ("error('load sentinel')", 2, "load sentinel"),
    ] {
        std::fs::write(&suite, body).unwrap();
        let output = daemon
            .command()
            .args(["plugin", "test"])
            .arg(&plugin)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains(message), "{text}");
    }
    // A filter matching nothing must not report a successful suite.
    std::fs::remove_file(&suite).unwrap();
    daemon
        .command()
        .args(["plugin", "test"])
        .arg(&plugin)
        .assert()
        .code(2);

    let definitions = tmp.path().join("definitions");
    daemon
        .command()
        .args(["plugin", "stubs", "--offline", "--output"])
        .arg(&definitions)
        .assert()
        .success();
    daemon
        .command()
        .args(["plugin", "check"])
        .arg(&plugin)
        .arg("--definitions")
        .arg(definitions.join("cru.d.luau"))
        .assert()
        .success()
        .stdout(predicate::str::contains("typecheck: ran"));
    std::fs::write(plugin.join("init.luau"),
        "local number: number = 'wrong'\nreturn { name = 'cli-probe', version = '0.1.0', number = number }").unwrap();
    daemon
        .command()
        .args(["plugin", "check"])
        .arg(&plugin)
        .arg("--definitions")
        .arg(definitions.join("cru.d.luau"))
        .assert()
        .failure();
}
