use std::path::PathBuf;

use anyhow::{Context, Result};
use crucible_daemon::LuaGenerateStubsRequest;

use super::StubsArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: StubsArgs) -> Result<()> {
    let output_dir = resolve_output_dir(args.output)?;
    std::fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    // Connect to daemon
    let client = crate::common::daemon_client().await?;

    // Generate or verify stubs via daemon RPC
    let response = client
        .lua_generate_stubs(LuaGenerateStubsRequest {
            output_dir: output_dir.to_string_lossy().to_string(),
            verify: args.verify,
        })
        .await?;

    if args.verify {
        if response.status == "ok" {
            println!("✓ Stubs are up to date");
        } else {
            eprintln!("✗ Stubs are out of date. Run: cru plugin stubs");
            std::process::exit(1);
        }
    } else {
        println!("✓ Stubs generated at: {}", output_dir.display());
        // `cru plugin new` writes this path into the scaffolded `.luarc.json`,
        // so only a hand-written or relocated one needs the instruction.
        println!();
        println!("Plugins scaffolded with `cru plugin new` already point here.");
        println!("For an existing .luarc.json, add to workspace.library:");
        println!("    \"{}\"", output_dir.display());
    }

    Ok(())
}

fn resolve_output_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    match explicit {
        Some(path) => Ok(path),
        None => default_stub_dir(),
    }
}

/// Where stubs land with no `--output`. Shared with the `cru plugin new`
/// scaffold, which writes this path into the generated `.luarc.json`.
pub(super) fn default_stub_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config_dir.join("crucible").join("stubs"))
}
