use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Bootstrap the Crucible runtime directory.
///
/// Copies the runtime files that layer per name — plugins and themes — to the
/// target directory, and creates a template `init.lua` if one doesn't exist.
/// `defaults/` is deliberately excluded; see `NOT_COPIED`.
pub fn execute(runtime_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let target = runtime_dir.unwrap_or_else(default_runtime_dir);

    // A tree on disk if there is one, the compiled-in copy otherwise. This used
    // to be a hard error — "Could not find Crucible runtime files" — which is
    // what every installed user saw, because no release ever put a tree on
    // disk for it to find.
    let source = find_source_runtime();

    match &source {
        Some(source) => println!("Source:  {}", source.display()),
        None => println!("Source:  bundled with cru"),
    }
    println!("Target:  {}", target.display());

    // Copying a tree onto itself truncates every file to zero bytes, because
    // `fs::copy` opens the destination for writing before reading the source.
    // `find_source_runtime` already excludes the destination; this refuses
    // rather than trusting that, because the failure destroys user data and
    // says nothing while it does it.
    if source.as_deref().is_some_and(|s| same_dir(s, &target)) {
        anyhow::bail!(
            "source and target are the same directory ({}); nothing to copy",
            target.display()
        );
    }

    if target.exists() && !force {
        println!("\nRuntime directory already exists. Use --force to overwrite.");
        return Ok(());
    }

    populate_runtime(source.as_deref(), &target)?;

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

/// Fill `target` with a runtime tree, from `source` if there is one.
///
/// `None` is the installed case rather than an error: the binary carries the
/// tree, so "no runtime files found" is only ever true of the filesystem.
/// Directories `cru setup` deliberately does NOT hand the user a copy of.
///
/// This command's target outranks every shipped root, so a copy here shadows
/// the shipped version permanently. For things that layer per name — plugins,
/// themes — that is exactly right: your `oci` wins, and a plugin added next
/// release still loads beside it. `defaults/init.lua` does not layer. It is one
/// file read first-hit-wins, so a copy of it silently freezes the defaults at
/// the version you ran setup on, and every default added afterwards ships to
/// nobody who ran this command.
///
/// The override point for defaults is `~/.config/crucible/init.lua`, which
/// already runs after them and wins per assignment — Vim's `after/` in
/// everything but name, and it exists precisely so nobody forks a runtime file.
const NOT_COPIED: &[&str] = &["defaults"];

fn populate_runtime(source: Option<&Path>, target: &Path) -> Result<()> {
    match source {
        Some(source) => copy_dir_recursive(source, target)
            .with_context(|| format!("Failed to copy runtime to {}", target.display())),
        None => crucible_core::runtime_roots::write_bundled_runtime(target)
            .with_context(|| format!("Failed to write runtime to {}", target.display())),
    }?;
    for name in NOT_COPIED {
        let path = target.join(name);
        if path.exists() {
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
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
    use crucible_core::runtime_roots::looks_like_runtime;

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
    /// An installed `cru` has no runtime tree to copy — the release archive
    /// never carried one and the shell installer would have discarded it
    /// anyway — so `cru setup` bailed with "Could not find Crucible runtime
    /// files" for precisely the users the command exists to serve.
    /// `cru setup` must not hand the user a copy of the shipped defaults.
    ///
    /// Its target outranks every shipped root, so a copied `defaults/init.lua`
    /// shadows the real one permanently: a default added in a later release
    /// ships and reaches nobody who ran setup — the users engaged enough to run
    /// it. Splunk states the general form outright, that a full copy of
    /// defaults in a user directory "will make your app insensitive to updates
    /// supplied by the app vendor", and Zellij has it as an open bug
    /// (zellij-org/zellij#4360) where a generated config silently hid a new
    /// release's feature.
    ///
    /// Plugins and themes are different and are still copied: those merge per
    /// name across roots, so a user's copy shadows only the item it names and a
    /// newly shipped plugin still loads. Helix moved its runtime lookup to that
    /// same per-file layering for this reason (helix-editor/helix#5411).
    ///
    /// The override point for defaults is `~/.config/crucible/init.lua`, which
    /// already runs after them — Vim's `after/` directory in everything but
    /// name, and it exists so nobody has to fork a runtime file.
    #[test]
    fn setup_does_not_copy_the_shipped_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("runtime");

        populate_runtime(None, &target).unwrap();

        assert!(
            !target.join("defaults").exists(),
            "a copied defaults/ shadows the shipped one for good"
        );
        assert!(
            target.join("plugins").join("kiln-expert").is_dir(),
            "plugins still come across — they layer per name"
        );
        assert!(target.join("themes").is_dir(), "so do themes");
    }

    #[test]
    fn setup_writes_the_compiled_in_tree_when_nothing_is_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("runtime");

        populate_runtime(None, &target).unwrap();

        assert!(target
            .join("plugins")
            .join("kiln-expert")
            .join("plugin.yaml")
            .is_file());
        assert!(target
            .join("crucible-help")
            .join("skills")
            .join("crucible-help")
            .join("SKILL.md")
            .is_file());
    }

    /// `cru setup --force` means "give me the shipped files back".
    ///
    /// The bundled tree skips writing when its stamp says the target already
    /// holds this build — right for the daemon's automatic extraction, wrong
    /// here, where the user has explicitly asked to overwrite whatever they
    /// did to it.
    #[test]
    fn setup_restores_a_hand_edited_tree_rather_than_trusting_its_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("runtime");
        populate_runtime(None, &target).unwrap();

        let plugin = target
            .join("plugins")
            .join("kiln-expert")
            .join("plugin.yaml");
        std::fs::write(&plugin, "# edited").unwrap();

        populate_runtime(None, &target).unwrap();

        assert_ne!(
            std::fs::read_to_string(&plugin).unwrap(),
            "# edited",
            "setup must restore the shipped file, not skip on a matching stamp"
        );
    }

    /// The compiled-in tree is the fallback, not the answer: a tree found next
    /// to the binary is what the user installed, and may have been patched by
    /// a distro packager.
    #[test]
    fn an_installed_tree_is_copied_in_preference_to_the_compiled_in_one() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let target = tmp.path().join("target");
        std::fs::create_dir_all(source.join("plugins")).unwrap();
        std::fs::write(source.join("plugins").join("marker.lua"), "-- packaged").unwrap();

        populate_runtime(Some(&source), &target).unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("plugins").join("marker.lua")).unwrap(),
            "-- packaged"
        );
        assert!(
            !target.join("plugins").join("kiln-expert").is_dir(),
            "the compiled-in tree must not be merged over the installed one"
        );
    }

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
