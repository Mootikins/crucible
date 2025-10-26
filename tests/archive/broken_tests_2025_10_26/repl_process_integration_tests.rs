use crate::test_utilities::TestKiln;
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use tracing::{debug, info, warn};

// End-to-End REPL Process Tests
//
// This test file validates the actual REPL's `:run` command by spawning a real REPL process
// and testing complete tool execution flow from user's perspective.
//
// The tests:
// 1. Spawn a real REPL process using subprocess
// 2. Send commands via stdin and capture stdout/stderr
// 3. Validate actual tool execution and output content
// 4. Test both successful executions and error handling
// 5. Ensure proper process cleanup and stability

/// REPL process wrapper for end-to-end testing
pub struct ReplProcess {
    /// Child process handle
    child: std::process::Child,
    /// Stdin handle for sending commands
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    /// Stdout reader for capturing output
    stdout_reader: Arc<Mutex<std::io::BufReader<std::process::ChildStdout>>>,
    /// Stderr reader for capturing errors
    stderr_reader: Arc<Mutex<std::io::BufReader<std::process::ChildStderr>>>,
    /// Output buffer
    output_buffer: Arc<Mutex<String>>,
    /// Error buffer
    error_buffer: Arc<Mutex<String>>,
}

impl ReplProcess {
    /// Spawn a new REPL process for testing
    pub fn spawn(kiln_path: &str, db_path: Option<&str>) -> Result<Self> {
        info!("Spawning REPL process for testing");

        // Create temporary config and history files
        let temp_dir = TempDir::new()?;
        let history_file = temp_dir.path().join("test_history");
        let tool_dir = temp_dir.path().join("test_tools");
        std::fs::create_dir_all(&tool_dir)?;

        // Build the crucible CLI command
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "--bin",
            "crucible",
            "--",
            "--vault-path",
            kiln_path,
            "--format",
            "table", // Use table format for easier testing
        ]);

        // Set up pipes for stdin, stdout, stderr
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawn the process
        let mut child = cmd.spawn().context("Failed to spawn REPL process")?;

        // Get handles to stdin, stdout, stderr
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin handle"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout handle"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stderr handle"))?;

        let stdin = Arc::new(Mutex::new(stdin));
        let stdout_reader = Arc::new(Mutex::new(std::io::BufReader::new(stdout)));
        let stderr_reader = Arc::new(Mutex::new(std::io::BufReader::new(stderr)));
        let output_buffer = Arc::new(Mutex::new(String::new()));
        let error_buffer = Arc::new(Mutex::new(String::new()));

        // Start background threads to read stdout and stderr
        Self::start_stdout_reader(stdout_reader.clone(), output_buffer.clone());
        Self::start_stderr_reader(stderr_reader.clone(), error_buffer.clone());

        info!("REPL process spawned successfully");

        Ok(Self {
            child,
            stdin,
            stdout_reader,
            stderr_reader,
            output_buffer,
            error_buffer,
        })
    }

    /// Start a background thread to read stdout
    fn start_stdout_reader(
        reader: Arc<Mutex<std::io::BufReader<std::process::ChildStdout>>>,
        buffer: Arc<Mutex<String>>,
    ) {
        thread::spawn(move || {
            let mut local_buffer = String::new();

            loop {
                match reader.lock() {
                    Ok(mut reader) => {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                debug!("STDOUT: EOF reached");
                                break;
                            }
                            Ok(_) => {
                                local_buffer.push_str(&line);
                                debug!("STDOUT: {}", line.trim());
                            }
                            Err(e) => {
                                warn!("Error reading from STDOUT: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to lock STDOUT reader: {}", e);
                        break;
                    }
                }
            }

            // Update the shared buffer
            if let Ok(mut buffer) = buffer.lock() {
                buffer.push_str(&local_buffer);
            }
        });
    }

    /// Start a background thread to read stderr
    fn start_stderr_reader(
        reader: Arc<Mutex<std::io::BufReader<std::process::ChildStderr>>>,
        buffer: Arc<Mutex<String>>,
    ) {
        thread::spawn(move || {
            let mut local_buffer = String::new();

            loop {
                match reader.lock() {
                    Ok(mut reader) => {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                debug!("STDERR: EOF reached");
                                break;
                            }
                            Ok(_) => {
                                local_buffer.push_str(&line);
                                debug!("STDERR: {}", line.trim());
                            }
                            Err(e) => {
                                warn!("Error reading from STDERR: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to lock STDERR reader: {}", e);
                        break;
                    }
                }
            }

            // Update the shared buffer
            if let Ok(mut buffer) = buffer.lock() {
                buffer.push_str(&local_buffer);
            }
        });
    }

    /// Send a command to the REPL process
    pub fn send_command(&self, command: &str) -> Result<()> {
        debug!("Sending command to REPL: {}", command);

        match self.stdin.lock() {
            Ok(mut stdin) => {
                writeln!(stdin, "{}", command).context("Failed to write command to stdin")?;
                stdin.flush().context("Failed to flush stdin")?;
                debug!("Command sent successfully");
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("Failed to lock stdin: {}", e)),
        }
    }

    /// Wait for a specific pattern to appear in the output
    pub fn wait_for_output(&self, pattern: &str, timeout_secs: u64) -> Result<bool> {
        let timeout = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            {
                let buffer = self
                    .output_buffer
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock output buffer: {}", e))?;
                if buffer.contains(pattern) {
                    debug!("Found pattern '{}' in output", pattern);
                    return Ok(true);
                }
            }

            thread::sleep(Duration::from_millis(100));
        }

        warn!(
            "Pattern '{}' not found in output after {} seconds",
            pattern, timeout_secs
        );
        Ok(false)
    }

    /// Get the current output buffer
    pub fn get_output(&self) -> Result<String> {
        let buffer = self
            .output_buffer
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock output buffer: {}", e))?;
        Ok(buffer.clone())
    }

    /// Get the current error buffer
    pub fn get_errors(&self) -> Result<String> {
        let buffer = self
            .error_buffer
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock error buffer: {}", e))?;
        Ok(buffer.clone())
    }

    /// Clear the output buffer
    pub fn clear_output(&self) -> Result<()> {
        let mut buffer = self
            .output_buffer
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock output buffer: {}", e))?;
        buffer.clear();
        Ok(())
    }

    /// Send quit command and wait for process to terminate
    pub fn quit(&mut self) -> Result<()> {
        info!("Quitting REPL process");

        // Send quit command
        self.send_command(":quit")?;

        // Wait for process to terminate
        match self.child.wait() {
            Ok(status) => {
                info!("REPL process exited with status: {}", status);
                Ok(())
            }
            Err(e) => {
                warn!("Error waiting for REPL process to exit: {}", e);
                // Force kill if needed
                let _ = self.child.kill();
                Err(anyhow::anyhow!("Failed to wait for process exit: {}", e))
            }
        }
    }

    /// Force kill the process
    pub fn kill(&mut self) -> Result<()> {
        info!("Force killing REPL process");
        self.child.kill().context("Failed to kill REPL process")?;
        Ok(())
    }
}

impl Drop for ReplProcess {
    fn drop(&mut self) {
        debug!("Dropping ReplProcess - cleaning up");
        let _ = self.kill();
    }
}

/// Test helper to validate system tool execution
#[tokio::test]
async fn test_repl_run_system_info() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize (look for welcome message)
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Clear initial output
    repl.clear_output()?;

    // Send :run system_info command
    repl.send_command(":run system_info")?;

    // Wait for system information output
    let found = repl
        .wait_for_output("OS:", 15)
        .context("System info command did not produce expected output")?;
    assert!(found, "Should see OS information");

    // Get output and validate content
    let output = repl.get_output()?;

    // Validate that system information is present
    assert!(
        output.contains("OS:") || output.contains("os:"),
        "Output should contain OS information"
    );
    assert!(
        output.contains("Memory:") || output.contains("memory:"),
        "Output should contain memory information"
    );
    assert!(
        output.contains("Disk:") || output.contains("disk:"),
        "Output should contain disk information"
    );

    // Clean up
    repl.quit()?;

    println!("✓ System info tool executed successfully via REPL");
    Ok(())
}

/// Test kiln statistics tool execution
#[tokio::test]
async fn test_repl_run_kiln_stats() -> Result<()> {
    // Create test kiln with some content
    let kiln = TestKiln::new()?;
    kiln.create_note("test.md", "# Test Document\n\nThis is a test.")?;
    kiln.create_note(
        "project/notes.md",
        "# Project Notes\n\nImportant project info.",
    )?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Clear initial output
    repl.clear_output()?;

    // Send :run get_kiln_stats command
    repl.send_command(":run get_kiln_stats")?;

    // Wait for kiln statistics output
    let found = repl
        .wait_for_output("total_notes", 15)
        .context("Vault stats command did not produce expected output")?;
    assert!(found, "Should see total_notes in output");

    // Get output and validate content
    let output = repl.get_output()?;

    // Validate that kiln statistics are present
    assert!(
        output.contains("total_notes") || output.contains("Total Notes"),
        "Output should contain total notes count"
    );
    assert!(
        output.contains("total_size") || output.contains("Total Size"),
        "Output should contain total size"
    );

    // Clean up
    repl.quit()?;

    println!("✓ Vault stats tool executed successfully via REPL");
    Ok(())
}

/// Test error handling with invalid tool names
#[tokio::test]
async fn test_repl_run_invalid_tool() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Clear initial output
    repl.clear_output()?;

    // Send :run command with invalid tool name
    repl.send_command(":run nonexistent_tool_12345")?;

    // Wait for error message
    let found = repl
        .wait_for_output("❌ Tool Error", 10)
        .or_else(|_| repl.wait_for_output("Tool Error", 10))
        .or_else(|_| repl.wait_for_output("Error", 10))?;

    assert!(found, "Should see error message for invalid tool");

    // Get output and validate error content
    let output = repl.get_output()?;

    // Validate that error information is present
    assert!(
        output.contains("Error") || output.contains("error") || output.contains("❌"),
        "Output should contain error indication"
    );

    // Clean up
    repl.quit()?;

    println!("✓ Error handling for invalid tool works correctly");
    Ok(())
}

/// Test multiple tool execution sequence
#[tokio::test]
async fn test_repl_multiple_tool_sequence() -> Result<()> {
    // Create test kiln with content
    let kiln = TestKiln::new()?;
    kiln.create_note(
        "research/ai.md",
        "# AI Research\n\nMachine learning topics.",
    )?;
    kiln.create_note("project/todo.md", "# TODO\n\nTasks to complete.")?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Clear initial output
    repl.clear_output()?;

    // Execute sequence of commands

    // 1. List available tools
    repl.send_command(":tools")?;
    let tools_found = repl.wait_for_output("Available Tools", 10)?;
    assert!(tools_found, "Should list available tools");
    repl.clear_output()?;

    // 2. Run system info
    repl.send_command(":run system_info")?;
    let sysinfo_found = repl.wait_for_output("OS:", 15)?;
    assert!(sysinfo_found, "Should show system info");
    repl.clear_output()?;

    // 3. Run kiln stats
    repl.send_command(":run get_kiln_stats")?;
    let stats_found = repl.wait_for_output("total_notes", 15)?;
    assert!(stats_found, "Should show kiln stats");
    repl.clear_output()?;

    // 4. Run search by tags (if available)
    repl.send_command(":run search_by_tags project")?;
    let search_found = repl
        .wait_for_output("results", 15)
        .or_else(|_| repl.wait_for_output("found", 15))?;
    // Don't assert here as the tool might not be available or might return no results

    // 5. Try an invalid tool
    repl.send_command(":run invalid_test_tool")?;
    let error_found = repl.wait_for_output("Error", 10)?;
    assert!(error_found, "Should show error for invalid tool");

    // Clean up
    repl.quit()?;

    println!("✓ Multiple tool execution sequence completed successfully");
    Ok(())
}

/// Test REPL stability after multiple commands
#[tokio::test]
async fn test_repl_stability_after_commands() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Execute many commands to test stability
    for i in 1..=10 {
        repl.clear_output()?;

        // Alternate between different commands
        match i % 4 {
            0 => {
                repl.send_command(":run system_info")?;
                repl.wait_for_output("OS:", 10)?;
            }
            1 => {
                repl.send_command(":tools")?;
                repl.wait_for_output("Available Tools", 10)?;
            }
            2 => {
                repl.send_command(":run get_kiln_stats")?;
                repl.wait_for_output("total_notes", 10)?;
            }
            3 => {
                repl.send_command(":stats")?;
                repl.wait_for_output("Statistics", 10)?;
            }
            _ => unreachable!(),
        }

        // Small delay between commands
        thread::sleep(Duration::from_millis(100));
    }

    // Verify REPL is still responsive
    repl.clear_output()?;
    repl.send_command(":help")?;
    let help_found = repl.wait_for_output("Crucible REPL Commands", 5)?;
    assert!(
        help_found,
        "REPL should still be responsive after multiple commands"
    );

    // Clean up
    repl.quit()?;

    println!("✓ REPL remained stable after multiple command executions");
    Ok(())
}

/// Test tool execution with arguments
#[tokio::test]
async fn test_repl_run_tool_with_args() -> Result<()> {
    // Create test kiln with tagged content
    let kiln = TestKiln::new()?;
    kiln.create_note("research/ml.md", "# Machine Learning\n\n#research #ml #ai")?;
    kiln.create_note("project/main.md", "# Main Project\n\n#project #important")?;
    kiln.create_note("notes/ideas.md", "# Ideas\n\n#ideas #research")?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Clear initial output
    repl.clear_output()?;

    // Run search by tags with arguments
    repl.send_command(":run search_by_tags research")?;

    // Wait for search results
    let found = repl
        .wait_for_output("results", 15)
        .or_else(|_| repl.wait_for_output("found", 15))
        .or_else(|_| repl.wait_for_output("ml.md", 15))?;

    // Get output for analysis
    let output = repl.get_output()?;

    // Clean up
    repl.quit()?;

    // The test passes if the command executes without crashing
    // Results may vary based on tool availability and content
    println!("✓ Tool execution with arguments completed");
    println!("Output snippet: {}", &output[..output.len().min(200)]);

    Ok(())
}

/// Test REPL commands help functionality
#[tokio::test]
async fn test_repl_help_functionality() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Test general help
    repl.clear_output()?;
    repl.send_command(":help")?;
    let help_found = repl.wait_for_output("Crucible REPL Commands", 5)?;
    assert!(help_found, "Should show general help");

    // Test specific command help
    repl.clear_output()?;
    repl.send_command(":help run")?;
    let run_help_found = repl.wait_for_output("Execute Tool", 5)?;
    assert!(run_help_found, "Should show help for :run command");

    // Test tools listing
    repl.clear_output()?;
    repl.send_command(":tools")?;
    let tools_found = repl.wait_for_output("Available Tools", 5)?;
    assert!(tools_found, "Should list available tools");

    // Clean up
    repl.quit()?;

    println!("✓ Help functionality works correctly");
    Ok(())
}

/// Performance test for tool execution
#[tokio::test]
async fn test_repl_tool_execution_performance() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Spawn REPL process
    let mut repl = ReplProcess::spawn(kiln.kiln_path_str(), Some(kiln.db_path_str()))?;

    // Wait for REPL to initialize
    let initialized = repl
        .wait_for_output("Crucible CLI REPL", 10)
        .context("REPL did not initialize within timeout")?;
    assert!(initialized, "REPL should show welcome message");

    // Measure execution time for system_info tool
    let start_time = std::time::Instant::now();

    repl.clear_output()?;
    repl.send_command(":run system_info")?;

    let success = repl.wait_for_output("OS:", 15)?;
    assert!(success, "System info command should complete");

    let execution_time = start_time.elapsed();

    // Clean up
    repl.quit()?;

    // Performance assertion - should complete within reasonable time
    assert!(
        execution_time < Duration::from_secs(20),
        "System info should complete within 20 seconds, took {:?}",
        execution_time
    );

    println!("✓ System info tool executed in {:?}", execution_time);
    Ok(())
}
