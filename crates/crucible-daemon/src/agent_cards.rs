//! Daemon-side agent-card discovery.
//!
//! Agent cards (markdown + YAML frontmatter, `crucible_core::agent`) define
//! specialized internal agents: a system prompt plus optional model,
//! generation knobs, tool policy, and MCP servers. The daemon discovers them
//! per session context and uses them as delegation targets and for
//! `session.create` agent resolution.
//!
//! Discovery sources, highest priority first:
//! 1. `agent_directories` from the app config, in config order
//! 2. `~/.config/crucible/agents/`. A user develops a card personally before
//!    sharing it, so these two personal layers are on top.
//! 3. `WORKSPACE/.crucible/agents/` — project-scoped cards (repos)
//! 4. `KILN/.crucible/agents/` — kiln config
//!
//! Layers override: a bare name resolves to the card of the highest layer
//! that has it. Every card also keeps its full name (`kiln:helper`). Two
//! cards of one name in the same layer are ambiguous.
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
use crucible_core::runtime_path::{build_path, search_paths, Origin, PathInputs, RuntimeAsset};
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
pub fn card_directories(roots: &CardRoots, workspace: &Path, kilns: &[PathBuf]) -> Vec<PathBuf> {
    card_sources(roots, workspace, kilns)
        .into_iter()
        .map(|(path, _, _)| path)
        .collect()
}

/// A card layer: the origin of a directory, and whether an
/// `agent_directories` entry named it. `agent_directories` and the config
/// home share `Origin::UserConfig`, but the user ranks `agent_directories`
/// above the config home, so they are two layers.
type Layer = (Origin, bool);

fn card_sources(
    roots: &CardRoots,
    workspace: &Path,
    kilns: &[PathBuf],
) -> Vec<(PathBuf, Layer, String)> {
    // `kiln == workspace` would otherwise offer `<kiln>/.crucible/agents`
    // twice. The kiln entry is the one kept, because a session with no
    // separate workspace is a kiln session.
    let workspace_roots = [".crucible".to_string()];
    let distinct_workspace =
        if workspace.as_os_str().is_empty() || kilns.iter().any(|k| k == workspace) {
            None
        } else {
            Some(workspace)
        };

    // `CardRoots::config_home` is the raw config dir (`dirs::config_dir()`),
    // so the `crucible` segment is added here to make it a runtime root.
    let config_root = roots.config_home.as_ref().map(|home| home.join("crucible"));

    let path = build_path(&PathInputs {
        workspace: distinct_workspace,
        workspace_roots: &workspace_roots,
        kilns,
        config_home: config_root.as_deref(),
        agent_directories: &roots.agent_directories,
        ..PathInputs::default()
    });

    // Cards only: the personal layer is on top. The other sources keep the
    // runtimepath order below it. `sort_by_key` is stable.
    let mut paths = search_paths(RuntimeAsset::Cards, &path);
    paths.sort_by_key(|c| c.origin != Origin::UserConfig);
    paths
        .into_iter()
        .map(|c| {
            let namespace = match c.origin {
                Origin::Workspace => "workspace".to_string(),
                Origin::Kiln => "kiln".to_string(),
                Origin::Config(index) => format!("config-{}", index + 1),
                Origin::Plugin => c
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "plugin".into()),
                Origin::Env => "env".to_string(),
                Origin::Harness => "harness".to_string(),
                Origin::UserConfig => "personal".to_string(),
                Origin::UserRuntime => "runtime".to_string(),
                Origin::Bundled => "builtin".to_string(),
            };
            let named = roots.agent_directories.contains(&c.path);
            (c.path, (c.origin, named), namespace)
        })
        .collect()
}

/// Discover agent cards visible to a session. The card of the highest layer
/// takes the bare name; each other card of that name takes its full name
/// (`kiln:helper`). Two cards of one name in the highest layer both take full
/// names, so the bare name is ambiguous. Best-effort: unreadable directories
/// or invalid cards are skipped (the loader warns per file).
///
/// `roots` is injected rather than read from the environment; see
/// [`CardRoots`] for why.
pub fn discover_agent_cards_in(
    roots: &CardRoots,
    workspace: &Path,
    kilns: &[PathBuf],
) -> HashMap<String, AgentCard> {
    warn_if_deprecated(roots);
    let mut discovered = Vec::new();
    let mut loader = AgentCardLoader::new();
    for (dir, layer, namespace) in card_sources(roots, workspace, kilns) {
        if !dir.is_dir() {
            continue;
        }
        let Some(dir_str) = dir.to_str() else {
            continue;
        };
        match loader.load_from_directory(dir_str) {
            Ok(loaded) => {
                for mut card in loaded {
                    card.namespace = Some(namespace.clone());
                    discovered.push((layer, card));
                }
            }
            Err(e) => debug!(dir = %dir.display(), error = %e, "Agent card directory skipped"),
        }
    }
    // The layer of the highest card of each name, and how many cards of that
    // name it holds. One card there takes the bare name.
    let mut top: HashMap<String, (Layer, usize)> = HashMap::new();
    for (layer, card) in &discovered {
        let entry = top.entry(card.name.clone()).or_insert((*layer, 0));
        if entry.0 == *layer {
            entry.1 += 1;
        }
    }
    let mut cards = HashMap::new();
    for (layer, card) in discovered {
        let full = |suffix: String| {
            format!(
                "{}{suffix}:{}",
                card.namespace.as_deref().unwrap_or("card"),
                card.name
            )
        };
        let mut key = match top.get(&card.name) {
            Some((top_layer, 1)) if *top_layer == layer && !cards.contains_key(&card.name) => {
                card.name.clone()
            }
            _ => full(String::new()),
        };
        let mut suffix = 2;
        while cards.contains_key(&key) {
            key = full(format!("-{suffix}"));
            suffix += 1;
        }
        cards.insert(key, card);
    }
    cards
}

/// The card `name` names: a bare name, which [`discover_agent_cards_in`]
/// gives to the card of the highest layer, or a full `namespace:name`.
pub fn resolve_card<'a>(
    cards: &'a HashMap<String, AgentCard>,
    name: &str,
) -> Result<Option<&'a AgentCard>, String> {
    if let Some(card) = cards.get(name) {
        return Ok(Some(card));
    }
    let mut matches: Vec<_> = cards
        .iter()
        .filter(|(_, card)| {
            card.name == name
                || format!(
                    "{}:{}",
                    card.namespace.as_deref().unwrap_or("card"),
                    card.name
                ) == name
        })
        .collect();
    matches.sort_by(|a, b| a.0.cmp(b.0));
    match matches.as_slice() {
        [] => Ok(None),
        [(_, card)] => Ok(Some(card)),
        _ => Err(format!(
            "Ambiguous agent card '{name}'. Use one of: {}",
            matches
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
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

        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            kiln.path(),
            &[kiln.path().to_path_buf()],
        );
        let card = cards.get("Researcher").expect("card named from file stem");
        assert_eq!(
            resolve_card(&cards, "kiln:Researcher")
                .unwrap()
                .unwrap()
                .name,
            "Researcher"
        );
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

        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            kiln.path(),
            &[kiln.path().to_path_buf()],
        );
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
    fn project_workspace_and_kiln_cards_remain_available() {
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

        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            workspace.path(),
            &[kiln.path().to_path_buf()],
        );
        let description = |name| {
            resolve_card(&cards, name)
                .unwrap()
                .unwrap()
                .description
                .clone()
        };
        assert_eq!(description("helper"), "project helper");
        assert_eq!(description("workspace:helper"), "project helper");
        assert_eq!(description("kiln:helper"), "kiln helper");
    }

    #[test]
    fn a_higher_layer_takes_the_bare_name_and_both_keep_full_names() {
        let kiln = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();
        write_card(
            &workspace.path().join(".crucible/agents"),
            "worker.md",
            "---\nname: worker\ndescription: workspace\n---\nWorkspace prompt\n",
        );
        write_card(
            &kiln.path().join(".crucible/agents"),
            "worker.md",
            "---\nname: worker\ndescription: kiln\n---\nKiln prompt\n",
        );
        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            workspace.path(),
            &[kiln.path().to_path_buf()],
        );
        assert_eq!(cards.len(), 2);
        assert_eq!(cards["worker"].description, "workspace");
        assert_eq!(cards["kiln:worker"].description, "kiln");
        assert_eq!(
            resolve_card(&cards, "worker").unwrap().unwrap().description,
            "workspace"
        );
        assert_eq!(
            resolve_card(&cards, "workspace:worker")
                .unwrap()
                .unwrap()
                .description,
            "workspace"
        );
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

        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            kiln.path(),
            &[kiln.path().to_path_buf()],
        );
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
        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            kiln.path(),
            &[kiln.path().to_path_buf()],
        );
        assert!(!cards.contains_key("bad"));
        assert!(cards.contains_key("good"));
    }

    /// The injected config dir supplies the personal cards.
    #[test]
    fn injected_config_dir_and_kiln_cards_remain_available() {
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
            &[kiln.path().to_path_buf()],
        );
        assert_eq!(cards["kiln:helper"].description, "kiln helper");
        assert_eq!(cards["helper"].description, "global helper");
        assert_eq!(cards["global_only"].description, "global only");

        // And nothing global leaks in when the caller injects None.
        let cards = discover_agent_cards_in(
            &CardRoots::default(),
            kiln.path(),
            &[kiln.path().to_path_buf()],
        );
        assert!(!cards.contains_key("global_only"));
    }
    /// Layers override, and the personal layer is on top: a user develops a
    /// card personally before sharing it in a kiln. A bare name resolves to
    /// the personal card; each full name reaches its own card.
    #[test]
    fn a_personal_card_outranks_a_kiln_card_of_the_same_name() {
        let config = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        write_card(
            &config.path().join("crucible").join("agents"),
            "helper.md",
            "---\ndescription: personal helper\n---\n\nPersonal prompt.\n",
        );
        write_card(
            &kiln.path().join(".crucible").join("agents"),
            "helper.md",
            "---\ndescription: kiln helper\n---\n\nKiln prompt.\n",
        );
        let roots = CardRoots {
            config_home: Some(config.path().to_path_buf()),
            agent_directories: Vec::new(),
        };
        let cards = discover_agent_cards_in(&roots, kiln.path(), &[kiln.path().to_path_buf()]);
        let description = |name| {
            resolve_card(&cards, name)
                .unwrap()
                .unwrap()
                .description
                .clone()
        };
        assert_eq!(description("helper"), "personal helper");
        assert_eq!(description("personal:helper"), "personal helper");
        assert_eq!(description("kiln:helper"), "kiln helper");
        assert_eq!(
            card_directories(&roots, kiln.path(), &[kiln.path().to_path_buf()])[0],
            config.path().join("crucible").join("agents")
        );
    }

    /// An `agent_directories` entry outranks `~/.config/crucible/agents/`.
    /// A name in both is not ambiguous: the bare name resolves to the
    /// `agent_directories` card, and the full name reaches the other card.
    #[test]
    fn an_agent_directories_card_outranks_a_config_home_card() {
        let config = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_card(
            &config.path().join("crucible").join("agents"),
            "helper.md",
            "---\ndescription: home\n---\n\nPrompt.\n",
        );
        write_card(
            shared.path(),
            "helper.md",
            "---\ndescription: shared\n---\n\nPrompt.\n",
        );
        let roots = CardRoots {
            config_home: Some(config.path().to_path_buf()),
            agent_directories: vec![shared.path().to_path_buf()],
        };
        let cards = discover_agent_cards_in(&roots, Path::new(""), &[]);
        let description = |name| {
            resolve_card(&cards, name)
                .unwrap()
                .unwrap()
                .description
                .clone()
        };
        assert_eq!(description("helper"), "shared");
        assert_eq!(description("personal:helper"), "home");
    }

    /// Two cards of one name in ONE layer are ambiguous: the bare name is
    /// refused, and the error lists the full names.
    #[test]
    fn two_cards_of_one_name_in_one_layer_are_ambiguous() {
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        for (dir, text) in [
            (first.path().to_path_buf(), "first"),
            (second.path().to_path_buf(), "second"),
            (kiln.path().join(".crucible").join("agents"), "kiln"),
        ] {
            write_card(
                &dir,
                "helper.md",
                &format!("---\ndescription: {text}\n---\n\nPrompt.\n"),
            );
        }
        let roots = CardRoots {
            config_home: None,
            agent_directories: vec![first.path().to_path_buf(), second.path().to_path_buf()],
        };
        let cards = discover_agent_cards_in(&roots, kiln.path(), &[kiln.path().to_path_buf()]);
        let error = resolve_card(&cards, "helper").unwrap_err();
        assert!(
            error.contains("personal:helper") && error.contains("personal-2:helper"),
            "{error}"
        );
        assert_eq!(
            resolve_card(&cards, "kiln:helper")
                .unwrap()
                .unwrap()
                .description,
            "kiln"
        );
    }

    /// A directory named in `agent_directories` supplies personal cards.
    #[test]
    fn configured_agent_directories_and_kiln_cards_remain_available() {
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
        let cards = discover_agent_cards_in(&roots, kiln.path(), &[kiln.path().to_path_buf()]);
        assert_eq!(cards["kiln:helper"].description, "kiln helper");
        assert!(cards
            .iter()
            .any(|(key, card)| key != "kiln:helper" && card.description == "shared helper"));
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

        let cards = discover_agent_cards_in(&roots, Path::new(""), &[]);
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
        let dirs = card_directories(&roots, Path::new("/ws"), &[PathBuf::from("/kiln")]);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/shared"),
                PathBuf::from("/cfg/crucible/agents"),
                PathBuf::from("/ws/.crucible/agents"),
                PathBuf::from("/kiln/.crucible/agents"),
            ]
        );
    }
}
