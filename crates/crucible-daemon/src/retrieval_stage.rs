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
use crucible_core::parser::BlockHash;
use crucible_core::storage::BlockRecord;
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

/// Fire `index:blocks` over a note's block rows and return the extra rows a
/// handler added, checked and ready to write beside `records`.
///
/// An extra row must name a kind [`BlockKind::STORED_NAMES`] lists, sit
/// inside the note, start where no other row starts, and carry a vector of
/// the note's own dimension. The model name comes from the note's embedded
/// blocks, so a note with none admits no extra rows. Rows that fail a check
/// are dropped with a warning, never the whole write.
pub async fn index_blocks_extra_rows(
    vm: &StageVm,
    kiln_name: Option<&crucible_core::config::KilnName>,
    note_path: &str,
    records: &[BlockRecord],
) -> Vec<BlockRecord> {
    let vms = std::slice::from_ref(vm);
    if !has_handlers(StageId::IndexBlocks, vms) {
        return Vec::new();
    }

    let blocks: Vec<serde_json::Value> = records
        .iter()
        .map(|record| {
            let mut entry = serde_json::Map::new();
            entry.insert("span_start".into(), serde_json::json!(record.span_start));
            entry.insert("span_end".into(), serde_json::json!(record.span_end));
            entry.insert("kind".into(), serde_json::json!(record.kind));
            if let Some(vector) = record.embedding.as_ref() {
                entry.insert("vector".into(), serde_json::json!(vector));
            }
            serde_json::Value::Object(entry)
        })
        .collect();
    let mut payload = serde_json::Map::new();
    if let Some(name) = kiln_name {
        payload.insert("kiln".into(), serde_json::json!(name.as_str()));
    }
    payload.insert("path".into(), serde_json::json!(note_path));
    payload.insert("blocks".into(), serde_json::Value::Array(blocks));
    let event = SessionEvent::Custom {
        name: StageId::IndexBlocks.as_str().to_string(),
        payload: serde_json::Value::Object(payload),
    };

    first_usable_transform(StageId::IndexBlocks, vms, None, &event, |value| {
        let extra = value.get("extra")?;
        Some(admit_extra_rows(note_path, records, &lua_array(extra)?))
    })
    .await
    .unwrap_or_default()
}

/// The subset of `extra` that passes every check, as rows of `note_path`.
fn admit_extra_rows(
    note_path: &str,
    records: &[BlockRecord],
    extra: &[serde_json::Value],
) -> Vec<BlockRecord> {
    use crucible_core::parser::types::BlockKind;

    let note_end = records.iter().map(|r| r.span_end).max().unwrap_or(0);
    let reference = records
        .iter()
        .find_map(|r| Some((r.embedding_model.clone()?, r.embedding_dimensions?)));
    let mut starts: std::collections::HashSet<usize> =
        records.iter().map(|r| r.span_start).collect();
    let mut admitted = Vec::new();

    for entry in extra {
        let span_start = entry.get("span_start").and_then(|v| v.as_u64());
        let span_end = entry.get("span_end").and_then(|v| v.as_u64());
        let kind = entry.get("kind").and_then(|v| v.as_str());
        let vector: Option<Vec<f32>> = entry.get("vector").and_then(lua_array).map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_f64())
                .map(|f| f as f32)
                .collect()
        });

        let (Some(span_start), Some(span_end), Some(kind), Some(vector)) =
            (span_start, span_end, kind, vector)
        else {
            warn!(
                note_path,
                "index:blocks extra row lacks span, kind or vector; dropping"
            );
            continue;
        };
        let (span_start, span_end) = (span_start as usize, span_end as usize);
        if !BlockKind::STORED_NAMES.contains(&kind) {
            warn!(
                note_path,
                kind, "index:blocks extra row names an unknown kind; dropping"
            );
            continue;
        }
        if span_start > span_end || span_end > note_end {
            warn!(
                note_path,
                span_start, span_end, "index:blocks extra row is not inside the note; dropping"
            );
            continue;
        }
        let Some((model, dimensions)) = reference.as_ref() else {
            warn!(
                note_path,
                "index:blocks extra row on a note with no embedded block; dropping"
            );
            continue;
        };
        if vector.len() != *dimensions as usize {
            warn!(
                note_path,
                span_start,
                dimensions,
                got = vector.len(),
                "index:blocks extra row has a vector of another dimension; dropping"
            );
            continue;
        }
        if !starts.insert(span_start) {
            warn!(
                note_path,
                span_start, "index:blocks extra row starts where a row already starts; dropping"
            );
            continue;
        }

        let text = match entry.get("text").and_then(|v| v.as_str()) {
            Some(text) => text.to_string(),
            // Every stored block the span touches, so a row across two
            // blocks quotes both.
            None => records
                .iter()
                .filter(|r| r.span_end > span_start && r.span_start < span_end)
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        };
        admitted.push(BlockRecord {
            note_path: note_path.to_string(),
            span_start,
            span_end,
            kind: kind.to_string(),
            content_hash: BlockHash::new(*blake3::hash(text.as_bytes()).as_bytes()),
            text,
            embedding_dimensions: Some(*dimensions),
            embedding: Some(vector),
            embedding_model: Some(model.clone()),
        });
    }
    admitted
}
