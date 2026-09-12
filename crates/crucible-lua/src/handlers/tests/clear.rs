//! Tests for `cru.clear` — the Lua-visible retirement call.
//!
//! `nvim_clear_autocmds` is the most used retirement call in Neovim's shipped
//! runtime, and its dominant form is `{ group = g, buf = b }`: "my own rows,
//! for this container". Crucible had the Rust for that twice — `clear_source`
//! and `clear_session` — and reached neither from Lua, so a plugin author who
//! wanted to retire a handler early had no call at all.
//!
//! The property that matters most is the implicit source. A plugin clears its
//! own rows and cannot reach another plugin's, and it holds because the source
//! comes from the VM's Rust-side app data rather than from an argument.

use crate::handlers::{
    register_cru_on_api, Firing, LuaScriptHandlerRegistry, SessionScope, StageId,
};
use crate::plugin_context::{enter_plugin, enter_session, LuaSource};
use mlua::Lua;

fn vm() -> (Lua, LuaScriptHandlerRegistry) {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).expect("register cru.on");
    (lua, registry)
}

/// Run `source` as if the host were inside `session`.
fn load_in_session(lua: &Lua, session: &str, source: &str) -> mlua::Result<()> {
    let _guard = enter_session(lua, Some(session));
    lua.load(source).exec()
}

fn clear(lua: &Lua, call: &str) -> mlua::Result<usize> {
    lua.load(call).eval::<usize>()
}

/// The gate: a plugin reaches its own rows and nobody else's.
///
/// The source is not an argument, so there is nothing for a caller to spell.
/// This is the same reason `register` takes the source from the VM.
#[test]
fn clear_removes_only_the_calling_sources_rows() {
    let (lua, registry) = vm();

    enter_plugin(&lua, "alpha");
    lua.load(r#"cru.on("turn:complete", function() end)"#)
        .exec()
        .expect("alpha registers");
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("alpha registers a second");

    enter_plugin(&lua, "beta");
    lua.load(r#"cru.on("turn:complete", function() end)"#)
        .exec()
        .expect("beta registers");

    assert_eq!(registry.all().len(), 3);

    // Running as beta, clear everything: alpha's two rows must survive.
    assert_eq!(clear(&lua, "return cru.clear()").expect("clears"), 1);

    let left = registry.all();
    assert_eq!(left.len(), 2, "only beta's row went");
    assert!(
        left.iter()
            .all(|r| r.source == LuaSource::Plugin("alpha".into())),
        "a second plugin's rows survive: {:?}",
        left.iter().map(|r| &r.source).collect::<Vec<_>>()
    );
}

/// Every filter absent clears everything the source registered — what
/// `nvim_del_augroup_by_name` does for a group.
#[test]
fn clear_with_no_filter_takes_every_row_of_the_source() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(
        r#"
        cru.on("turn:complete", function() end)
        cru.on("pre_tool_call", { pattern = "bash" }, function() end)
        "#,
    )
    .exec()
    .expect("registers");

    assert_eq!(clear(&lua, "return cru.clear{}").expect("clears"), 2);
    assert!(registry.all().is_empty());
}

/// `name` narrows to one hook.
#[test]
fn clear_by_name_leaves_the_other_hooks() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(
        r#"
        cru.on("turn:complete", function() end)
        cru.on("pre_tool_call", function() end)
        "#,
    )
    .exec()
    .expect("registers");

    assert_eq!(
        clear(&lua, r#"return cru.clear{ name = "turn:complete" }"#).expect("clears"),
        1
    );
    let left = registry.all();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, StageId::PreToolCall.into());
}

/// The pattern is compared as EXACT TEXT and never evaluated as a glob.
///
/// Neovim states the same rule for `nvim_clear_autocmds`
/// (`api/autocmd.c:538`). A glob evaluated here would remove rows the caller
/// never named: clearing `"bash"` would take a row registered `"b*"`, which
/// also fires for tools the caller said nothing about.
#[test]
fn clear_by_pattern_matches_the_text_and_does_not_evaluate_the_glob() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(
        r#"
        cru.on("pre_tool_call", { pattern = "bash" }, function() end)
        cru.on("pre_tool_call", { pattern = "b*" }, function() end)
        "#,
    )
    .exec()
    .expect("registers");

    assert_eq!(
        clear(&lua, r#"return cru.clear{ pattern = "bash" }"#).expect("clears"),
        1,
        "the glob row is NOT taken, even though it fires for `bash`"
    );
    let left = registry.all();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].pattern.as_deref(), Some("b*"));
}

/// `session` narrows to the rows scoped to it, and leaves the unscoped ones.
#[test]
fn clear_by_session_leaves_the_global_rows() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("a global row");
    load_in_session(
        &lua,
        "s1",
        r#"cru.on("pre_tool_call", { session = "s1" }, function() end)"#,
    )
    .expect("a scoped row");
    assert_eq!(registry.all().len(), 2);

    let removed = {
        let _guard = enter_session(&lua, Some("s1"));
        clear(&lua, r#"return cru.clear{ session = "s1" }"#).expect("clears")
    };
    assert_eq!(removed, 1);

    let left = registry.all();
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].scope,
        SessionScope::Global,
        "an unscoped row belongs to the load, not to the session"
    );
}

/// A `session` naming no live session is refused, exactly as a scoped
/// registration is — the host resolves the id, and there is nothing here to
/// resolve it against.
#[test]
fn clear_naming_a_session_outside_every_session_is_refused() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("registers");

    let err = clear(&lua, r#"return cru.clear{ session = "s1" }"#)
        .expect_err("a session outside every session must be refused");
    assert!(
        err.to_string().contains("cru.clear"),
        "the message must name the API: {err}"
    );
    assert_eq!(registry.all().len(), 1, "and nothing may be removed");
}

/// A caller may not name a session other than the one it runs in. Without
/// this, a plugin in session A could sweep its own rows out of session B.
#[test]
fn clear_naming_another_session_is_refused() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    load_in_session(
        &lua,
        "s1",
        r#"cru.on("pre_tool_call", { session = "s1" }, function() end)"#,
    )
    .expect("registers");

    let err = {
        let _guard = enter_session(&lua, Some("s1"));
        clear(&lua, r#"return cru.clear{ session = "s2" }"#)
            .expect_err("naming another session must be refused")
    };
    assert!(err.to_string().contains("s2"), "{err}");
    assert_eq!(registry.all().len(), 1, "and nothing may be removed");
}

/// An unknown `name` RAISES rather than clearing nothing.
///
/// A typo that silently removes no rows is the defect `cru.on` closed by
/// refusing a misspelt event: the author reads a success and believes the
/// handler is gone.
#[test]
fn clear_with_a_misspelt_name_is_refused_with_a_suggestion() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("registers");

    let err = clear(&lua, r#"return cru.clear{ name = "pre_toolcall" }"#)
        .expect_err("a typo must not read as a successful clear");
    assert!(
        err.to_string().contains("did you mean `pre_tool_call`"),
        "{err}"
    );
    assert_eq!(registry.all().len(), 1, "and nothing may be removed");
}

/// `cru.clear` reaches the four names that have their own registration API.
///
/// `cru.on` refuses those names, because each takes a different argument. A
/// row registered through `cru.on_session_start` lives in the same store, and
/// the plugin that made it has to be able to retire it.
#[test]
fn clear_reaches_a_hook_that_has_its_own_registration_api() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).expect("register cru.on");
    let cru = crate::lua_util::get_or_create_namespace(&lua, "cru").expect("cru");
    crate::hooks::register_hooks_module(&lua, &cru).expect("the session hook API");
    enter_plugin(&lua, "alpha");

    lua.load(r#"cru.on_session_start(function(s) end)"#)
        .exec()
        .expect("registers");
    assert_eq!(registry.all().len(), 1);

    assert_eq!(
        clear(&lua, r#"return cru.clear{ name = "session:start" }"#).expect("clears"),
        1,
        "`cru.on` refuses this name, and `cru.clear` must not"
    );
    assert!(registry.all().is_empty());
}

/// Code outside every host bracket runs as the user's own `init.lua`, which
/// owns what it registered and nothing else.
///
/// [`LuaSource`] is total, so there is no "no source" to refuse. The property
/// the implicit source exists for holds all the same: a clear from here
/// cannot reach a plugin's rows.
#[test]
fn clear_outside_every_plugin_reaches_only_the_users_own_rows() {
    let (lua, registry) = vm();

    enter_plugin(&lua, "alpha");
    lua.load(r#"cru.on("turn:complete", function() end)"#)
        .exec()
        .expect("alpha registers");

    // Back to the user's own source, and register one row there.
    crate::plugin_context::set_source(&lua, LuaSource::UserLua);
    lua.load(r#"cru.on("turn:complete", function() end)"#)
        .exec()
        .expect("init.lua registers");
    assert_eq!(registry.all().len(), 2);

    assert_eq!(
        clear(&lua, "return cru.clear()").expect("clears"),
        1,
        "it takes the user's row"
    );
    let left = registry.all();
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].source,
        LuaSource::Plugin("alpha".into()),
        "a plugin's row is out of reach"
    );
}

/// Clearing is idempotent, and the count is what was removed.
#[test]
fn clear_answers_how_many_it_removed_and_is_idempotent() {
    let (lua, _registry) = vm();
    enter_plugin(&lua, "alpha");
    lua.load(
        r#"
        cru.on("turn:complete", function() end)
        cru.on("turn:complete", function() end)
        "#,
    )
    .exec()
    .expect("two unscoped rows append");

    assert_eq!(
        clear(&lua, r#"return cru.clear{ name = "turn:complete" }"#).expect("clears"),
        2
    );
    assert_eq!(
        clear(&lua, r#"return cru.clear{ name = "turn:complete" }"#).expect("clears"),
        0,
        "a second clear removes nothing and does not raise"
    );
}

/// The registration a session-scoped author really writes: clear, then
/// register. Under the replacement key this is belt and braces; for an
/// unscoped row it is the only way to avoid accumulating.
#[test]
fn clear_then_register_leaves_one_row_across_repeated_activations() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");

    // Three activations: a create and two history fetches.
    for _ in 0..3 {
        load_in_session(
            &lua,
            "s1",
            r#"
            cru.clear{ name = "pre_tool_call", session = "s1" }
            cru.on("pre_tool_call", { session = "s1" }, function() end)
            "#,
        )
        .expect("activates");
    }

    assert_eq!(
        registry
            .runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s1"))
            .len(),
        1,
        "three activations leave one handler"
    );
    assert_eq!(registry.all().len(), 1);
}
