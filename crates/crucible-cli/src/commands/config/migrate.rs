//! `cru config migrate` — the one-time TOML → Lua generator.
//!
//! Reads `config.toml` through the oracle, splits the machine-owned entries
//! into the state files (`auto` kilns and the machine default into
//! `kilns.json`, `[projects.*]` into `projects.json`), emits the remaining
//! keys as Lua, VERIFIES the emitted chunk in-memory against the expected
//! remainder BEFORE writing anything, then writes and renames `config.toml`
//! to `config.toml.migrated`.
//!
//! Placement respects the no-Lua-edit rule (no code edits an existing Lua
//! file): with no `init.lua`, the chunk becomes `init.lua`; with one, it
//! becomes `<config_root>/lua/migrated_config.lua` plus one printed
//! instruction to `require` it near the top.

use anyhow::{bail, Context, Result};
use crucible_core::config::{emit_lua_config, expand_tilde, CliAppConfig, KilnEntry, KilnName};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One `auto = true` kiln entry headed for `kilns.json`.
struct AutoKiln {
    name: KilnName,
    path: PathBuf,
}

pub fn run(config_path_flag: Option<PathBuf>) -> Result<()> {
    let source = config_path_flag.unwrap_or_else(CliAppConfig::default_config_path);
    if !source.exists() {
        bail!(
            "nothing to migrate: {} does not exist. The Lua config (init.lua) is already the \
             only config source.",
            source.display()
        );
    }
    let config_root = source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // Through the oracle: same parse, same rejections, same include pass.
    let oracle = CliAppConfig::load(Some(source.clone()), None, None)?;
    let mut remaining = CliAppConfig::load_seed_value(&source)?;

    // Split 1: `auto = true` kiln entries move to `kilns.json` — the machine
    // wrote them and machine state is where they belong. Hand-written
    // entries stay in the Lua.
    let auto_kilns = split_auto_kilns(&mut remaining)?;

    // Split 2: the machine-set default. The old chat preflight both
    // registered the auto kiln and set `default_kiln`; a default that names
    // an auto entry is that machine's work and moves with it. A default
    // naming a hand-written kiln is user authorship and stays.
    let moved_default = remaining
        .get("default_kiln")
        .and_then(Value::as_str)
        .filter(|default| auto_kilns.iter().any(|k| k.name.as_str() == *default))
        .map(str::to_string);
    if moved_default.is_some() {
        if let Some(object) = remaining.as_object_mut() {
            object.remove("default_kiln");
        }
    }

    // Split 3: `[projects.*]` seeds `projects.json` — projects are matched
    // state since M4. An entry whose directory does not exist cannot be
    // registered (the manager canonicalizes); it stays in the Lua rather
    // than being dropped.
    let project_paths = split_existing_projects(&mut remaining);

    // Emit the remainder, and VERIFY in-memory before writing anything:
    // defaults + the emitted chunk must extract to exactly defaults + the
    // remainder. On a mismatch nothing is written — the emitter is wrong.
    let lua_chunk = emit_lua_config(&remaining);
    let evaluated = crucible_lua::evaluate_config_source(&lua_chunk)?;
    let mut expected_store = crucible_core::config::ConfigStore::for_load();
    expected_store.merge(
        serde_json::to_value(CliAppConfig::default())?,
        crucible_core::config::ConfigSource::Default,
    );
    expected_store.merge(
        remaining.clone(),
        crucible_core::config::ConfigSource::Toml(source.clone()),
    );
    let expected = expected_store.extract()?;
    if serde_json::to_value(&evaluated)? != serde_json::to_value(&expected)? {
        bail!(
            "migrate verification failed: the generated Lua does not reproduce the config. \
             Nothing was written. This is a bug in the emitter — report it."
        );
    }

    // Pre-check the kilns.json merge for re-points BEFORE any write, so a
    // refusal cannot leave a half-migrated tree behind.
    let data_home = oracle
        .data_home
        .clone()
        .unwrap_or_else(crucible_core::config::crucible_home);
    let kiln_state = crucible_daemon::kiln_state::KilnStateStore::new(&data_home);
    let existing = kiln_state.read().unwrap_or_default();
    for kiln in &auto_kilns {
        if let Some(entry) = existing.kilns.get(kiln.name.as_str()) {
            if entry.path != kiln.path {
                bail!(
                    "the kiln name '{}' is already registered to '{}' in {}; \
                     it cannot be re-pointed to '{}'. Nothing was written.",
                    kiln.name,
                    entry.path.display(),
                    kiln_state.path().display(),
                    kiln.path.display()
                );
            }
        }
    }

    // Write the Lua. No code edits an existing Lua file: absent init.lua
    // gets the chunk AS init.lua; a present one gets a module beside it.
    let init_path = config_root.join("init.lua");
    let written = if init_path.exists() {
        let lua_dir = config_root.join("lua");
        std::fs::create_dir_all(&lua_dir)?;
        let module_path = lua_dir.join("migrated_config.lua");
        std::fs::write(&module_path, &lua_chunk)?;
        println!(
            "Wrote {}.\nAdd this line near the TOP of {} (so your later lines override it):\n\n    \
             require(\"migrated_config\")\n",
            module_path.display(),
            init_path.display()
        );
        module_path
    } else {
        std::fs::write(&init_path, &lua_chunk)?;
        println!("Wrote {}.", init_path.display());
        init_path.clone()
    };

    // The state files, under their own sidecar locks.
    for kiln in &auto_kilns {
        kiln_state
            .register(
                &kiln.name,
                &kiln.path,
                true,
                moved_default.as_deref() == Some(kiln.name.as_str()),
            )
            .with_context(|| format!("registering kiln '{}'", kiln.name))?;
    }
    if !project_paths.is_empty() {
        let projects =
            crucible_daemon::project_manager::ProjectManager::new(data_home.join("projects.json"));
        for path in &project_paths {
            if let Err(e) = projects.register_if_missing(path) {
                tracing::warn!("project '{}' was not seeded: {e}", path.display());
            }
        }
    }

    // Retire the TOML. The seed path stops finding it, so the Lua is now
    // the config.
    let migrated = source.with_extension("toml.migrated");
    std::fs::rename(&source, &migrated)?;
    println!("Renamed {} to {}.", source.display(), migrated.display());

    // Say what moved and what stayed, BY NAME. A migration that relocates a
    // user's registrations silently is a supersession the user cannot see —
    // and this one they cannot easily undo.
    if !auto_kilns.is_empty() {
        let moved: Vec<String> = auto_kilns
            .iter()
            .map(|k| format!("{} (auto)", k.name))
            .collect();
        println!(
            "Moved to {}: {}",
            kiln_state.path().display(),
            moved.join(", ")
        );
    }
    let kept_kilns: Vec<String> = remaining
        .get("kilns")
        .and_then(Value::as_object)
        .map(|kilns| kilns.keys().cloned().collect())
        .unwrap_or_default();
    if !kept_kilns.is_empty() {
        println!(
            "Kept in the Lua config (hand-written): {}",
            kept_kilns.join(", ")
        );
    }
    if !project_paths.is_empty() {
        let moved: Vec<String> = project_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        println!("Moved to projects.json: {}", moved.join(", "));
    }
    let kept_projects: Vec<String> = remaining
        .get("projects")
        .and_then(Value::as_object)
        .map(|projects| projects.keys().cloned().collect())
        .unwrap_or_default();
    if !kept_projects.is_empty() {
        println!(
            "Kept in the Lua config (directory missing): {}",
            kept_projects.join(", ")
        );
    }

    // A `[plugins.X]` section stops applying the moment init.lua calls X's
    // setup directly — the direct call owns the plugin. Say so HERE, where
    // the user can still act on it, with the same words the boot warning
    // uses.
    if init_path.exists() {
        if let Ok(init_source) = std::fs::read_to_string(&init_path) {
            if let Some(sections) = remaining.get("plugins").and_then(Value::as_object) {
                for name in sections.keys() {
                    let double = format!("require(\"{name}\")");
                    let single = format!("require('{name}')");
                    if init_source.contains(&double) || init_source.contains(&single) {
                        println!(
                            "note: init.lua appears to call {name}'s setup directly, so the \
                             plugins.{name} section will be ignored; move those keys into the \
                             setup call"
                        );
                    }
                }
            }
        }
    }

    println!("Restart the daemon to apply: cru daemon restart");
    let _ = written;
    Ok(())
}

/// Remove the `auto = true` kiln entries from the seed value and return
/// them, tilde-expanded. A relative path cannot be registered (state
/// registrations are absolute) and stays in the Lua.
fn split_auto_kilns(remaining: &mut Value) -> Result<Vec<AutoKiln>> {
    let Some(kilns) = remaining.get_mut("kilns").and_then(Value::as_object_mut) else {
        return Ok(Vec::new());
    };

    let mut moved = Vec::new();
    let names: Vec<String> = kilns.keys().cloned().collect();
    for name in names {
        let entry: KilnEntry = match serde_json::from_value(kilns[&name].clone()) {
            Ok(entry) => entry,
            Err(_) => continue, // an unparseable entry stays where it is
        };
        let KilnEntry::Config {
            auto: true, path, ..
        } = entry
        else {
            continue;
        };
        let expanded = expand_tilde(&path.to_string_lossy(), dirs::home_dir().as_deref());
        if !expanded.is_absolute() {
            tracing::warn!(
                "auto kiln '{name}' has the relative path '{}'; it stays in the Lua config",
                path.display()
            );
            continue;
        }
        let parsed = KilnName::parse(&name)
            .map_err(|e| anyhow::anyhow!("kiln name '{name}' does not parse: {e}"))?;
        kilns.remove(&name);
        moved.push(AutoKiln {
            name: parsed,
            path: expanded,
        });
    }

    if kilns.is_empty() {
        if let Some(object) = remaining.as_object_mut() {
            object.remove("kilns");
        }
    }
    Ok(moved)
}

/// Remove the `[projects.*]` entries whose directories exist and return
/// their paths. A missing directory stays in the Lua rather than being
/// dropped.
fn split_existing_projects(remaining: &mut Value) -> Vec<PathBuf> {
    let Some(projects) = remaining.get_mut("projects").and_then(Value::as_object_mut) else {
        return Vec::new();
    };

    let mut moved = Vec::new();
    let names: Vec<String> = projects.keys().cloned().collect();
    for name in names {
        let Some(path) = projects[&name]
            .get("path")
            .and_then(Value::as_str)
            .map(|raw| expand_tilde(raw, dirs::home_dir().as_deref()))
        else {
            continue;
        };
        if path.is_dir() {
            projects.remove(&name);
            moved.push(path);
        } else {
            tracing::warn!(
                "project '{name}' points at a directory that does not exist; it stays in the Lua config"
            );
        }
    }

    if projects.is_empty() {
        if let Some(object) = remaining.as_object_mut() {
            object.remove("projects");
        }
    }
    moved
}
