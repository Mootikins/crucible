//! Daemon-side agent-card discovery.
//!
//! Agent cards (markdown + YAML frontmatter, `crucible_core::agent`) define
//! specialized internal agents: a system prompt plus optional model,
//! generation knobs, tool policy, and MCP servers. The daemon discovers them
//! per session context and uses them as delegation targets and for
//! `session.create` agent resolution.
//!
//! Discovery precedence (later shadows earlier, by card name):
//! 1. `~/.config/crucible/agents/` — global personal cards
//! 2. `KILN/.crucible/agents/` — kiln config
//! 3. `WORKSPACE/.crucible/agents/` — project-scoped cards (repos)
//!
//! Only `.crucible/` directories. See [`card_directories`] for why a kiln's
//! visible top level is not scanned.
//!
//! Discovery runs per use (like skills discovery) rather than through a
//! cached registry — card sets are tiny and this avoids staleness/watchers.

use crucible_core::agent::{AgentCard, AgentCardLoader};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Candidate card directories for a session context, in precedence order
/// (later shadows earlier).
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
fn card_directories(workspace: &Path, kiln: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        dirs.push(config_dir.join("crucible").join("agents"));
    }
    if let Some(kiln) = kiln {
        dirs.push(kiln.join(".crucible").join("agents"));
    }
    if kiln != Some(workspace) {
        dirs.push(workspace.join(".crucible").join("agents"));
    }
    dirs
}

/// Discover agent cards visible to a session (workspace + kiln), keyed by
/// card name. Best-effort: unreadable directories or invalid cards are
/// skipped (the loader warns per file).
pub fn discover_agent_cards(workspace: &Path, kiln: Option<&Path>) -> HashMap<String, AgentCard> {
    let mut cards = HashMap::new();
    let mut loader = AgentCardLoader::new();
    for dir in card_directories(workspace, kiln) {
        if !dir.is_dir() {
            continue;
        }
        let Some(dir_str) = dir.to_str() else {
            continue;
        };
        match loader.load_from_directory(dir_str) {
            Ok(loaded) => {
                for card in loaded {
                    cards.insert(card.name.clone(), card);
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

        let cards = discover_agent_cards(kiln.path(), Some(kiln.path()));
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
            "---\nname: worker\nversion: 1.2.3\ndescription: base\nmodel: llama3.2\nprovider: ollama\ntemperature: 0.2\nmax_tokens: 1000\nmax_turns: 4\nmode: plan\ntools:\n  bash: deny\n---\n\nBase prompt.\n",
        );

        let cards = discover_agent_cards(kiln.path(), Some(kiln.path()));
        let card = cards.get("worker").unwrap();
        assert_eq!(card.description, "base");
        assert_eq!(card.version, "1.2.3");
        assert_eq!(card.model.as_deref(), Some("llama3.2"));
        assert_eq!(card.provider.as_deref(), Some("ollama"));
        assert_eq!(card.temperature, Some(0.2));
        assert_eq!(card.max_tokens, Some(1000));
        assert_eq!(card.max_turns, Some(4));
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

        let cards = discover_agent_cards(workspace.path(), Some(kiln.path()));
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

        let cards = discover_agent_cards(kiln.path(), Some(kiln.path()));
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
        let cards = discover_agent_cards(kiln.path(), Some(kiln.path()));
        assert!(!cards.contains_key("bad"));
        assert!(cards.contains_key("good"));
    }
}
