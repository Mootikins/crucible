//! Daemon-side agent-card discovery.
//!
//! Agent cards (markdown + YAML frontmatter, `crucible_core::agent`) define
//! specialized internal agents: a system prompt plus optional model,
//! generation knobs, tool policy, and MCP servers. The daemon discovers them
//! per session context and uses them as delegation targets and for
//! `session.create` agent resolution.
//!
//! Discovery sources, highest priority first (see `crucible_core::sources`):
//! 1. `agent_directories` from the app config (`agent-dir-N`), then
//!    `~/.config/crucible/agents/` (`personal`). A user develops a card
//!    personally before sharing it, so the personal sources are on top.
//! 2. `WORKSPACE/.crucible/agents/` (`workspace`) — project-scoped cards
//! 3. `KILN/.crucible/agents/` (`kiln`, `kiln-2`, ...) — kiln config
//!
//! A bare name resolves to the card of the highest source that has it.
//! Every card also keeps its full name (`kiln:helper`). Two cards of one
//! name at one priority are ambiguous.
//!
//! Only `.crucible/` directories. See [`card_directories`] for why a kiln's
//! visible top level is not scanned.
//!
//! Discovery runs per use (like skills discovery) rather than through a
//! cached registry — card sets are tiny and this avoids staleness/watchers.
//!
//! The CLI (`cru agents`) reads cards off disk through [`card_directories`]
//! too, so the two never disagree about where a card may come from.

use crate::runtime_path::SourceRoots;
use crucible_core::agent::{AgentCard, AgentCardLoader};
use crucible_core::runtime_path::{
    build_path, search_sources, PathInputs, RuntimeAsset, SearchPath,
};
use crucible_core::sources::{Entry, Sources};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Warn once that `agent_directories` is superseded by `runtimepath`.
///
/// Once per process, not per session: card discovery runs per use, and a line
/// per turn would be noise rather than guidance.
pub fn warn_if_deprecated(roots: &SourceRoots) {
    use std::sync::Once;
    static WARNED: Once = Once::new();
    if roots.agent_directories.is_empty() {
        return;
    }
    WARNED.call_once(|| {
        tracing::warn!(directories = ?roots.agent_directories, "{DEPRECATION_ADVICE}");
    });
}

/// The advice [`warn_if_deprecated`] gives. It names only a directory that
/// card discovery reads today.
const DEPRECATION_ADVICE: &str = "`agent_directories` is deprecated. \
     To keep a card, move it to ~/.config/crucible/agents/ \
     or to <project>/.crucible/agents/.";

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
pub fn card_directories(roots: &SourceRoots, workspace: &Path, kilns: &[PathBuf]) -> Vec<PathBuf> {
    card_sources(roots, workspace, kilns)
        .list()
        .iter()
        .map(|source| source.value.path.clone())
        .collect()
}

/// The card directories as sources, sorted by priority: the personal
/// sources (`agent_directories`, then the config home), the workspace, then
/// each kiln.
fn card_sources(roots: &SourceRoots, workspace: &Path, kilns: &[PathBuf]) -> Sources<SearchPath> {
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

    // `SourceRoots::config_home` is the raw config dir (`dirs::config_dir()`),
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
    search_sources(RuntimeAsset::Cards, &path).unwrap_or_else(|error| {
        warn!(%error, "Agent card directories are invalid; no cards load");
        Sources::default()
    })
}

/// Discover agent cards visible to a session, keyed by the name each is
/// listed under. The card of the highest source takes the bare name; each
/// other card takes its full name (`kiln:helper`). Two cards of one name at
/// one priority both take full names, so the bare name is ambiguous.
/// Best-effort: unreadable directories or invalid cards are skipped (the
/// loader warns per file).
///
/// `roots` is injected rather than read from the environment; see
/// [`SourceRoots`] for why.
pub fn discover_agent_cards_in(
    roots: &SourceRoots,
    workspace: &Path,
    kilns: &[PathBuf],
) -> HashMap<String, AgentCard> {
    warn_if_deprecated(roots);
    let sources = card_sources(roots, workspace, kilns);
    let mut entries: Vec<Entry<AgentCard>> = Vec::new();
    let mut loader = AgentCardLoader::new();
    for (index, source) in sources.list().iter().enumerate() {
        let dir = &source.value.path;
        if !dir.is_dir() {
            continue;
        }
        let Some(dir_str) = dir.to_str() else {
            continue;
        };
        match loader.load_from_directory(dir_str) {
            Ok(loaded) => {
                for mut card in loaded {
                    // One name twice in one directory is the directory's
                    // defect, not a tie between sources.
                    if entries
                        .iter()
                        .any(|e| e.source == index && e.name == card.name)
                    {
                        warn!(dir = %dir.display(), card = %card.name, "Second agent card of one name skipped");
                        continue;
                    }
                    card.namespace = Some(source.name.clone());
                    entries.push(Entry {
                        source: index,
                        name: card.name.clone(),
                        value: card,
                    });
                }
            }
            Err(e) => debug!(dir = %dir.display(), error = %e, "Agent card directory skipped"),
        }
    }
    let keys = crucible_core::sources::listing(&sources, &entries);
    let mut cards: Vec<Option<AgentCard>> = entries.into_iter().map(|e| Some(e.value)).collect();
    keys.into_iter()
        .filter_map(|(key, index)| Some((key, cards[index].take()?)))
        .collect()
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
    /// `runtimepath` does not reach cards, so the advice must not name it.
    #[test]
    fn the_deprecation_advice_names_a_card_directory_that_is_read() {
        assert!(!super::DEPRECATION_ADVICE.contains("runtimepath"));
        assert!(super::DEPRECATION_ADVICE.contains("~/.config/crucible/agents/"));
    }

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
            &SourceRoots::default(),
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
            &SourceRoots::default(),
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
            &SourceRoots::default(),
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
            &SourceRoots::default(),
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
            &SourceRoots::default(),
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
            &SourceRoots::default(),
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
            &SourceRoots {
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
            &SourceRoots::default(),
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
        let roots = SourceRoots {
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
        let roots = SourceRoots {
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
        assert_eq!(
            description("agent-dir-1:helper"),
            "shared",
            "an agent_directories card has a full name of its own"
        );
    }

    /// Two cards of one name at ONE priority are ambiguous: the bare name
    /// is refused, and the error lists the full names. Each
    /// `agent_directories` entry is a source of its own.
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
        let roots = SourceRoots {
            config_home: None,
            agent_directories: vec![first.path().to_path_buf(), second.path().to_path_buf()],
        };
        let cards = discover_agent_cards_in(&roots, kiln.path(), &[kiln.path().to_path_buf()]);
        let error = resolve_card(&cards, "helper").unwrap_err();
        assert!(
            error.contains("agent-dir-1:helper") && error.contains("agent-dir-2:helper"),
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

        let roots = SourceRoots {
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
        let roots = SourceRoots {
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
        let roots = SourceRoots {
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
