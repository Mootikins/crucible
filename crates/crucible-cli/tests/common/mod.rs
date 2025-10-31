use anyhow::{Context, Result};
use crucible_config::Config;
use std::path::PathBuf;

pub use crucible_core::test_support::{create_basic_kiln, create_kiln_with_files, kiln_path_str};

/// Output captured from running a CLI command.
pub struct CliCommandOutput {
    pub stdout: String,
    pub stderr: String,
}

/// Locate the compiled `cru` binary within the workspace.
pub fn cli_binary_path() -> PathBuf {
    let base_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
        std::env::current_dir()
            .expect("current directory should be accessible")
            .to_string_lossy()
            .to_string()
    });

    let debug_path = PathBuf::from(&base_dir).join("../../target/debug/cru");
    let release_path = PathBuf::from(&base_dir).join("../../target/release/cru");

    if debug_path.exists() {
        debug_path
    } else if release_path.exists() {
        release_path
    } else {
        panic!("`cru` binary not found. Run `cargo build -p crucible-cli` first.");
    }
}

/// Run the CLI with the provided arguments and optional configuration.
pub async fn run_cli_command(args: &[&str], config: &Config) -> Result<CliCommandOutput> {
    let binary_path = cli_binary_path();
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let kiln_path = config.kiln_path_opt().map(|s| s.to_string());

    tokio::task::spawn_blocking(move || -> Result<CliCommandOutput> {
        let mut cmd = std::process::Command::new(&binary_path);

        let temp_home = tempfile::Builder::new()
            .prefix("crucible-cli-home")
            .tempdir()
            .context("failed to create temporary HOME directory")?;

        cmd.env("HOME", temp_home.path());
        cmd.env("XDG_CONFIG_HOME", temp_home.path());
        cmd.env("XDG_DATA_HOME", temp_home.path());

        let _config_file = if let Some(kiln_path) = kiln_path {
            let temp_config = tempfile::Builder::new()
                .prefix("crucible-cli-config")
                .suffix(".toml")
                .tempfile()
                .context("failed to create temporary CLI config file")?;

            let cli_config_toml = format!(
                "[kiln]\npath = \"{}\"\nembedding_url = \"http://localhost:11434\"\n",
                kiln_path.replace('\\', "\\\\")
            );

            std::fs::write(temp_config.path(), cli_config_toml)
                .context("failed to write CLI config file")?;

            cmd.arg("--config").arg(temp_config.path());
            Some(temp_config)
        } else {
            None
        };

        cmd.args(&args);

        let output = cmd.output().context("command execution failed")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            return Err(anyhow::anyhow!(
                "CLI command failed (exit code {}): {}\n{}",
                code,
                stderr,
                stdout
            ));
        }

        Ok(CliCommandOutput { stdout, stderr })
    })
    .await
    .expect("spawn_blocking should not panic")
}
