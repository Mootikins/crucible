//! All Bases mutations use one policy stage and one post-commit notification.
use super::*;
use crate::agent_manager::PluginHandlers;
use crucible_core::events::SessionEvent;
use crucible_lua::{Firing, ScriptHandlerResult, StageId};

/// Run the `base:before_write` handlers. `Some(reason)` when one cancels the
/// write; an error when one fails, which refuses the write too.
pub(super) async fn before(
    handlers: Option<PluginHandlers>,
    session: Option<&str>,
    payload: serde_json::Value,
) -> Result<Option<String>> {
    let Some((registry, lua)) = handlers else {
        return Ok(None);
    };
    let stage = StageId::BaseBeforeWrite;
    let event = SessionEvent::Custom {
        name: stage.as_str().into(),
        payload: payload.clone(),
    };
    for handler in registry.runtime_handlers_for(
        stage.as_str(),
        payload["path"].as_str(),
        Firing::of(session),
    ) {
        match registry
            .execute_runtime_handler(&lua, handler.id, &event, session)
            .await?
        {
            ScriptHandlerResult::PassThrough => {}
            ScriptHandlerResult::Cancel { reason } => {
                return Ok(Some(format!("Bases policy refused: {reason}")))
            }
            _ => anyhow::bail!("Bases policy must return nil or cancel"),
        }
    }
    Ok(None)
}
