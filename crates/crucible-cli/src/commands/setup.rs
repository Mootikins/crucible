use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Bootstrap the Crucible runtime directory.
///
/// Copies bundled runtime files (plugins, themes) to the target directory
/// and creates a template init.lua if one doesn't exist.
pub fn execute(runtime_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let target = runtime_dir.unwrap_or_else(default_runtime_dir);

    // Find source runtime directory
    let source = find_source_runtime()
        .context("Could not find Crucible runtime files. If you installed via cargo install, clone the repo and point to it:\n  cru setup --runtime-dir /path/to/crucible/runtime")?;

    println!("Source:  {}", source.display());
    println!("Target:  {}", target.display());

    if target.exists() && !force {
        println!("\nRuntime directory already exists. Use --force to overwrite.");
        return Ok(());
    }

    // Copy runtime directory
    copy_dir_recursive(&source, &target)
        .with_context(|| format!("Failed to copy runtime to {}", target.display()))?;

    println!("Copied runtime files.");

    // Create template init.lua if it doesn't exist
    let config_dir = dirs::config_dir()
        .map(|d| d.join("crucible"))
        .unwrap_or_else(|| PathBuf::from("~/.config/crucible"));
    let init_lua = config_dir.join("init.lua");

    if !init_lua.exists() {
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(&init_lua, TEMPLATE_INIT_LUA)?;
        println!("Created {}", init_lua.display());
    }

    println!("\nSetup complete. Add to your shell profile:");
    println!("  export CRUCIBLE_RUNTIME=\"{}\"", target.display());
    println!("\nOr add to ~/.config/crucible/config.toml:");
    println!("  runtimepath = [\"{}\"]", target.display());

    Ok(())
}

fn default_runtime_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("crucible").join("runtime"))
        .unwrap_or_else(|| PathBuf::from("~/.config/crucible/runtime"))
}

/// Find the source runtime directory — check alongside the binary, then repo-relative.
fn find_source_runtime() -> Option<PathBuf> {
    // 1. Exe-relative: <exe>/../share/crucible/runtime (installed)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let installed = exe_dir
                .join("..")
                .join("share")
                .join("crucible")
                .join("runtime");
            if installed.join("plugins").exists() || installed.join("themes").exists() {
                return Some(installed);
            }
            // 2. Dev: <exe>/../../runtime (cargo build in repo)
            let dev = exe_dir.join("..").join("..").join("runtime");
            if dev.join("plugins").exists() || dev.join("themes").exists() {
                return Some(dev);
            }
        }
    }

    // 3. CWD/runtime (running from repo root)
    let cwd_runtime = PathBuf::from("runtime");
    if cwd_runtime.join("plugins").exists() || cwd_runtime.join("themes").exists() {
        return Some(cwd_runtime);
    }

    None
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

const TEMPLATE_INIT_LUA: &str = r#"-- Crucible user configuration
-- This file runs after the built-in defaults. Override anything here.
-- See: https://mootikins.github.io/crucible/Help/Lua/Configuration/

-- Configure plugins with require("name").setup({...})
-- Bundled plugins load with defaults; your setup() overrides them.
-- Don't require a plugin to skip loading it entirely.
--
-- require("kiln-expert").setup({
--   kilns = { docs = "~/crucible/docs" },
--   timeout = 60,
-- })

-- Configure the statusline
-- cru.statusline.setup({
--   left = { cru.statusline.mode(), cru.statusline.model({ max_length = 25 }) },
--   right = { cru.statusline.notification({ fallback = cru.statusline.context() }) },
-- })

-- Session defaults
-- crucible.on_session_start(function(session)
--   session.temperature = 0.7
-- end)
"#;
