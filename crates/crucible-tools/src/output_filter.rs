//! Output filtering for MCP tool results
//!
//! This module provides built-in filters that transform tool output to be more
//! useful for LLMs. The primary use case is summarizing verbose test output.
//!
//! ## Design Philosophy
//!
//! LLMs benefit from concise, structured output. Verbose test logs with
//! individual test names waste tokens and obscure the important information:
//! did tests pass or fail, and if they failed, what failed?
//!
//! The filters here extract:
//! - Summary lines (pass/fail counts, timing)
//! - Error messages and failure details
//! - Warnings that might indicate problems
//!
//! ## Supported Test Frameworks
//!
//! - **Cargo test** (Rust): `test result: ok. 42 passed`
//! - **pytest** (Python): `====== 10 passed in 0.12s ======`
//! - **Jest** (JavaScript): `Tests: 10 passed, 2 failed`
//! - **Go test**: `PASS` / `FAIL` with package summaries
//! - **`RSpec`** (Ruby): `10 examples, 0 failures`
//! - **Mix test** (Elixir): `10 tests, 0 failures`

use tracing::debug;

/// Filter test output to extract only summary and error information
///
/// Returns `Some(filtered)` if the output was filtered, `None` if it should
/// pass through unchanged (not recognized as test output).
#[must_use]
pub fn filter_test_output(output: &str) -> Option<String> {
    // Detect which test framework produced this output
    if is_cargo_test(output) {
        Some(filter_cargo_test(output))
    } else if is_pytest(output) {
        Some(filter_pytest(output))
    } else if is_jest(output) {
        Some(filter_jest(output))
    } else if is_go_test(output) {
        Some(filter_go_test(output))
    } else if is_rspec_or_mix(output) {
        Some(filter_rspec_mix(output))
    } else {
        None // Not test output, pass through unchanged
    }
}

/// Check if output looks like cargo test
fn is_cargo_test(output: &str) -> bool {
    output.contains("test result:") || (output.contains("running ") && output.contains(" test"))
}

/// Check if output looks like pytest
fn is_pytest(output: &str) -> bool {
    output.contains("passed in ")
        || output.contains("failed in ")
        || (output.contains("=====") && (output.contains("passed") || output.contains("failed")))
}

/// Check if output looks like Jest
fn is_jest(output: &str) -> bool {
    output.contains("Test Suites:")
        || (output.contains("Tests:") && (output.contains("passed") || output.contains("failed")))
}

/// Check if output looks like go test
fn is_go_test(output: &str) -> bool {
    output.starts_with("PASS")
        || output.starts_with("FAIL")
        || output.contains("\nPASS\n")
        || output.contains("\nFAIL\n")
        || output.contains("\nok \t")
        || output.contains("\nFAIL\t")
}

/// Check if output looks like `RSpec` or Mix test
fn is_rspec_or_mix(output: &str) -> bool {
    (output.contains(" examples,") && output.contains(" failure"))
        || (output.contains(" tests,") && output.contains(" failure"))
        || output.contains("Finished in ")
}

/// Filter cargo test output
fn filter_cargo_test(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        // Track failure section
        if line.contains("failures:") {
            in_failures = true;
            continue;
        }
        if in_failures && line.trim().is_empty() {
            in_failures = false;
        }

        // Capture failure details
        if in_failures && !line.trim().is_empty() && !line.contains("---- ") {
            failure_lines.push(line);
        }

        // "running X tests" header
        if line.starts_with("running ") && line.contains(" test") {
            summary_lines.push(line.to_string());
        }

        // Final result line
        if line.contains("test result:") {
            summary_lines.push(line.to_string());
        }

        // Compilation errors
        if line.starts_with("error[") || line.starts_with("error:") {
            summary_lines.push(line.to_string());
        }

        // Warning summary
        if line.contains("warning:") && line.contains("generated") {
            summary_lines.push(line.to_string());
        }
    }

    // Add failures section if any
    if !failure_lines.is_empty() {
        summary_lines.push("\nFailures:".to_string());
        for line in failure_lines.iter().take(20) {
            // Limit failure output
            summary_lines.push(format!("  {line}"));
        }
        if failure_lines.len() > 20 {
            summary_lines.push(format!("  ... and {} more", failure_lines.len() - 20));
        }
    }

    debug!(
        "Filtered cargo test output: {} lines -> {} lines",
        output.lines().count(),
        summary_lines.len()
    );

    summary_lines.join("\n")
}

/// Filter pytest output
fn filter_pytest(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        // Track FAILURES section
        if line.contains("= FAILURES =") || line.contains("= ERRORS =") {
            in_failures = true;
            summary_lines.push(line.to_string());
            continue;
        }

        // End of failures section (next === line that's not FAILURES)
        if in_failures
            && line.starts_with('=')
            && !line.contains("FAILURES")
            && !line.contains("ERRORS")
        {
            in_failures = false;
        }

        // Capture failure details (limited)
        if in_failures {
            failure_lines.push(line.to_string());
            if failure_lines.len() >= 30 {
                in_failures = false; // Stop capturing after limit
            }
        }

        // Summary line with pass/fail counts
        if line.starts_with('=')
            && (line.contains("passed") || line.contains("failed") || line.contains("error"))
        {
            summary_lines.push(line.to_string());
        }

        // Short test summary info
        if line.starts_with("FAILED ") || line.starts_with("ERROR ") {
            summary_lines.push(line.to_string());
        }
    }

    // Add failure details
    if !failure_lines.is_empty() {
        summary_lines.extend(failure_lines.into_iter().take(30));
    }

    debug!(
        "Filtered pytest output: {} lines -> {} lines",
        output.lines().count(),
        summary_lines.len()
    );

    summary_lines.join("\n")
}

/// Filter Jest output
fn filter_jest(output: &str) -> String {
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim_start();

        // Test suites summary
        if line.contains("Test Suites:") {
            summary_lines.push(line.to_string());
        }

        // Tests summary
        if line.contains("Tests:") && (line.contains("passed") || line.contains("failed")) {
            summary_lines.push(line.to_string());
        }

        // Snapshots
        if line.contains("Snapshots:") {
            summary_lines.push(line.to_string());
        }

        // Time
        if line.contains("Time:") {
            summary_lines.push(line.to_string());
        }

        // PASS/FAIL per file (may have leading whitespace)
        if trimmed.starts_with("PASS ") || trimmed.starts_with("FAIL ") {
            summary_lines.push(line.to_string());
        }

        // Failure details
        if line.contains("● ") {
            summary_lines.push(line.to_string());
        }
    }

    debug!(
        "Filtered Jest output: {} lines -> {} lines",
        output.lines().count(),
        summary_lines.len()
    );

    summary_lines.join("\n")
}

/// Filter go test output
fn filter_go_test(output: &str) -> String {
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        // Package pass/fail (tab or spaces after FAIL/ok)
        if line.starts_with("ok \t") || line.starts_with("ok  ") {
            summary_lines.push(line.to_string());
        }
        if line.starts_with("FAIL\t") || line.starts_with("FAIL ") {
            // But not "FAIL:" which is a different pattern
            if !line.starts_with("FAIL:") {
                summary_lines.push(line.to_string());
            }
        }

        // Overall PASS/FAIL
        if line == "PASS" || line == "FAIL" {
            summary_lines.push(line.to_string());
        }

        // Individual test failures
        if line.starts_with("--- FAIL:") {
            summary_lines.push(line.to_string());
        }

        // Error output
        if line.contains("FAIL:") || line.starts_with("    Error:") {
            summary_lines.push(line.to_string());
        }
    }

    debug!(
        "Filtered go test output: {} lines -> {} lines",
        output.lines().count(),
        summary_lines.len()
    );

    summary_lines.join("\n")
}

/// Filter `RSpec` or Elixir mix test output
fn filter_rspec_mix(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        // Failures section
        if line.contains("Failures:") {
            in_failures = true;
            summary_lines.push(line.to_string());
            continue;
        }

        // End of failures
        if in_failures && line.starts_with("Finished in ") {
            in_failures = false;
        }

        // Capture failure details
        if in_failures {
            failure_lines.push(line.to_string());
            if failure_lines.len() >= 30 {
                in_failures = false;
            }
        }

        // Timing
        if line.starts_with("Finished in ") {
            summary_lines.push(line.to_string());
        }

        // RSpec summary
        if line.contains(" examples,") && line.contains(" failure") {
            summary_lines.push(line.to_string());
        }

        // Mix test summary
        if line.contains(" tests,") && line.contains(" failure") {
            summary_lines.push(line.to_string());
        }
    }

    // Add failure details
    if !failure_lines.is_empty() {
        summary_lines.extend(failure_lines.into_iter().take(30));
    }

    debug!(
        "Filtered RSpec/Mix output: {} lines -> {} lines",
        output.lines().count(),
        summary_lines.len()
    );

    summary_lines.join("\n")
}

/// Configuration for output filtering
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Enable test output filtering (default: true)
    pub filter_test_output: bool,
    /// Maximum lines to include from failure details (default: 30)
    pub max_failure_lines: usize,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            filter_test_output: true,
            max_failure_lines: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_test_filter() {
        let input = r"
   Compiling myproject v0.1.0
    Finished test target(s) in 2.34s
     Running unittests src/lib.rs

running 42 tests
test foo::test_one ... ok
test foo::test_two ... ok
test bar::test_three ... ok
... (many more tests)
test bar::test_forty_two ... ok

test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("running 42 tests"));
        assert!(filtered.contains("test result: ok. 42 passed"));
        assert!(!filtered.contains("test_one"));
        assert!(!filtered.contains("Compiling"));
    }

    #[test]
    fn test_cargo_test_with_failures() {
        let input = r"
running 5 tests
test foo::test_one ... ok
test foo::test_two ... FAILED
test foo::test_three ... ok
test foo::test_four ... FAILED
test foo::test_five ... ok

failures:

---- foo::test_two stdout ----
assertion failed: 1 == 2

---- foo::test_four stdout ----
thread panicked at 'explicit panic'

failures:
    foo::test_two
    foo::test_four

test result: FAILED. 3 passed; 2 failed; 0 ignored
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("running 5 tests"));
        assert!(filtered.contains("test result: FAILED. 3 passed; 2 failed"));
        assert!(filtered.contains("Failures:"));
        // Should not contain individual test lines
        assert!(!filtered.contains("test foo::test_one ... ok"));
    }

    #[test]
    fn test_pytest_filter() {
        let input = r"
============================= test session starts ==============================
platform linux -- Python 3.10.0, pytest-7.0.0
collected 25 items

test_module.py::test_one PASSED
test_module.py::test_two PASSED
test_module.py::test_three PASSED
... many more ...
test_module.py::test_twenty_five PASSED

============================== 25 passed in 1.23s ==============================
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("25 passed in 1.23s"));
        assert!(!filtered.contains("test_one PASSED"));
        assert!(!filtered.contains("platform linux"));
    }

    #[test]
    fn test_jest_filter() {
        let input = r"
 PASS  src/components/Button.test.js
 PASS  src/components/Input.test.js
 FAIL  src/components/Form.test.js
  ● Form › should submit data

    expect(received).toBe(expected)

Test Suites: 1 failed, 2 passed, 3 total
Tests:       1 failed, 15 passed, 16 total
Snapshots:   0 total
Time:        2.34 s
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("Test Suites: 1 failed, 2 passed"));
        assert!(filtered.contains("Tests:       1 failed, 15 passed"));
        // Note: Jest output has leading space, our filter trims it
        assert!(
            filtered.contains("FAIL  src/components/Form.test.js")
                || filtered.contains("FAIL src/components/Form.test.js")
        );
        assert!(filtered.contains("● Form › should submit data"));
    }

    #[test]
    fn test_go_test_filter() {
        let input = r"
=== RUN   TestFoo
--- PASS: TestFoo (0.00s)
=== RUN   TestBar
--- PASS: TestBar (0.00s)
=== RUN   TestBaz
--- FAIL: TestBaz (0.01s)
    baz_test.go:15: expected 42, got 41
FAIL
exit status 1
FAIL    github.com/user/project    0.123s
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("--- FAIL: TestBaz"));
        // Go output can have tabs or spaces between FAIL and package name
        assert!(
            filtered.contains("FAIL") && filtered.contains("github.com/user/project"),
            "Should contain FAIL and package name. Got: {filtered}"
        );
        assert!(!filtered.contains("=== RUN   TestFoo"));
    }

    #[test]
    fn test_non_test_output_passes_through() {
        let input = "Hello, this is just regular output.";
        assert!(filter_test_output(input).is_none());
    }

    #[test]
    fn test_rspec_filter() {
        let input = r"
Randomized with seed 12345

.........F..........

Failures:

  1) Widget should do something
     Failure/Error: expect(widget.value).to eq(42)
       expected: 42
            got: 41

Finished in 0.12345 seconds (files took 0.5 seconds to load)
20 examples, 1 failure
";

        let filtered = filter_test_output(input).unwrap();

        assert!(filtered.contains("20 examples, 1 failure"));
        assert!(filtered.contains("Finished in 0.12345"));
        assert!(!filtered.contains("Randomized with seed"));
    }
}
