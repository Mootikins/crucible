//! The command catalog of a session, and the routing of a `/name` message.
//!
//! The catalog joins five sources in one order: the built-in commands, the
//! session's modes, plugin commands, skills, and the commands that the ACP
//! agent advertises. When two sources give one name, the earlier source keeps
//! it, so a plugin cannot hide `/help` and an agent cannot hide a plugin. The
//! routing reads the same catalog, so a client never completes a command that
//! the daemon then routes to a different source.

use crucible_core::types::{split_slash_command, BuiltinCommand, CommandKind, SessionCommand};
use strum::IntoEnumIterator;

use super::attachments::AttachedContext;
use super::{AgentError, AgentManager};

/// The tag and injection kind of a skill's instructions in the context.
pub(crate) const SKILL_KIND: &str = "skill";

/// What the daemon does with a message, after it reads the command name.
#[derive(Debug)]
pub(crate) enum SlashRoute {
    /// Send the text as it is. The text names no command that the daemon
    /// runs: it is plain text, a built-in command, or an agent command,
    /// which the agent reads from the prompt.
    Message,
    /// Switch the session to this mode, then send `rest` when it is not empty.
    Mode { mode_id: String, rest: String },
    /// Run this plugin command with `rest` as its input.
    Plugin { name: String, rest: String },
    /// Send the text with the skill's instructions attached to the turn.
    Skill { instructions: AttachedContext },
}

impl AgentManager {
    /// Every command of the session, in the order of its sources.
    ///
    /// The read brings the agent up, as the mode list does: an ACP agent
    /// advertises its commands after the handshake, and a resumed session
    /// must not answer without them.
    pub async fn session_commands(
        &self,
        session_id: &str,
        event_tx: Option<&crate::EventBus>,
    ) -> Result<Vec<SessionCommand>, AgentError> {
        let session = self
            .session_manager
            .read_session(session_id)
            .await?
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        if session.agent.is_some() {
            if let Err(e) = self.ensure_agent_handle(session_id, event_tx).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "The agent did not come up for the command list"
                );
            }
        }

        let modes = self
            .session_modes(session_id)
            .available_modes
            .into_iter()
            .map(|mode| SessionCommand {
                name: mode.id.0.to_string(),
                description: mode
                    .description
                    .unwrap_or_else(|| format!("Switch to the {} mode", mode.name)),
                input_hint: None,
                kind: CommandKind::Mode {
                    mode_id: mode.id.0.to_string(),
                },
            });

        let plugins = match self.plugin_registry().await {
            Some(registry) => registry
                .commands_json()
                .into_iter()
                .filter_map(|command| {
                    Some(SessionCommand {
                        name: command["name"].as_str()?.to_string(),
                        description: command["description"].as_str().unwrap_or("").to_string(),
                        input_hint: command["hint"].as_str().map(str::to_string),
                        kind: CommandKind::Plugin {
                            plugin: command["plugin"].as_str().unwrap_or("").to_string(),
                        },
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        let mut skills: Vec<SessionCommand> = self
            .session_skills(&session)
            .into_iter()
            .map(|(name, resolved)| SessionCommand {
                name,
                description: resolved.skill.description,
                input_hint: None,
                kind: CommandKind::Skill,
            })
            .collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));

        let agent = self
            .slot(session_id)
            .agent_surface()
            .commands
            .map(|commands| commands.borrow().clone())
            .unwrap_or_default();

        let mut catalog: Vec<SessionCommand> = Vec::new();
        let all = BuiltinCommand::iter()
            .map(BuiltinCommand::entry)
            .chain(modes)
            .chain(plugins)
            .chain(skills)
            .chain(agent);
        for command in all {
            if catalog.iter().any(|kept| kept.name == command.name) {
                tracing::debug!(
                    name = %command.name,
                    kind = ?command.kind,
                    "An earlier source keeps this command name"
                );
                continue;
            }
            catalog.push(command);
        }
        Ok(catalog)
    }

    /// The route of `content`, from the session's catalog.
    pub(crate) async fn slash_route(
        &self,
        session_id: &str,
        content: &str,
        event_tx: &crate::EventBus,
    ) -> Result<SlashRoute, AgentError> {
        let Some((name, rest)) = split_slash_command(content) else {
            return Ok(SlashRoute::Message);
        };
        let mut catalog = self.session_commands(session_id, Some(event_tx)).await?;
        // The exact name first. Then a name in another case, so `/review`
        // reaches a mode declared as `Review`.
        let position = catalog
            .iter()
            .position(|command| command.name == name)
            .or_else(|| {
                catalog
                    .iter()
                    .position(|command| command.name.eq_ignore_ascii_case(name))
            });
        let Some(command) = position.map(|i| catalog.swap_remove(i)) else {
            return Ok(SlashRoute::Message);
        };
        let rest = rest.to_string();
        Ok(match command.kind {
            CommandKind::Builtin { .. } | CommandKind::Agent => SlashRoute::Message,
            CommandKind::Mode { mode_id } => SlashRoute::Mode { mode_id, rest },
            CommandKind::Plugin { .. } => SlashRoute::Plugin {
                name: command.name,
                rest,
            },
            CommandKind::Skill => {
                let session = self
                    .session_manager
                    .read_session(session_id)
                    .await?
                    .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
                // The catalog lists each skill under its discovery key. A
                // skill that left its folder since then is text again.
                let Some(skill) = self.session_skills(&session).remove(&command.name) else {
                    return Ok(SlashRoute::Message);
                };
                SlashRoute::Skill {
                    instructions: AttachedContext {
                        kind: SKILL_KIND,
                        source: command.name.clone(),
                        body: format!(
                            "The user invoked the skill `{}`. Follow its instructions:\n\n{}",
                            command.name,
                            crate::skills::skill_instructions(&command.name, &skill.skill)
                        ),
                    },
                }
            }
        })
    }

    /// The skills that the session can use, keyed by the name it lists them
    /// under. Discovery reads the same paths as the agent's skill catalog.
    /// A discovery failure gives no skills, because the other commands must
    /// still work.
    fn session_skills(
        &self,
        session: &crucible_core::session::Session,
    ) -> std::collections::HashMap<String, crate::skills::ResolvedSkill> {
        let workspace =
            super::scope::session_tool_root(session, self.session_manager.sessions_root());
        let kilns = self.session_manager.kiln_paths(&session.kilns);
        crate::skills::FolderDiscovery::with_default_paths(&self.source_roots, &workspace, &kilns)
            .discover()
            .unwrap_or_else(|e| {
                tracing::warn!("Skill discovery failed; the session lists no skills: {e}");
                Default::default()
            })
    }
}
