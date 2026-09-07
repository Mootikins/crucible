//! Daemon-side agent-card discovery.
//!
//! Agent cards (markdown + YAML frontmatter, `crucible_core::agent`) define
//! specialized internal agents: a system prompt plus optional model,
//! generation knobs, tool policy, and MCP servers. The daemon discovers them
//! per session context and uses them as delegation targets and for
//! `session.create` agent resolution.
//!
//! Discovery precedence (highest first, first match wins):
//! 1. `WORKSPACE/.crucible/agents/` — project-scoped cards (repos)
//! 2. `KILN/.crucible/agents/` — kiln config
//! 3. `agent_directories` from the app config, in config order
//! 4. `~/.config/crucible/agents/` — global personal cards
//!
//! The list used to run the other way and take the LAST match. Same outcome,
//! opposite spelling; it reads highest-first now because every other resolver
//! does, and carrying two directions in one codebase is how `cru agents list`
//! came to advertise cards the daemon would not resolve.
//!
//! Only `.crucible/` directories. See [`card_directories`] for why a kiln's
//! visible top level is not scanned.
//!
//! Discovery runs per use (like skills discovery) rather than through a
//! cached registry — card sets are tiny and this avoids staleness/watchers.
//!
//! The CLI (`cru agents`) reads cards off disk through [`card_directories`]
//! too, so the two never disagree about where a card may come from.

use crucible_core::agent::{AgentCard, AgentCardLoader};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

/// The session-independent roots of agent-card discovery: the global config
/// directory and the directories the app config names.
///
/// Both are injected as values, never read from the environment at discovery
/// time. Global cards are first in precedence, so a handler that read
/// `dirs::config_dir()` would resolve a developer's own cards in every test —
/// passing on CI, failing locally. `Default` is "no global cards, no
/// configured directories".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardRoots {
    /// The config home `<config_home>/crucible/agents` hangs off. `None`
    /// means "no global cards".
    pub config_home: Option<PathBuf>,
    /// `agent_directories` from the app config, tilde already expanded.
    ///
    /// **Deprecated.** It names one leaf directory that serves cards only.
    /// `runtimepath` names a root that serves every asset kind, so a user who
    /// wants to share cards, skills and themes from one directory writes one
    /// line instead of three knobs. Kept working; [`warn_if_deprecated`] says
    /// so once.
    pub agent_directories: Vec<PathBuf>,
}

impl CardRoots {
    /// Read `agent_directories` out of the serialized app config. `home`
    /// expands a leading `~`; `None` leaves the path as written.
    pub fn from_app_config(
        config_home: Option<PathBuf>,
        app_config: Option<&serde_json::Value>,
        home: Option<&Path>,
    ) -> Self {
        let agent_directories = app_config
            .and_then(|v| v.get("agent_directories"))
            .and_then(|v| v.as_array())
            .map(|dirs| {
                dirs.iter()
                    .filter_map(|d| d.as_str())
                    .map(|d| crucible_core::config::expand_tilde(d, home))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            config_home,
            agent_directories,
        }
    }
}

/// Warn once that `agent_directories` is superseded by `runtimepath`.
///
/// Once per process, not per session: card discovery runs per use, and a line
/// per turn would be noise rather than guidance.
pub fn warn_if_deprecated(roots: &CardRoots) {
    use std::sync::Once;
    static WARNED: Once = Once::new();
    if roots.agent_directories.is_empty() {
        return;
    }
    WARNED.call_once(|| {
        tracing::warn!(
            directories = ?roots.agent_directories,
            "`agent_directories` is deprecated: it serves cards only. \
             Put the directory on `runtimepath` instead and its agents/, \
             skills/ and themes/ are all found."
        );
    });
}

/// Candidate card directories for a session context, highest priority first.
///
/// Only `.crucible/` directories, never a kiln's visible top level. `KILN/agents/`
/// and `KILN/Agents/` used to be scanned, which made any cloned or synced kiln
/// able to introduce an agent card — a card names a model, a system prompt and
/// a tool set, so that is a meaningful thing to have appear without asking.
/// The visible top level of a kiln belongs to notes; Crucible reads only the
/// config directory it owns.
///
/// A kiln that genuinely is a card library — an org's shared agent + skill
/// repo — composes itself in rather than being scanned: its Lua adds the
/// directory to the path at load. The component brings itself, the host does
/// not go looking.
pub fn card_directories(roots: &CardRoots, workspace: &Path, kiln: Option<&Path>) -> Vec<PathBuf> {
    use crucible_core::runtime_path::{build_path, search_paths, PathInputs, RuntimeAsset};

    // `kiln == workspace` would otherwise offer `<kiln>/.crucible/agents`
    // twice. The kiln entry is the one kept, because a session with no
    // separate workspace is a kiln session.
    let workspace_roots = [".crucible".to_string()];
    let distinct_workspace =
        workspace
            .as_os_str()
            .is_empty()
            .then_some(None)
            .unwrap_or(if kiln == Some(workspace) {
                None
            } else {
                Some(workspace)
            });

    // `CardRoots::config_home` is the raw config dir (`dirs::config_dir()`),
    // so the `crucible` segment is added here to make it a runtime root.
    let config_root = roots.config_home.as_ref().map(|home| home.join("crucible"));

    let path = build_path(&PathInputs {
        workspace: distinct_workspace,
        workspace_roots: &workspace_roots,
        kiln,
        config_home: config_root.as_deref(),
        agent_directories: &roots.agent_directories,
        ..PathInputs::default()
    });

    search_paths(RuntimeAsset::Cards, &path)
        .into_iter()
        .map(|c| c.path)
        .collect()
}

/// Discover agent cards visible to a session (workspace + kiln), keyed by
/// card name. Best-effort: unreadable directories or invalid cards are
/// skipped (the loader warns per file).
///
/// `roots` is injected rather than read from the environment; see
/// [`CardRoots`] for why.
pub fn discover_agent_cards_in(
    roots: &CardRoots,
    workspace: &Path,
    kiln: Option<&Path>,
) -> HashMap<String, AgentCard> {
    warn_if_deprecated(roots);
    let mut cards = HashMap::new();
    let mut loader = AgentCardLoader::new();
    for dir in card_directories(roots, workspace, kiln) {
        if !dir.is_dir() {
            continue;
        }
        let Some(dir_str) = dir.to_str() else {
            continue;
        };
        match loader.load_from_directory(dir_str) {
            Ok(loaded) => {
                for card in loaded {
                    // FIRST wins. `card_directories` is highest-priority
                    // first now, like every other resolver, so an already
                    // present name must not be overwritten. This used to be
                    // an unconditional insert over a lowest-first list; the
                    // observable outcome is identical, and the three
                    // shadowing tests below prove it.
                    cards.entry(card.name.clone()).or_insert(card);
                }
            }
            Err(e) => debug!(dir = %dir.display(), error = %e, "Agent card directory skipped"),
        }
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_card(dir: &Path, file: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(file), body).unwrap();
    }

    #[test]
    fn documented_minimal_card_loads_with_defaults() {
        let kiln = TempDir::new().unwrap();
        // The doc's Basic Example shape: description + specialty + tools
        // (bool + ask forms) + mcps alias, no name/version.
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "Researcher.md",
            "---\ndescription: Explores and synthesizes knowledge\nspecialty: reasoning\ntools:\n  semantic_search: true\n  read_note: true\n  create_note: ask\nmcps:\n  - context7\n---\n\nYou are a research assistant.\n",
        );

        let cards = discover_agent_cards_in(&CardRoots::default(), kiln.path(), Some(kiln.path()));
        let card = cards.get("Researcher").expect("card named from file stem");
        assert_eq!(card.version, "0.1.0");
        assert_eq!(card.specialty.as_deref(), Some("reasoning"));
        assert_eq!(card.mcp_servers, vec!["context7".to_string()]);
        assert!(card.system_prompt.contains("research assistant"));
        let tools = card.tools.as_ref().unwrap();
        use crucible_core::agent::ToolPolicy;
        assert_eq!(tools["semantic_search"], ToolPolicy::Allow);
        assert_eq!(tools["create_note"], ToolPolicy::Ask);
    }

    #[test]
    fn full_card_fields_parse() {
        let kiln = TempDir::new().unwrap();
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "worker.md",
            "---\nname: worker\nversion: 1.2.3\ndescription: base\nmodel: llama3.2\nprovider: ollama\nmode: plan\ntools:\n  bash: deny\n---\n\nBase prompt.\n",
        );

        let cards = discover_agent_cards_in(&CardRoots::default(), kiln.path(), Some(kiln.path()));
        let card = cards.get("worker").unwrap();
        assert_eq!(card.description, "base");
        assert_eq!(card.version, "1.2.3");
        assert_eq!(card.model.as_deref(), Some("llama3.2"));
        assert_eq!(card.provider.as_deref(), Some("ollama"));
        assert_eq!(card.mode.as_deref(), Some("plan"));
        assert_eq!(
            card.tools.as_ref().unwrap()["bash"],
            crucible_core::agent::ToolPolicy::Deny
        );
    }

    #[test]
    fn project_workspace_cards_shadow_kiln_cards() {
        let kiln = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "helper.md",
            "---\ndescription: kiln helper\n---\n\nKiln prompt.\n",
        );
        write_card(
            &workspace.path().join(".crucible").join("agents"),
            "helper.md",
            "---\ndescription: project helper\n---\n\nProject prompt.\n",
        );

        let cards =
            discover_agent_cards_in(&CardRoots::default(), workspace.path(), Some(kiln.path()));
        assert_eq!(cards["helper"].description, "project helper");
    }

    /// A kiln's visible top level is not scanned for agent cards.
    ///
    /// `KILN/agents/` and `KILN/Agents/` used to be discovery paths, so any
    /// kiln that happened to contain such a directory — cloned, synced, or
    /// shared by an org — introduced agent cards into the session. A card
    /// carries a system prompt, a model and a tool policy, so that is not a
    /// passive thing to pick up. The kiln's top level is notes; Crucible reads
    /// only the `.crucible/` directory it owns.
    #[test]
    fn a_kiln_visible_agents_dir_is_not_discovered() {
        let kiln = TempDir::new().unwrap();
        for dir in ["agents", "Agents"] {
            write_card(
                &kiln.path().join(dir),
                "ambient.md",
                "---\ndescription: should not load\n---\n\nPrompt.\n",
            );
        }
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "configured.md",
            "---\ndescription: loads\n---\n\nPrompt.\n",
        );

        let cards = discover_agent_cards_in(&CardRoots::default(), kiln.path(), Some(kiln.path()));
        assert!(
            !cards.contains_key("ambient"),
            "a card in the kiln's visible tree must not load: {:?}",
            cards.keys().collect::<Vec<_>>()
        );
        assert!(
            cards.contains_key("configured"),
            "the kiln's .crucible/agents/ must still load: {:?}",
            cards.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_tool_policy_value_fails_the_card_only() {
        let kiln = TempDir::new().unwrap();
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "bad.md",
            "---\ndescription: bad tools\ntools:\n  bash: maybe\n---\n\nPrompt.\n",
        );
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "good.md",
            "---\ndescription: fine\n---\n\nPrompt.\n",
        );
        let cards = discover_agent_cards_in(&CardRoots::default(), kiln.path(), Some(kiln.path()));
        assert!(!cards.contains_key("bad"));
        assert!(cards.contains_key("good"));
    }

    /// The injected config dir supplies global cards and is the *lowest*
    /// precedence — a kiln card of the same name wins.
    #[test]
    fn injected_config_dir_supplies_global_cards_that_the_kiln_shadows() {
        let config = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        write_card(
            &config.path().join("crucible").join("agents"),
            "helper.md",
            "---\ndescription: global helper\n---\n\nGlobal prompt.\n",
        );
        write_card(
            &config.path().join("crucible").join("agents"),
            "global_only.md",
            "---\ndescription: global only\n---\n\nGlobal prompt.\n",
        );
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "helper.md",
            "---\ndescription: kiln helper\n---\n\nKiln prompt.\n",
        );

        let cards = discover_agent_cards_in(
            &CardRoots {
                config_home: Some(config.path().to_path_buf()),
                agent_directories: Vec::new(),
            },
            kiln.path(),
            Some(kiln.path()),
        );
        assert_eq!(cards["helper"].description, "kiln helper");
        assert_eq!(cards["global_only"].description, "global only");

        // And nothing global leaks in when the caller injects None.
        let cards = discover_agent_cards_in(&CardRoots::default(), kiln.path(), Some(kiln.path()));
        assert!(!cards.contains_key("global_only"));
    }
    /// A directory named in `agent_directories` supplies cards. It sits
    /// between the global cards and the kiln, so the kiln still wins.
    #[test]
    fn configured_agent_directories_supply_cards_that_the_kiln_shadows() {
        let shared = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        write_card(
            shared.path(),
            "helper.md",
            "---\ndescription: shared helper\n---\n\nShared prompt.\n",
        );
        write_card(
            shared.path(),
            "shared_only.md",
            "---\ndescription: shared only\n---\n\nShared prompt.\n",
        );
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "helper.md",
            "---\ndescription: kiln helper\n---\n\nKiln prompt.\n",
        );

        let roots = CardRoots {
            config_home: None,
            agent_directories: vec![shared.path().to_path_buf()],
        };
        let cards = discover_agent_cards_in(&roots, kiln.path(), Some(kiln.path()));
        assert_eq!(cards["helper"].description, "kiln helper");
        assert_eq!(cards["shared_only"].description, "shared only");
    }

    /// `agent_directories` comes off the serialized app config with `~`
    /// expanded against the injected home, never the environment.
    #[test]
    fn card_roots_read_agent_directories_from_the_app_config() {
        let home = Path::new("/home/tester");
        let config = serde_json::json!({
            "agent_directories": ["~/shared-agents", "/abs/agents"],
            "kiln_path": "/unrelated",
        });
        let roots = CardRoots::from_app_config(None, Some(&config), Some(home));
        assert_eq!(
            roots.agent_directories,
            vec![
                PathBuf::from("/home/tester/shared-agents"),
                PathBuf::from("/abs/agents")
            ]
        );

        let roots = CardRoots::from_app_config(None, None, Some(home));
        assert!(roots.agent_directories.is_empty());
    }

    /// A config naming only `agent_directories` still resolves its cards.
    ///
    /// The knob is deprecated, not removed. Someone who has it set must keep
    /// getting their cards until they move the directory to `runtimepath`.
    #[test]
    fn a_deprecated_agent_directory_still_supplies_its_cards() {
        let shared = TempDir::new().unwrap();
        write_card(
            shared.path(),
            "legacy.md",
            "---\ndescription: legacy card\n---\n\nPrompt.\n",
        );
        let roots = CardRoots {
            config_home: None,
            agent_directories: vec![shared.path().to_path_buf()],
        };

        let cards = discover_agent_cards_in(&roots, Path::new(""), None);
        assert_eq!(
            cards["legacy"].description, "legacy card",
            "a deprecated knob must keep working"
        );
    }

    /// Precedence order of the full list: workspace, kiln, configured, global.
    ///
    /// This is the one test the reversal inverts. It asserts the ORDER, and
    /// the order is now highest-first; the three tests above assert the
    /// OUTCOME, and they are unchanged, which is what proves the reversal did
    /// not move which card a user gets.
    #[test]
    fn card_directories_follow_the_documented_precedence() {
        let roots = CardRoots {
            config_home: Some(PathBuf::from("/cfg")),
            agent_directories: vec![PathBuf::from("/shared")],
        };
        let dirs = card_directories(&roots, Path::new("/ws"), Some(Path::new("/kiln")));
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/ws/.crucible/agents"),
                PathBuf::from("/kiln/.crucible/agents"),
                PathBuf::from("/shared"),
                PathBuf::from("/cfg/crucible/agents"),
            ]
        );
    }
}
