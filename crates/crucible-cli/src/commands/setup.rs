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

    // Copying a tree onto itself truncates every file to zero bytes, because
    // `fs::copy` opens the destination for writing before reading the source.
    // `find_source_runtime` already excludes the destination; this refuses
    // rather than trusting that, because the failure destroys user data and
    // says nothing while it does it.
    if same_dir(&source, &target) {
        anyhow::bail!(
            "source and target are the same directory ({}); nothing to copy",
            target.display()
        );
    }

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

    // The default target is a well-known root (`crucible_core::runtime_roots`),
    // so it needs no follow-up. Only a custom `--runtime-dir` does — this used
    // to print the env-var instruction unconditionally, for a directory
    // nothing read.
    if crucible_core::runtime_roots::user_runtime().as_deref() == Some(target.as_path()) {
        println!("\nSetup complete. Crucible reads this directory automatically.");
    } else {
        println!("\nSetup complete. Point Crucible at it — shell profile:");
        println!("  export CRUCIBLE_RUNTIME=\"{}\"", target.display());
        println!("\nor ~/.config/crucible/config.toml:");
        println!("  runtimepath = [\"{}\"]", target.display());
    }

    Ok(())
}

fn default_runtime_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("crucible").join("runtime"))
        .unwrap_or_else(|| PathBuf::from("~/.config/crucible/runtime"))
}

/// Find the source runtime directory — check alongside the binary, then
/// repo-relative, then the CWD.
///
/// **Exe-relative roots only**, deliberately not the full
/// `runtime_roots::for_current_exe()`. That list leads with
/// `~/.config/crucible/runtime`, which is this command's *destination*: a
/// second `cru setup --force` would find its own output, set source == target,
/// and `fs::copy` every file onto itself — truncating each one to zero bytes.
/// Read locations and copy sources are not the same list.
fn find_source_runtime() -> Option<PathBuf> {
    let looks_like_runtime =
        |dir: &Path| dir.join("plugins").exists() || dir.join("themes").exists();

    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(crucible_core::runtime_roots::exe_relative))
        .map(Vec::from)
        .unwrap_or_default()
        .into_iter()
        // Running from the repo root, with no usable exe-relative tree.
        .chain(std::iter::once(PathBuf::from("runtime")))
        .find(|dir| looks_like_runtime(dir))
}

/// Whether two paths resolve to the same directory, `..` and symlinks and all.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        // An uncreated target cannot be the source; a source that will not
        // canonicalize would have failed the existence check above.
        _ => false,
    }
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

-- Colours. "term4" is the terminal's own slot 4 — whatever you configured
-- there — rather than a claim that it looks blue.
-- crucible.colorscheme.setup({ colors = { primary = "term4" } })

-- Surfaces
-- crucible.ui.setup({
--   popup  = { border = "rounded", padding = 1 },
--   prompt = { normal = { glyph = "> " } },
-- })

-- Statusline: each region is an ordered list; the input is an element
-- local sl = crucible.statusline
-- sl.setup({
--   prompt = {
--     sl.input,
--     { sl.mode, " ", sl.model({ max = 25 }),
--       sl.align,
--       sl.any(sl.notification, sl.context) },
--   },
-- })

-- Session defaults: the values a NEW session starts from.
-- (`cru.defaults.x` is Neovim's `vim.o`; `session.x` is `vim.bo`.)
-- cru.defaults.temperature = 0.7
-- cru.defaults.system_prompt = cru.defaults.system_prompt
--   .. "\n\nAnswer in British English."

-- Per session, for anything conditional
-- cru.on_session_start(function(session)
--   if session.workspace:match("/work/") then
--     session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
--   end
-- end)

-- Modes. `tools` gates visibility; `permissions` gates what may be done with
-- a visible tool, in the same `tool:pattern` grammar as [permissions].
-- cru.modes.review = {
--   tools = { "read_*", "*_search", "bash" },
--   permissions = { default = "deny", allow = { "bash:rg *", "bash:grep *" } },
-- }

-- Permission hooks, for anything conditional. Yours run BEFORE the shipped
-- ones, so this wins over the built-ins.
-- cru.permissions.on_request(function(request)
--   return { deny = true }
-- end, { pattern = "bash" })
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// `cru setup` must never take its own destination as its source.
    ///
    /// `~/.config/crucible/runtime` became a read root so the daemon would
    /// find what `cru setup` writes. Reusing that list to pick the *source*
    /// made a second `cru setup --force` copy the tree onto itself, and
    /// `fs::copy` truncates the destination before reading — every file in a
    /// user's customised runtime went to zero bytes, silently.
    #[test]
    fn a_directory_is_the_same_as_itself_through_any_spelling() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("runtime");
        std::fs::create_dir_all(dir.join("plugins")).unwrap();

        assert!(same_dir(&dir, &dir));
        assert!(
            same_dir(&dir, &dir.join("plugins").join("..")),
            "`..` must not disguise the same directory"
        );
        assert!(
            !same_dir(&dir, tmp.path()),
            "a parent is not the same directory"
        );
    }
}
