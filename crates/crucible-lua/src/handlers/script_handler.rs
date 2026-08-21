use mlua::{Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

use super::conversion::lua_table_to_json;
/// Result of script handler execution
///
/// Represents the possible outcomes from a Lua handler function:
/// - Transform: Handler returned a modified event table (as JSON for cross-boundary safety)
/// - PassThrough: Handler returned nil (no changes)
/// - Cancel: Handler returned `{cancel=true, reason="..."}` to abort
/// - Inject: Handler wants to inject a follow-up message
/// - Handled: Handler fully handled the event and provides the result directly
#[derive(Debug, Clone)]
pub enum ScriptHandlerResult {
    /// Handler returned modified event - continue with changes
    /// Stored as JSON to avoid Lua value lifetime issues
    Transform(JsonValue),
    /// Handler returned nil - pass through unchanged
    PassThrough,
    /// Handler returned cancel object - abort pipeline
    Cancel { reason: String },
    /// Handler wants to inject a follow-up message
    Inject {
        /// Content to inject
        content: String,
        /// Where to inject: "user_prefix" (default), "user_suffix"
        position: String,
    },
    /// Handler fully handled the event — use this result instead of default execution.
    /// Returned when Lua handler returns `{ handled = true, result = ..., terminate = bool }`.
    ///
    /// `terminate=true` signals the agent loop should end the turn after this tool
    /// batch (conjunctive across batch: only stops if every tool result in the batch
    /// sets terminate=true).
    Handled { result: JsonValue, terminate: bool },
}

/// Interpret the return value from a Lua handler function
///
/// Implements the neovim-style return conventions:
/// - nil → PassThrough
/// - table with `inject={content="...", position="..."}` → Inject
/// - table with `cancel=true` → Cancel
/// - table without `cancel` or `inject` → Transform
/// - other → Transform (treat as modified value)
pub fn interpret_handler_result(result: &Value) -> LuaResult<ScriptHandlerResult> {
    match result {
        Value::Nil => Ok(ScriptHandlerResult::PassThrough),
        Value::Table(t) => {
            // Directives only apply to directive-SHAPED returns. Flat events
            // carry the envelope `type` key, so a handler returning the event
            // table (a documented transform pattern) must stay a transform
            // even when the event's payload contains `cancel`/`handled`/
            // `inject` fields — otherwise a payload key silently becomes a
            // cancellation.
            let is_directive_shape = !t.contains_key("type").unwrap_or(false);
            if is_directive_shape {
                // {inject={content="...", position="..."}}
                if let Ok(inject_table) = t.get::<Table>("inject") {
                    let content = inject_table.get::<String>("content")?;
                    let position = inject_table
                        .get::<String>("position")
                        .unwrap_or_else(|_| "user_prefix".to_string());
                    return Ok(ScriptHandlerResult::Inject { content, position });
                }
                // {handled=true, result=..., terminate=bool}
                if let Ok(true) = t.get::<bool>("handled") {
                    let result = match lua_table_to_json(t) {
                        Ok(json) => json.get("result").cloned().unwrap_or(JsonValue::Null),
                        Err(_) => JsonValue::Null,
                    };
                    let terminate = t.get::<bool>("terminate").unwrap_or(false);
                    return Ok(ScriptHandlerResult::Handled { result, terminate });
                }
                // {cancel=true, reason="..."}
                if t.get::<bool>("cancel").unwrap_or(false) {
                    let reason = t
                        .get::<String>("reason")
                        .unwrap_or_else(|_| "cancelled".to_string());
                    return Ok(ScriptHandlerResult::Cancel { reason });
                }
            }
            // Anything else is a transform
            let json = lua_table_to_json(t)?;
            Ok(ScriptHandlerResult::Transform(json))
        }
        _ => {
            // Other values treated as transform - convert to JSON
            warn!("Handler returned unexpected type, treating as transform");
            let json = serde_json::to_value(result).map_err(mlua::Error::external)?;
            Ok(ScriptHandlerResult::Transform(json))
        }
    }
}

/// What a handler for a broadcast [`EventName`](super::EventName) may ask for.
///
/// Two variants, because an event has already happened and already been sent
/// before any handler runs. There is nothing downstream to rewrite and nothing
/// left to take over, so [`ScriptHandlerResult`]'s `Transform`, `Inject` and
/// `Handled` have no meaning here.
///
/// They used to be *representable* here anyway, and `file_event_hooks.rs`
/// carried a match arm that logged and dropped them at runtime. That is the
/// gap this closes: `EventName` and `StageId` split the two contracts by name,
/// and this splits them by type, so the event dispatch loop can be exhaustive
/// over what an event can actually do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    /// The handler observed the event. The next one runs.
    Observed,
    /// Stop running the remaining handlers.
    ///
    /// Not "cancel the event" — it already happened and was already broadcast.
    /// `Event Hooks.md` gives every chain a way to stop, and this event class
    /// has handlers to stop even though it has no pipeline to abort.
    StopChain {
        /// Why the handler stopped the chain, for the log.
        reason: String,
    },
}

impl ScriptHandlerResult {
    /// Narrow a handler's return value to what an event can act on.
    ///
    /// `dropped` is called with a short description of any return value that
    /// cannot apply, so the caller decides how loudly to say so rather than
    /// this deciding for it. An author who writes `return { handled = true }`
    /// in a `note:created` handler has misunderstood the contract and should
    /// hear about it.
    #[must_use]
    pub fn into_event_outcome(self, dropped: &mut dyn FnMut(&str)) -> EventOutcome {
        match self {
            Self::PassThrough => EventOutcome::Observed,
            Self::Cancel { reason } => EventOutcome::StopChain { reason },
            Self::Transform(_) => {
                dropped("a transformed event; nothing downstream reads it");
                EventOutcome::Observed
            }
            Self::Inject { .. } => {
                dropped("an injection; an event has no turn to inject into");
                EventOutcome::Observed
            }
            Self::Handled { .. } => {
                dropped("a replacement result; an event has no execution to replace");
                EventOutcome::Observed
            }
        }
    }
}

#[cfg(test)]
mod event_outcome_tests {
    use super::*;

    fn narrow(result: ScriptHandlerResult) -> (EventOutcome, Vec<String>) {
        let mut dropped = Vec::new();
        let outcome = result.into_event_outcome(&mut |d| dropped.push(d.to_string()));
        (outcome, dropped)
    }

    #[test]
    fn nil_observes_and_drops_nothing() {
        let (outcome, dropped) = narrow(ScriptHandlerResult::PassThrough);
        assert_eq!(outcome, EventOutcome::Observed);
        assert!(dropped.is_empty());
    }

    #[test]
    fn cancel_stops_the_chain_and_keeps_its_reason() {
        let (outcome, dropped) = narrow(ScriptHandlerResult::Cancel {
            reason: "enough".into(),
        });
        assert_eq!(
            outcome,
            EventOutcome::StopChain {
                reason: "enough".into()
            }
        );
        assert!(dropped.is_empty());
    }

    /// The three that have no meaning for an event still let the chain run, and
    /// each one says what it dropped.
    ///
    /// Silence was the old behaviour's real defect: an author who wrote
    /// `return { handled = true }` in a `note:created` handler got no signal
    /// that the contract does not work that way.
    #[test]
    fn a_return_value_an_event_cannot_act_on_is_reported() {
        for result in [
            ScriptHandlerResult::Transform(serde_json::json!({"a": 1})),
            ScriptHandlerResult::Inject {
                content: "hi".into(),
                position: "user_prefix".into(),
            },
            ScriptHandlerResult::Handled {
                result: serde_json::json!("done"),
                terminate: false,
            },
        ] {
            let (outcome, dropped) = narrow(result);
            assert_eq!(
                outcome,
                EventOutcome::Observed,
                "an event a handler cannot change still runs the next handler"
            );
            assert_eq!(dropped.len(), 1, "exactly one report per dropped value");
            assert!(
                !dropped[0].is_empty(),
                "the report must say what was dropped"
            );
        }
    }
}
