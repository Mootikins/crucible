//! `cru.context.attach` — mid-turn knowledge attachment.
//!
//! A Lua handler that retrieves something useful partway through a turn (from
//! `tool_result`, say) needs somewhere to put it where the agent's *next* LLM
//! call will see it. This is that place.
//!
//! Context only. Attachments never reach the conversation tree or the session
//! log — history stays append-only and scheduler-owned, and forking is the only
//! way to diverge from it. An attachment shapes one turn's context and then it
//! is gone.
//!
//! Split from `context.rs` to stay under the module line budget.

use crate::error::LuaError;
use mlua::{Lua, Table};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Cumulative character budget for everything attached during one session.
///
/// Deliberately tight. Retrieval quality does not improve monotonically with
/// volume — past a few hundred tokens of *curated* context the additional
/// material dilutes rather than helps, and every character is also re-sent on
/// each subsequent LLM call in the turn. A generous budget makes the feature
/// worse, not better.
pub const DEFAULT_ATTACH_BUDGET_CHARS: usize = 2000;

/// Why an attachment did not make it into the context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachRejection {
    /// `key` was already attached for this session. Ten `.cpp` files should
    /// surface the C++ notes once, not ten times.
    DuplicateKey,
    /// Would exceed the session's cumulative budget.
    BudgetExhausted { used: usize, budget: usize },
    /// Empty content — nothing to attach.
    Empty,
}

impl AttachRejection {
    pub fn reason(&self) -> String {
        match self {
            Self::DuplicateKey => "duplicate key; already attached this session".to_string(),
            Self::BudgetExhausted { used, budget } => {
                format!("attach budget exhausted ({used}/{budget} chars)")
            }
            Self::Empty => "empty content".to_string(),
        }
    }
}

#[derive(Debug, Default)]
struct SessionAttachments {
    /// Attached but not yet handed to the agent.
    pending: Vec<String>,
    /// Dedup identities seen for the whole session, not just the pending
    /// batch — otherwise draining would let the same key back in.
    seen_keys: HashSet<String>,
    chars_used: usize,
}

/// Per-session attachment buffers, written by Lua and drained by the scheduler.
///
/// Same shape as the status/isolation/validator registries: one instance owned
/// by the daemon, shared with the plugin VM. All plugins share one Lua state, so
/// everything here is keyed by session explicitly rather than held in handler
/// closures.
#[derive(Debug, Clone)]
pub struct ContextAttachRegistry {
    sessions: Arc<Mutex<HashMap<String, SessionAttachments>>>,
    budget: usize,
}

impl Default for ContextAttachRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_ATTACH_BUDGET_CHARS)
    }
}

impl ContextAttachRegistry {
    pub fn new(budget: usize) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            budget,
        }
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Queue content for the session's next LLM call.
    ///
    /// Enforcement lives here, allocation lives in the handler — the same
    /// division the precognition char cap uses.
    pub fn attach(
        &self,
        session_id: &str,
        content: &str,
        key: Option<&str>,
    ) -> Result<(), AttachRejection> {
        if content.trim().is_empty() {
            return Err(AttachRejection::Empty);
        }

        let Ok(mut guard) = self.sessions.lock() else {
            // Poisoned lock: drop the attachment rather than panic. Losing a
            // retrieval degrades an answer; panicking kills the turn.
            return Err(AttachRejection::Empty);
        };
        let entry = guard.entry(session_id.to_string()).or_default();

        if let Some(key) = key {
            if entry.seen_keys.contains(key) {
                return Err(AttachRejection::DuplicateKey);
            }
        }

        let cost = content.chars().count();
        if entry.chars_used + cost > self.budget {
            return Err(AttachRejection::BudgetExhausted {
                used: entry.chars_used,
                budget: self.budget,
            });
        }

        if let Some(key) = key {
            entry.seen_keys.insert(key.to_string());
        }
        entry.chars_used += cost;
        entry.pending.push(content.to_string());
        Ok(())
    }

    /// Take everything queued for this session. Keys and the spent budget
    /// survive the drain — dedup is per session, not per batch.
    pub fn drain(&self, session_id: &str) -> Vec<String> {
        let Ok(mut guard) = self.sessions.lock() else {
            return Vec::new();
        };
        guard
            .get_mut(session_id)
            .map(|entry| std::mem::take(&mut entry.pending))
            .unwrap_or_default()
    }

    /// Whether anything is queued, without taking it.
    pub fn has_pending(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .ok()
            .and_then(|g| g.get(session_id).map(|e| !e.pending.is_empty()))
            .unwrap_or(false)
    }

    /// Drop a session's buffer and its dedup state at session end.
    pub fn release(&self, session_id: &str) {
        if let Ok(mut guard) = self.sessions.lock() {
            guard.remove(session_id);
        }
    }
}

/// Register `cru.context.attach(session_id, content, opts?)`.
///
/// ```lua
/// crucible.on("tool_result", { pattern = "read_file" }, function(ctx, event)
///   local ft = event.args.path:match("%.(%w+)$")
///   if not ft then return end
///   local notes = cru.kiln.search("conventions for " .. ft)
///   cru.context.attach(ctx.session_id, notes, { key = "filetype:" .. ft })
/// end)
/// ```
///
/// Returns `(true, nil)` when queued and `(false, reason)` when dropped —
/// duplicate key, exhausted budget, or empty content. Dropping is normal
/// operation, not an error: a handler firing on every tool call is *expected*
/// to be deduplicated away most of the time.
pub fn register_context_attach(
    lua: &Lua,
    registry: Arc<ContextAttachRegistry>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    // The per-session VM is a bare `Lua::new()` with no `cru` table, while the
    // plugin VM has a fully populated one. Create only what's missing so this
    // registers identically on both — a handler shouldn't care which VM it
    // happens to be running in.
    let cru: Table = match globals.get::<Table>("cru") {
        Ok(existing) => existing,
        Err(_) => {
            let created = lua.create_table()?;
            globals.set("cru", created.clone())?;
            created
        }
    };
    let context: Table = match cru.get::<Table>("context") {
        Ok(existing) => existing,
        Err(_) => {
            let created = lua.create_table()?;
            cru.set("context", created.clone())?;
            created
        }
    };

    let attach_fn = lua.create_function(
        move |_, (session_id, content, opts): (String, String, Option<Table>)| {
            let key = opts
                .as_ref()
                .and_then(|t| t.get::<String>("key").ok())
                .filter(|k| !k.is_empty());

            match registry.attach(&session_id, &content, key.as_deref()) {
                Ok(()) => Ok((true, None::<String>)),
                Err(rejection) => Ok((false, Some(rejection.reason()))),
            }
        },
    )?;
    context.set("attach", attach_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_queues_content_for_drain() {
        let registry = ContextAttachRegistry::default();
        registry.attach("s1", "note body", None).unwrap();

        assert!(registry.has_pending("s1"));
        assert_eq!(registry.drain("s1"), vec!["note body".to_string()]);
        assert!(!registry.has_pending("s1"));
    }

    #[test]
    fn drain_is_scoped_to_one_session() {
        let registry = ContextAttachRegistry::default();
        registry.attach("s1", "for one", None).unwrap();
        registry.attach("s2", "for two", None).unwrap();

        assert_eq!(registry.drain("s1"), vec!["for one".to_string()]);
        assert_eq!(registry.drain("s2"), vec!["for two".to_string()]);
    }

    #[test]
    fn duplicate_key_is_rejected_so_repeated_triggers_attach_once() {
        let registry = ContextAttachRegistry::default();
        registry
            .attach("s1", "cpp notes", Some("filetype:cpp"))
            .unwrap();

        assert_eq!(
            registry.attach("s1", "cpp notes", Some("filetype:cpp")),
            Err(AttachRejection::DuplicateKey)
        );
        assert_eq!(registry.drain("s1").len(), 1);
    }

    #[test]
    fn dedup_survives_a_drain() {
        // The agent consuming the attachment must not re-open the door: the
        // notes are in the context now, so attaching them again is still a
        // duplicate.
        let registry = ContextAttachRegistry::default();
        registry.attach("s1", "cpp notes", Some("k")).unwrap();
        registry.drain("s1");

        assert_eq!(
            registry.attach("s1", "cpp notes", Some("k")),
            Err(AttachRejection::DuplicateKey)
        );
    }

    #[test]
    fn keyless_attachments_are_never_deduplicated() {
        let registry = ContextAttachRegistry::default();
        registry.attach("s1", "one", None).unwrap();
        registry.attach("s1", "one", None).unwrap();

        assert_eq!(registry.drain("s1").len(), 2);
    }

    #[test]
    fn budget_is_cumulative_across_attachments() {
        let registry = ContextAttachRegistry::new(10);
        registry.attach("s1", "12345", None).unwrap();
        registry.attach("s1", "12345", None).unwrap();

        assert_eq!(
            registry.attach("s1", "x", None),
            Err(AttachRejection::BudgetExhausted {
                used: 10,
                budget: 10
            })
        );
    }

    #[test]
    fn budget_counts_characters_not_bytes() {
        // Multi-byte content must not be charged triple; the precognition cap
        // counts characters and this has to agree with it.
        let registry = ContextAttachRegistry::new(10);
        registry.attach("s1", "日本語テキスト", None).unwrap();

        assert_eq!(registry.drain("s1").len(), 1);
    }

    #[test]
    fn spent_budget_survives_a_drain() {
        let registry = ContextAttachRegistry::new(10);
        registry.attach("s1", "1234567890", None).unwrap();
        registry.drain("s1");

        assert!(matches!(
            registry.attach("s1", "more", None),
            Err(AttachRejection::BudgetExhausted { .. })
        ));
    }

    #[test]
    fn empty_content_is_rejected() {
        let registry = ContextAttachRegistry::default();
        assert_eq!(
            registry.attach("s1", "   \n ", None),
            Err(AttachRejection::Empty)
        );
    }

    #[test]
    fn release_clears_buffer_and_dedup_state() {
        let registry = ContextAttachRegistry::new(10);
        registry.attach("s1", "1234567890", Some("k")).unwrap();
        registry.release("s1");

        assert!(!registry.has_pending("s1"));
        // Budget and keys reset with the session.
        registry.attach("s1", "1234567890", Some("k")).unwrap();
    }
}
