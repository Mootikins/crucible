//! The two retrieval stages Lua can transform: `search:rerank` over the
//! merged hits of a search, and `index:blocks` over a note's block rows
//! before the pipeline writes them.
//!
//! Both are decisions, not patches: the first usable Transform wins, the
//! session VM (when the caller has one) runs before the plugin VM, and a
//! handler that errors leaves the Rust default in place. The search path
//! outside precognition has no session VM, and the index pipeline never has
//! one, so those callers hand over the plugin VM alone.

use crucible_core::events::SessionEvent;
use crucible_lua::{LuaScriptHandlerRegistry, ScriptHandlerResult, StageId};
use mlua::Lua;
use std::sync::{Arc, OnceLock};
use tracing::warn;

/// One VM a stage may reach: a handler registry with the `Lua` state that
/// owns its functions. Both halves are `Arc`-backed and cheap to clone.
pub type StageVm = (LuaScriptHandlerRegistry, Lua);

/// The plugin VM, bound once at daemon boot and read by every kiln pipeline
/// created before or after that bind.
pub type SharedStageVm = Arc<OnceLock<StageVm>>;

/// Run `stage` over `vms` in order and return what the first usable
/// Transform maps to under `apply`.
///
/// `apply` says whether a Transform is usable: it returns `None` for a
/// return value the stage cannot read, and the pass moves on to the next
/// handler with a warning. `PassThrough`, `Cancel`, `Inject` and `Handled`
/// mean nothing at these stages and are skipped.
pub async fn first_usable_transform<T>(
    stage: StageId,
    vms: &[StageVm],
    session_id: Option<&str>,
    event: &SessionEvent,
    mut apply: impl FnMut(&serde_json::Value) -> Option<T>,
) -> Option<T> {
    for (registry, lua) in vms {
        for handler in registry.runtime_handlers_for(stage.as_str(), None) {
            match registry
                .execute_runtime_handler(lua, &handler.name, event, session_id)
                .await
            {
                Ok(ScriptHandlerResult::Transform(value)) => {
                    if let Some(applied) = apply(&value) {
                        return Some(applied);
                    }
                    warn!(
                        stage = stage.as_str(),
                        handler = %handler.name,
                        "handler returned a value the stage cannot read; ignoring"
                    );
                }
                Ok(ScriptHandlerResult::PassThrough)
                | Ok(ScriptHandlerResult::Cancel { .. })
                | Ok(ScriptHandlerResult::Inject { .. })
                | Ok(ScriptHandlerResult::Handled { .. }) => {}
                Err(error) => {
                    warn!(
                        stage = stage.as_str(),
                        handler = %handler.name,
                        error = %error,
                        "handler error (fail-open)"
                    );
                }
            }
        }
    }
    None
}

/// The entries of a Lua array a handler returned.
///
/// A top-level table crosses as a JSON object keyed `"1"`, `"2"`, … (nested
/// tables cross as arrays), so both shapes are read. An empty table is an
/// empty list; a table with no numeric key is not a list at all.
pub fn lua_array(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    if let Some(array) = value.as_array() {
        return Some(array.clone());
    }

    let map = value.as_object()?;
    if map.is_empty() {
        return Some(Vec::new());
    }

    let mut keyed: Vec<(u64, serde_json::Value)> = map
        .iter()
        .filter_map(|(key, value)| key.parse::<u64>().ok().map(|k| (k, value.clone())))
        .collect();

    if keyed.is_empty() {
        return None;
    }

    keyed.sort_by_key(|(key, _)| *key);
    Some(keyed.into_iter().map(|(_, value)| value).collect())
}

/// Whether any VM has a handler at `stage`. A search over-fetches only when
/// something will rerank the extra rows.
pub fn has_handlers(stage: StageId, vms: &[StageVm]) -> bool {
    vms.iter().any(|(registry, _)| {
        !registry
            .runtime_handlers_for(stage.as_str(), None)
            .is_empty()
    })
}
