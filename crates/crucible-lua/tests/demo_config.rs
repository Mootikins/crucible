//! The shipped demo configs must be a config the daemon actually reads.
//!
//! `assets/demo-config/init.lua` and `assets/demo-acp-config/init.lua` are the
//! configs the VHS tape recordings boot against. They were
//! `demo-config.toml.example` until v0.30.0, and the recipe named the `.toml`
//! file to `--config`. `--config` gives the boot a config ROOT and the boot
//! reads `init.lua` under it, so after the TOML reader went away every demo
//! recorded on the defaults: no kiln, no provider, no ACP profile — and
//! nothing said so.
//!
//! Each file is evaluated through [`crucible_lua::evaluate_config_source`],
//! the same store-and-evaluate construction as the daemon's boot, and the
//! values a demo depends on are read back off the extracted config. Parsing
//! alone would not do: no struct in the config tree sets
//! `deny_unknown_fields`, so a dead key merges, extracts clean, and the file
//! looks configured while the demo runs on a default.

use crucible_core::config::CliAppConfig;
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/crucible-lua has a workspace root two levels up")
        .to_path_buf()
}

/// Evaluate one demo config the way the boot does.
fn evaluate(relative: &str) -> CliAppConfig {
    let path = workspace_root().join(relative);
    assert!(path.exists(), "{relative} must exist at {}", path.display());
    let source = std::fs::read_to_string(&path).expect("the demo config must be readable");
    crucible_lua::evaluate_config_source(&source)
        .unwrap_or_else(|e| panic!("{relative} must evaluate through the boot path: {e}"))
}

/// The demo config must be `init.lua`, and its kiln must be the `docs/` tree
/// that ships with the repository.
#[test]
fn demo_config_evaluates_and_sets_the_docs_kiln() {
    let config = evaluate("assets/demo-config/init.lua");

    assert!(
        config.kiln_path.ends_with("docs"),
        "kiln_path should end with 'docs', got: {:?}",
        config.kiln_path
    );
    assert_eq!(
        config.llm.default.as_deref(),
        Some("demo"),
        "the demo provider must be the default one"
    );
    assert!(
        config.llm.providers.contains_key("demo"),
        "the 'demo' provider must be declared: {:?}",
        config.llm.providers.keys().collect::<Vec<_>>()
    );
}

/// The ACP demo config carries the same kiln plus the agent profiles the
/// delegation recording drives.
#[test]
fn demo_acp_config_evaluates_and_sets_the_docs_kiln() {
    let config = evaluate("assets/demo-acp-config/init.lua");

    assert!(
        config.kiln_path.ends_with("docs"),
        "kiln_path should end with 'docs', got: {:?}",
        config.kiln_path
    );
    assert!(
        config.llm.providers.contains_key("demo"),
        "the 'demo' provider must be declared: {:?}",
        config.llm.providers.keys().collect::<Vec<_>>()
    );
}

/// The delegation demo records one agent delegating to two others. Every part
/// of that shape has to survive the evaluation.
#[test]
fn demo_acp_config_has_valid_delegation_config() {
    let config = evaluate("assets/demo-acp-config/init.lua");

    assert!(
        !config.acp.agents.is_empty(),
        "ACP config should have agent profiles"
    );
    let claude = config
        .acp
        .agents
        .get("claude")
        .expect("ACP config should have a 'claude' agent profile");

    let delegation = claude
        .delegation
        .as_ref()
        .expect("the claude agent should have a delegation config");
    assert!(delegation.enabled, "claude delegation should be enabled");
    assert_eq!(delegation.max_depth, 1, "max_depth should be 1");

    let allowed = delegation
        .allowed_targets
        .as_ref()
        .expect("the claude agent should have allowed_targets");
    assert!(
        allowed.contains(&"cursor".to_string()),
        "cursor should be in allowed_targets: {allowed:?}"
    );
    assert!(
        allowed.contains(&"opencode".to_string()),
        "opencode should be in allowed_targets: {allowed:?}"
    );
}

/// No COMMITTED demo config may be TOML again. `--config` names a file whose
/// DIRECTORY is the config root; a `.toml` beside it is read by nothing, so a
/// recipe pointing at one is a recipe that records the defaults.
///
/// Tracked AND present: a developer's own untracked copy of the old file is
/// theirs to keep, and a gate that reddened on it would be reporting
/// something no commit can fix. A path git still tracks but the tree no
/// longer holds is already on its way out.
#[test]
fn no_committed_demo_config_is_toml() {
    let listing = Command::new("git")
        .args(["ls-files", "-z", "--", "assets"])
        .current_dir(workspace_root())
        .output()
        .expect("git ls-files must run");
    assert!(listing.status.success(), "git ls-files failed");

    let strays: Vec<String> = String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            name.starts_with("demo") && name.contains("config") && name.contains(".toml")
        })
        .filter(|path| workspace_root().join(path).exists())
        .map(str::to_string)
        .collect();
    assert!(
        strays.is_empty(),
        "no committed demo config may be TOML — the boot reads init.lua: {strays:?}"
    );
}
