use std::fs;
use tempfile::TempDir;

#[test]
fn test_init_creates_directories_and_config() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path_arg = temp_dir.path().to_str().unwrap();

    // Run init command
    let output = std::process::Command::new("cargo")
        .current_dir(std::env::current_dir().unwrap())
        .args(&[
            "run",
            "-p",
            "crucible-cli",
            "--",
            "init",
            "--path",
            path_arg,
        ])
        .output()
        .expect("Failed to run init command");

    assert!(
        output.status.success(),
        "Init command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let crucible_dir = temp_dir.path().join(".crucible");

    // Verify directories were created
    assert!(
        crucible_dir.exists(),
        "Expected .crucible directory to be created"
    );
    assert!(
        crucible_dir.join("config.toml").exists(),
        "Expected config.toml to be created"
    );
    assert!(
        crucible_dir.join("sessions").exists(),
        "Expected sessions directory to be created"
    );
    assert!(
        crucible_dir.join("plugins").exists(),
        "Expected plugins directory to be created"
    );

    // Verify config.toml content
    let config_content =
        fs::read_to_string(crucible_dir.join("config.toml")).expect("Failed to read config.toml");
    assert!(config_content.contains("[kiln]"));
    assert!(config_content.contains("[storage]"));
    assert!(config_content.contains("[llm]"));
    assert!(config_content.contains("backend = \"sqlite\""));
}

#[test]
fn test_init_with_existing_directory_errors() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path_arg = temp_dir.path().to_str().unwrap();

    // Run init command first
    let output1 = std::process::Command::new("cargo")
        .current_dir(std::env::current_dir().unwrap())
        .args(&[
            "run",
            "-p",
            "crucible-cli",
            "--",
            "init",
            "--path",
            path_arg,
        ])
        .output()
        .expect("Failed to run init command");
    assert!(output1.status.success(), "First init should succeed");

    // Run init command again (should fail)
    let output2 = std::process::Command::new("cargo")
        .current_dir(std::env::current_dir().unwrap())
        .args(&[
            "run",
            "-p",
            "crucible-cli",
            "--",
            "init",
            "--path",
            path_arg,
        ])
        .output()
        .expect("Failed to run init command");

    assert!(!output2.status.success(), "Second init should have failed");

    let stderr = String::from_utf8_lossy(&output2.stderr);
    assert!(
        stderr.contains("Kiln already initialized"),
        "Expected error message about existing kiln"
    );
}

#[test]
fn test_init_with_force_overwrites() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path_arg = temp_dir.path().to_str().unwrap();

    // Create .crucible directory with some content
    let crucible_dir = temp_dir.path().join(".crucible");
    fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible directory");
    let test_file = crucible_dir.join("test.txt");
    fs::write(&test_file, "test content").expect("Failed to write test file");

    // Run init command with force
    let output = std::process::Command::new("cargo")
        .current_dir(std::env::current_dir().unwrap())
        .args(&[
            "run",
            "-p",
            "crucible-cli",
            "--",
            "init",
            "--path",
            path_arg,
            "--force",
        ])
        .output()
        .expect("Failed to run init command");

    assert!(
        output.status.success(),
        "Init command with force should succeed"
    );

    // Verify test file was removed and directories recreated
    assert!(
        !test_file.exists(),
        "Test file should have been removed by --force"
    );
    assert!(
        crucible_dir.exists(),
        "Expected .crucible directory to still exist"
    );
    assert!(
        crucible_dir.join("config.toml").exists(),
        "Expected config.toml to be created"
    );
}
