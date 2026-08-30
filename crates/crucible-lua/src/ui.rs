//! `cru.ui.*` — asking the user something from a plugin.
//!
//! Every function here builds one [`InteractionRequest`] variant, hands it to
//! the daemon, and parks until a client answers. The seven variants are the
//! closed set the TUI and the web both render; this module adds no eighth
//! shape of its own, because a request no client knows how to draw is a
//! plugin hanging until its timeout.
//!
//! ## Why this is not `cru.session.ask`
//!
//! An interaction is addressed to a *client*, not to a session — the session
//! id only says which attached client to route to. Putting these under
//! `cru.session` would read as "do something to this conversation", which is
//! what `send_message` does and this does not.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local answer = cru.ui.ask(session, {
//!   question = "Which branch?",
//!   choices = { "main", "develop" },
//!   allow_other = true,
//! })
//! if answer.kind == "cancelled" then return end
//! ```
//!
//! Every function returns `(response, nil)` or `(nil, err)`. A response of
//! `{ kind = "cancelled" }` is a *successful* call that nobody answered —
//! no client attached, the user dismissed the modal, or the timeout elapsed.
//! Plugins must handle it; it is the common case on a headless daemon.

use crate::error::LuaError;
use crate::host_registry::Ns;
use crate::lua_util::gate_module_keys;
use crate::sessions::DaemonSessionApi;
use mlua::{Lua, LuaSerdeExt, Value};
use std::sync::Arc;

/// Default seconds to wait for an answer.
///
/// Matches the permission prompt's 300 s rather than picking a second number:
/// both are "a human is expected to look at a modal", and two different
/// answers to that question is one more than the tree needs.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// The `InteractionRequest` variants, by their serde tag.
///
/// Re-exported from `crucible-core` rather than restated: the list there is
/// kept complete by an exhaustive match that fails to compile when a variant
/// is added, and a second copy here would be a second thing to forget.
pub const INTERACTION_KINDS: &[&str] = crucible_core::interaction::InteractionRequest::KINDS;

/// Build the request JSON for `kind` from a Lua options table.
///
/// The table is passed through as the variant's body with `kind` stamped on
/// it, rather than each field being read and re-emitted here. Restating the
/// field list would make this module a second definition of seven structs
/// that already exist in `crucible-core`, and the failure mode of the two
/// drifting is a plugin setting a field the daemon silently ignores.
fn request_json(kind: &str, opts: Value) -> Result<serde_json::Value, mlua::Error> {
    let mut value: serde_json::Value = match opts {
        Value::Nil => serde_json::Value::Object(serde_json::Map::new()),
        other => serde_json::to_value(&other).map_err(mlua::Error::external)?,
    };
    let obj = value.as_object_mut().ok_or_else(|| {
        mlua::Error::external(LuaError::Runtime(format!(
            "cru.ui.{kind}: options must be a table"
        )))
    })?;
    // A caller's own `kind` is overwritten rather than rejected: the function
    // name already chose the variant, so a mismatched key is a typo with one
    // sensible reading.
    obj.insert(
        "kind".to_string(),
        serde_json::Value::String(kind.to_string()),
    );
    Ok(value)
}

/// Seconds to wait, from an options table's `timeout` key.
fn timeout_from(opts: &Value) -> u64 {
    let Value::Table(t) = opts else {
        return DEFAULT_TIMEOUT_SECS;
    };
    t.get::<Option<u64>>("timeout")
        .ok()
        .flatten()
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// One entry of a popup or a panel: `crucible_core::types::PopupEntry`, which
/// `PanelItem` is an alias of.
const ENTRY: &str = "{ label: string, description: string?, data: any? }";

/// The Luau type of `cru.ui.<kind>`, or `None` for a kind nobody wrote one
/// for.
///
/// The options table is the variant's own body from `crucible_core`, plus
/// `timeout`. It is passed straight through to serde (`request_json` stamps
/// only `kind` on it), so a field this list does not name is a field the
/// daemon rejects — which is why each shape is read off the struct rather
/// than widened to `table`.
///
/// `timeout` is in SECONDS, and the key cannot say so without changing what
/// `timeout_from` reads. Zero and absent both mean [`DEFAULT_TIMEOUT_SECS`].
///
/// Every payload field of a RESPONSE is optional, including the ones that are
/// required on the Rust struct: `{ kind = "cancelled" }` is a successful
/// answer with no payload at all, and it is the common case on a headless
/// daemon. Only `kind` is always there.
fn declaration(kind: &str) -> Option<String> {
    let (options, response) = match kind {
        "ask" => (
            "{ question: string, choices: { string }?, multi_select: boolean?, \
             allow_other: boolean?, timeout: number? }"
                .to_string(),
            "{ kind: string, selected: { number }?, other: string? }".to_string(),
        ),
        "ask_batch" => (
            // `id` is a uuid the host mints when it is absent; `choices` is
            // required on `AskQuestion`, unlike `AskRequest`'s.
            "{ questions: { { header: string, question: string, choices: { string }, \
             multi_select: boolean?, allow_other: boolean? } }, id: string?, \
             timeout: number? }"
                .to_string(),
            "{ kind: string, id: string?, \
             answers: { { selected: { number }?, other: string? } }?, \
             cancelled: boolean? }"
                .to_string(),
        ),
        "edit" => (
            // `format` is `ArtifactFormat`: markdown (the default), code,
            // json or plain.
            "{ content: string, format: string?, hint: string?, timeout: number? }".to_string(),
            "{ kind: string, modified: string? }".to_string(),
        ),
        // A `show` has no response variant of its own — nothing to answer
        // with — so every answer is `{ kind = "cancelled" }`. The plugin
        // waits for the client to dismiss it, or for the timeout.
        "show" => (
            "{ content: string, format: string?, title: string?, timeout: number? }".to_string(),
            "{ kind: string }".to_string(),
        ),
        "permission" => (
            // `action` is `PermAction`, tagged on `type`: bash carries
            // `tokens`, read and write carry `segments`, tool carries `name`
            // and `args`.
            "{ action: { type: string, tokens: { string }?, segments: { string }?, \
             name: string?, args: any? }, diffs: { any }?, timeout: number? }"
                .to_string(),
            // `scope` is `PermissionScope`: once (the default), session,
            // project or user.
            "{ kind: string, allowed: boolean?, pattern: string?, scope: string?, \
             reason: string? }"
                .to_string(),
        ),
        "popup" => (
            format!(
                "{{ title: string, entries: {{ {ENTRY} }}?, allow_other: boolean?, \
                 timeout: number? }}"
            ),
            format!(
                "{{ kind: string, selected_index: number?, selected_entry: {ENTRY}?, \
                 other: string? }}"
            ),
        ),
        "panel" => (
            format!(
                "{{ header: string, items: {{ {ENTRY} }}?, \
                 hints: {{ filterable: boolean?, multi_select: boolean?, \
                 allow_other: boolean?, initial_selection: {{ number }}?, \
                 initial_filter: string? }}?, timeout: number? }}"
            ),
            "{ kind: string, cancelled: boolean?, selected: { number }?, other: string? }"
                .to_string(),
        ),
        _ => return None,
    };
    // The error half is a STRING, and it is the second value. Every function
    // here answers `(response, nil)` or `(nil, err)`; neither is raised.
    Some(format!(
        "(session_id: string, options: {options}) -> ({response}?, string?)"
    ))
}

/// Open `cru.ui` with a declaration for every kind.
///
/// A kind with no entry in [`declaration`] fails the VM's construction rather
/// than registering undescribed, which is the same guarantee `gate_module_keys`
/// gives for a kind with no body.
fn declared_kinds() -> Result<Vec<(&'static str, String)>, LuaError> {
    INTERACTION_KINDS
        .iter()
        .map(|kind| {
            declaration(kind).map(|decl| (*kind, decl)).ok_or_else(|| {
                LuaError::Runtime(format!(
                    "cru.ui.{kind}: no Luau declaration. Add one to `ui::declaration`."
                ))
            })
        })
        .collect()
}

/// Register `cru.ui` with stub functions that report no daemon.
///
/// Mirrors `register_sessions_module`: the table exists in every VM so a
/// plugin can `pcall` it and fall back, rather than getting "attempt to index
/// a nil value" in the VM where no daemon is wired.
pub fn register_ui_module(lua: &Lua) -> Result<(), LuaError> {
    let mut ui = Ns::new(lua, "cru.ui")?;
    for (kind, decl) in declared_kinds()? {
        // The stub declares the SAME type as the daemon-backed body. It has
        // to: a plugin written against a wired VM must typecheck against an
        // unwired one, and the answer it gives — `(nil, err)` — is one the
        // declaration already covers.
        ui.async_func(kind, &decl, |lua, _args: (String, Value)| async move {
            let err = lua.create_string("no daemon connected")?;
            Ok((Value::Nil, Value::String(err)))
        })?;
    }
    ui.publish()?;
    Ok(())
}

/// Replace the `cru.ui` stubs with daemon-backed implementations.
pub fn register_ui_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    // Build a fresh table. The gate at the end compares its keys against
    // INTERACTION_KINDS, so a kind with no daemon-backed body cannot hide
    // behind a stub.
    let mut ui = Ns::new(lua, "cru.ui")?;

    for (kind, decl) in declared_kinds()? {
        let a = Arc::clone(&api);
        ui.async_func(
            kind,
            &decl,
            move |lua, (session_id, opts): (String, Value)| {
                let a = Arc::clone(&a);
                async move {
                    let timeout = timeout_from(&opts);
                    let request = match request_json(kind, opts) {
                        Ok(r) => r,
                        Err(e) => {
                            let err = lua.create_string(e.to_string())?;
                            return Ok((Value::Nil, Value::String(err)));
                        }
                    };
                    match a.request_interaction(session_id, request, timeout).await {
                        Ok(response) => Ok((lua.to_value(&response)?, Value::Nil)),
                        Err(e) => {
                            let err = lua.create_string(&e)?;
                            Ok((Value::Nil, Value::String(err)))
                        }
                    }
                }
            },
        )?;
    }

    gate_module_keys("ui", ui.table(), INTERACTION_KINDS)?;
    ui.publish()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind carries a declaration, and every declaration is Luau the
    /// parser accepts.
    ///
    /// The kinds come from `crucible-core`, so a new variant reaches this
    /// module without anyone editing it. `declared_kinds` already refuses to
    /// register one with no declaration; this says so at the level of the
    /// list, and adds the half `declared_kinds` cannot check — that the text
    /// is a type Luau can read. These declarations land in the one
    /// `declare cru: { … }` file `luau-lsp analyze` loads for every plugin,
    /// where a single malformed type costs every plugin every declaration.
    ///
    /// `permission`'s options carry a field called `type` (the `PermAction`
    /// serde tag), which is the one field name here that could have collided
    /// with the grammar.
    #[test]
    fn every_kind_declares_a_type_luau_accepts() {
        let lua = mlua::Lua::new();
        let kinds = declared_kinds().expect("every kind must carry a declaration");
        assert_eq!(kinds.len(), INTERACTION_KINDS.len());

        for (kind, decl) in kinds {
            let ty = crate::signature::LuaType::parse(&decl)
                .unwrap_or_else(|e| panic!("cru.ui.{kind}: the host cannot read `{decl}`: {e}"));
            let rendered = ty.to_luau();
            let source = format!("type Probe = {rendered}\nreturn 1");
            if let Err(e) = lua.load(&source).exec() {
                panic!("cru.ui.{kind} is declared `{rendered}`, which Luau refuses: {e}");
            }
        }
    }

    /// The stub VM and the daemon-backed VM declare the SAME types.
    ///
    /// A plugin is written against a daemon and checked against whatever VM
    /// `cru plugin stubs` ran on. If the stub declared less, correct code
    /// would fail the check on a machine with no daemon wired.
    #[test]
    fn the_stubs_declare_what_the_daemon_backed_bodies_declare() {
        let lua = mlua::Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").expect("cru");
        register_ui_module(&lua).expect("stubs");
        let stubbed = crate::host_registry::HostSignatures::of(&lua);

        for (kind, decl) in declared_kinds().expect("declarations") {
            let recorded = stubbed
                .get(&format!("cru.ui.{kind}"))
                .unwrap_or_else(|| panic!("cru.ui.{kind} must be declared on the stub VM"));
            let expected = crate::signature::LuaType::parse(&decl).expect("well formed");
            assert_eq!(recorded, expected, "cru.ui.{kind}");
        }
    }
}
