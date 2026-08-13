use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use super::NewArgs;
use crate::config::CliConfig;

const TEMPLATE_PLUGIN_YAML: &str = include_str!("templates/plugin.yaml");
const TEMPLATE_INIT_LUA: &str = include_str!("templates/init.lua");
const TEMPLATE_HEALTH_LUA: &str = include_str!("templates/health.lua");
const TEMPLATE_LUARC_JSON: &str = include_str!("templates/.luarc.json");
const TEMPLATE_TESTS_INIT: &str = include_str!("templates/tests/init_test.lua");

pub async fn execute(_config: CliConfig, args: NewArgs) -> Result<()> {
    let output_dir = args
        .output
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let plugin_dir = output_dir.join(&args.name);

    if plugin_dir.exists() && !args.force {
        eprintln!(
            "Error: directory '{}' already exists. Use --force to overwrite.",
            args.name
        );
        std::process::exit(1);
    }

    if plugin_dir.exists() && args.force {
        fs::remove_dir_all(&plugin_dir)?;
    }

    fs::create_dir_all(&plugin_dir)?;
    fs::create_dir_all(plugin_dir.join("tests"))?;

    let plugin_yaml = TEMPLATE_PLUGIN_YAML.replace("{{name}}", &args.name);
    let init_lua = TEMPLATE_INIT_LUA.replace("{{name}}", &args.name);
    let health_lua = TEMPLATE_HEALTH_LUA.replace("{{name}}", &args.name);
    let tests_init = TEMPLATE_TESTS_INIT.replace("{{name}}", &args.name);

    fs::write(plugin_dir.join("plugin.yaml"), plugin_yaml)?;
    fs::write(plugin_dir.join("init.lua"), init_lua)?;
    fs::write(plugin_dir.join("health.lua"), health_lua)?;
    fs::write(plugin_dir.join(".luarc.json"), luarc_json(&stub_dir()))?;
    fs::write(plugin_dir.join("tests/init_test.lua"), tests_init)?;

    println!(
        "✓ Plugin '{}' created at {}",
        args.name,
        plugin_dir.display()
    );
    println!();
    println!("Next steps:");
    println!("  cd {}", args.name);
    println!("  cru plugin stubs   # if you have not generated them yet");
    println!("  cru plugin test .");

    Ok(())
}

/// Where the stubs live. One shared answer, because a scaffold pointing
/// somewhere nothing writes is the same as not pointing anywhere.
fn stub_dir() -> PathBuf {
    crucible_core::config::lua_stubs_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config/crucible/luals"))
}

/// The scaffolded `.luarc.json`, with the stub directory substituted in.
///
/// The template was written verbatim, so a fresh plugin got a `.luarc.json`
/// whose `workspace.library` never mentioned the stubs — "zero-config IDE
/// setup" that required the author to paste the path in by hand, which is
/// exactly what `cru plugin stubs` told them to do.
fn luarc_json(stub_dir: &Path) -> String {
    TEMPLATE_LUARC_JSON.replace("{{stub_dir}}", &stub_dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scaffolded plugin's `.luarc.json` must point at the type stubs.
    ///
    /// The template was written out verbatim, so `workspace.library` never
    /// mentioned them and the advertised "zero-config IDE setup" required the
    /// author to paste a path in by hand — which is precisely what
    /// `cru plugin stubs` printed instructions to do.
    #[test]
    fn the_scaffolded_luarc_json_points_at_the_generated_stub_directory() {
        let stubs = Path::new("/home/dev/.config/crucible/stubs");
        let rendered = luarc_json(stubs);

        assert!(
            !rendered.contains("{{stub_dir}}"),
            "the placeholder must be substituted, got:\n{rendered}"
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&rendered).expect("scaffolded .luarc.json must be valid JSON");
        let library = parsed["workspace"]["library"]
            .as_array()
            .expect("workspace.library");

        assert!(
            library
                .iter()
                .any(|entry| entry.as_str() == Some("/home/dev/.config/crucible/stubs")),
            "workspace.library must contain the stub dir, got: {library:?}"
        );
    }

    /// The scaffolded suite must require the plugin by its DIRECTORY NAME.
    ///
    /// It required `"init"`, which resolves against nothing: the test runner
    /// mirrors the daemon loader's `package.path`, and that exposes a plugin as
    /// `<plugins-parent>/?/init.lua`. So every plugin `cru plugin new` had ever
    /// produced shipped with a test file that could not load — "0 passed, 0
    /// failed, 1 test file(s) failed to load" — which is the first thing a new
    /// plugin author runs.
    #[test]
    fn the_scaffolded_test_requires_the_plugin_by_its_directory_name() {
        let rendered = TEMPLATE_TESTS_INIT.replace("{{name}}", "my-widget");

        assert!(
            rendered.contains(r#"require("my-widget")"#),
            "the suite must require the plugin by directory name, got:\n{rendered}"
        );
        assert!(
            !rendered.contains(r#"require("init")"#),
            "require(\"init\") resolves against nothing in the test runner"
        );
    }

    /// The scaffolded plugin must not lead with constructs the loader ignores.
    ///
    /// The template returned `hooks = { on_session_start = ... }`, but the spec
    /// parser's recognised fields are name/version/tools/commands/handlers/
    /// views/setup (`crucible-lua/src/lifecycle/spec.rs`), so the hook never
    /// fired. Its `-- @tool` / `-- @param` doc comments were decorative in the
    /// same way: annotations are not discovered.
    #[test]
    fn the_scaffolded_plugin_uses_only_constructs_the_loader_honours() {
        let rendered = TEMPLATE_INIT_LUA.replace("{{name}}", "my-widget");

        // Anchored to a line start, not a bare substring: the template
        // explains in prose why a `hooks = {...}` field does not work, and a
        // substring check matches that explanation.
        assert!(
            !rendered
                .lines()
                .any(|line| line.trim_start().starts_with("hooks = {")),
            "a `hooks` table is silently ignored by the spec parser"
        );
        assert!(
            rendered.contains("crucible.on_session_start("),
            "the template should show the registration that actually works"
        );
        // Anchored, for the same reason as above: the template names these
        // constructs in prose to say they do nothing.
        for annotation in ["@tool", "@param"] {
            assert!(
                !rendered.lines().any(|line| {
                    let trimmed = line.trim_start();
                    trimmed.starts_with("-- ") && trimmed[3..].trim_start().starts_with(annotation)
                }),
                "`{annotation}` is not discovered; it must not look load-bearing"
            );
        }
    }

    /// …and it must be the directory `cru plugin stubs` actually writes to.
    /// Two independent spellings of one path is how this breaks next.
    #[test]
    fn the_scaffold_and_the_stubs_command_agree_on_the_directory() {
        assert_eq!(
            stub_dir(),
            super::super::stubs::default_stub_dir().expect("default stub dir"),
        );
    }
}
