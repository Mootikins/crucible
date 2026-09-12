//! Tests for the daemon plugin loader — split from `mod.rs` for the
//! file-size gate; same module path (`daemon_plugins::tests`) as before.
//!
//! `shipped` holds everything asserted about the BUNDLED plugin set —
//! discovery, execution, manifest shape, the config kill switch. Kept
//! apart because that set grows and these do not. `lifecycle` holds the
//! load/reload bookkeeping contracts (spec merging, the inert-on-failure
//! guarantee, loading-marker attribution).
use super::*;

mod active_kiln;
mod install;
mod lifecycle;
mod plugin_context;
mod services;
mod shipped;

#[test]
fn daemon_plugin_loader_creates_successfully() {
    let loader = DaemonPluginLoader::new(HashMap::new());
    assert!(
        loader.is_ok(),
        "DaemonPluginLoader::new() failed: {:?}",
        loader.err()
    );
}

/// Contract test for the plugin runtime's API surface.
///
/// Plugins execute in this loader's VM, which is disjoint from the
/// per-`lua.init_session` VM. Anything a plugin is
/// documented to call must be registered *here* — registering it on the
/// other two is invisible to plugins.
///
/// `cru.on` was missing for exactly this reason: it was registered on
/// the other two VMs, so every hook-registering plugin (`oci`, the
/// reference interception plugin) raised "attempt to call a nil value" at
/// load and was silently downgraded to a `warn!`.
#[test]
fn plugin_runtime_exposes_the_documented_api_surface() {
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let lua = loader.plugin_lua();

    for symbol in ["cru.on", "cru.on_session_start", "cru.on_session_end"] {
        let is_function: bool = lua
            .load(format!("return type({symbol}) == 'function'"))
            .eval()
            .unwrap_or(false);
        assert!(
            is_function,
            "`{symbol}` must be a function in the plugin runtime — plugins \
             documented to call it would raise 'attempt to call a nil value' \
             at load, and the loader downgrades that to a warning"
        );
    }
}

/// Session lifecycle hooks registered by a plugin must fire.
///
/// `oci` — the reference interception plugin — registers its `cru.on`
/// handlers *inside* `on_session_start` (`init.lua:261,306`). Making
/// `cru.on` callable is not enough on its own: if nothing fires the
/// plugin runtime's lifecycle hooks, that registration never runs.
///
/// `fire_session_start_hooks` was only ever called at `server/lua.rs:34`,
/// on the per-call `lua.init_session` executor — never on the plugin
/// loader's.
#[tokio::test]
async fn plugin_session_lifecycle_hooks_fire() {
    use crucible_lua::{Session, UnsupportedSessionRpc};

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        start_fired = false
        end_fired = false
        cru.on_session_start(function(s) start_fired = true end)
        cru.on_session_end(function(s) end_fired = true end)
    "#,
        )
        .exec()
        .expect("register lifecycle hooks");

    let session = Session::new("test-session".to_string());
    session.bind(Box::new(UnsupportedSessionRpc));

    loader
        .fire_session_start(&session)
        .await
        .expect("fire start");
    let started: bool = loader
        .plugin_lua()
        .load("return start_fired")
        .eval()
        .unwrap();
    assert!(
        started,
        "plugin on_session_start hook did not fire — plugins that register \
         handlers there (oci) never run"
    );

    loader.fire_session_end(&session).await.expect("fire end");
    let ended: bool = loader.plugin_lua().load("return end_fired").eval().unwrap();
    assert!(ended, "plugin on_session_end hook did not fire");
}

/// A slow `on_session_start` hook still completes.
///
/// The lifecycle budget is 120 s, not the 30 s a turn-loop stage gets, and the
/// reason is `oci`: it pulls container images in `on_session_start`. A budget
/// short enough to be pleasant for a turn-loop stage would refuse every
/// session a shipped plugin sandboxes.
#[tokio::test]
async fn a_slow_session_start_hook_completes_under_the_lifecycle_budget() {
    use crucible_lua::{Session, UnsupportedSessionRpc};

    assert!(
        crucible_lua::LIFECYCLE_BUDGET > crucible_lua::TURN_STAGE_BUDGET,
        "a lifecycle hook may take longer than a turn-loop stage"
    );

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        slow_ok = false
        cru.on_session_start(function(s)
            cru.timer.sleep(1.2)
            slow_ok = true
        end)
    "#,
        )
        .exec()
        .expect("register hook");

    let session = Session::new("slow-start".to_string());
    session.bind(Box::new(UnsupportedSessionRpc));
    loader
        .fire_session_start(&session)
        .await
        .expect("a slow hook must not fail the session");

    let ok: bool = loader.plugin_lua().load("return slow_ok").eval().unwrap();
    assert!(ok, "a hook slower than a turn-loop stage was cut short");
}

/// A lifecycle hook must be able to call the async `cru.*` APIs.
///
/// `cru.shell.exec`, `cru.http.*` and `cru.timer.sleep` are all
/// `create_async_function`s, which cannot suspend under a plain
/// `Function::call`. Firing hooks synchronously meant a plugin could not
/// start a container from `on_session_start` — the entire point of `oci`.
#[tokio::test]
async fn a_lifecycle_hook_can_call_async_apis() {
    use crucible_lua::{Session, UnsupportedSessionRpc};

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        async_ok = false
        cru.on_session_start(function(s)
            -- cru.timer.sleep is async; under a synchronous call this
            -- either raises or never resumes.
            cru.timer.sleep(1)
            async_ok = true
        end)
    "#,
        )
        .exec()
        .expect("register hook");

    let session = Session::new("async-hook".to_string());
    session.bind(Box::new(UnsupportedSessionRpc));
    loader
        .fire_session_start(&session)
        .await
        .expect("hook runs");

    let ok: bool = loader.plugin_lua().load("return async_ok").eval().unwrap();
    assert!(
        ok,
        "hook calling an async cru API did not complete — lifecycle hooks \
         are being fired without an async context"
    );
}

/// A lifecycle hook must see the session's real workspace.
///
/// `oci` bind-mounts `session.workspace` into the container. If it is nil
/// the plugin can't start; if it is a placeholder the plugin isolates the
/// wrong directory — which looks like it worked. The property is read-only
/// for the same reason.
#[tokio::test]
async fn a_lifecycle_hook_sees_the_session_workspace() {
    use crucible_lua::{Session, UnsupportedSessionRpc};

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        seen_workspace = nil
        readonly_enforced = false
        cru.on_session_start(function(s)
            seen_workspace = s.workspace
            readonly_enforced = not pcall(function() s.workspace = "/hijacked" end)
        end)
    "#,
        )
        .exec()
        .expect("register hook");

    let session = Session::new("ws-test".to_string()).with_workspace("/tmp/crucible-ws-fixture");
    session.bind(Box::new(UnsupportedSessionRpc));
    loader
        .fire_session_start(&session)
        .await
        .expect("hook runs");

    let seen: Option<String> = loader
        .plugin_lua()
        .load("return seen_workspace")
        .eval()
        .unwrap();
    assert_eq!(
        seen.as_deref(),
        Some("/tmp/crucible-ws-fixture"),
        "hook did not receive the session's workspace"
    );

    let readonly: bool = loader
        .plugin_lua()
        .load("return readonly_enforced")
        .eval()
        .unwrap();
    assert!(readonly, "session.workspace must be read-only from Lua");
}

/// An ordinary plugin's failing start hook must not refuse the session;
/// only a hook that opted in with `{ required = true }` may.
///
/// Refusal exists for hooks that own an isolation boundary (`oci` and its
/// container). Making *every* hook fatal meant one typo in any loaded
/// plugin refused every session daemon-wide — and a shipped doc example
/// used a `Session` property that raises, so copying it bricked session
/// creation.
#[tokio::test]
async fn only_required_start_hooks_can_refuse_a_session() {
    use crucible_lua::{Session, UnsupportedSessionRpc};

    // An ordinary plugin that raises — logged, session proceeds.
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(r#"cru.on_session_start(function(s) error("ordinary boom") end)"#)
        .exec()
        .unwrap();
    let session = Session::new("s1".to_string());
    session.bind(Box::new(UnsupportedSessionRpc));
    assert!(
        loader.fire_session_start(&session).await.is_ok(),
        "an ordinary plugin's failure must not refuse the session"
    );

    // A plugin that opted in — refuses.
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(r#"cru.on_session_start(function(s) error("gate boom") end, { required = true })"#)
        .exec()
        .unwrap();
    let session = Session::new("s2".to_string());
    session.bind(Box::new(UnsupportedSessionRpc));
    let err = loader
        .fire_session_start(&session)
        .await
        .expect_err("a required hook's failure must refuse the session");
    assert!(
        err.to_string().contains("gate boom"),
        "refusal must name the underlying failure, got: {err}"
    );
}

/// Reloading a plugin must replace its handlers, not append a second set.
///
/// The registry is loader-global and append-only, so before attribution
/// every `plugin.reload` (and every file-watcher trigger) left the previous
/// handlers registered and firing against dead state. With `pre_tool_call`
/// failing closed, one stale handler raising denies every tool call in
/// every session — so the append-only registry and the fail-closed gate
/// were a bad pair.
#[tokio::test]
async fn reloading_a_plugin_replaces_its_handlers() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("reloadable");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        cru.on("pre_tool_call", { pattern = "bash" }, function(ctx, event) end)
        return { name = "reloadable", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let init = dir.join("init.lua");
    loader
        .execute_plugin("reloadable", &init)
        .await
        .expect("first load");
    let after_first = loader
        .plugin_handlers()
        .runtime_handlers_for("pre_tool_call", Some("bash"))
        .len();
    assert_eq!(after_first, 1, "first load should register exactly one");

    loader
        .execute_plugin("reloadable", &init)
        .await
        .expect("reload");
    let after_reload = loader
        .plugin_handlers()
        .runtime_handlers_for("pre_tool_call", Some("bash"))
        .len();
    assert_eq!(
        after_reload, 1,
        "reload duplicated handlers — stale copies keep firing against dead state"
    );
}

/// Reloading one plugin must not rebind another registrant's handler.
///
/// Handler names were allocated from `runtime_handlers.len()`, which
/// `clear_plugin_handlers` shrinks — so a reload handed the reloaded plugin
/// names a *surviving* plugin (or the user's `init.lua`) still held in
/// `handler_functions`, and dispatch, which is by name, ran the wrong body.
/// With `pre_tool_call` failing closed, a body raising against the wrong event
/// shape denies every matching tool call in every session.
#[tokio::test]
async fn reloading_one_plugin_leaves_another_plugins_handler_bound_to_its_own_function() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();

    // Two handlers, so alpha's re-registration reaches beta's name.
    let alpha = tmp.path().join("alpha");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::write(
        alpha.join("init.lua"),
        r#"
        cru.on("turn:complete", function() return { handled = true, result = "alpha" } end)
        cru.on("turn:complete", function() return { handled = true, result = "alpha" } end)
        return { name = "alpha", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let beta = tmp.path().join("beta");
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::write(
        beta.join("init.lua"),
        r#"
        cru.on("pre_tool_call", { pattern = "bash" }, function()
            return { handled = true, result = "beta" }
        end)
        return { name = "beta", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .execute_plugin("alpha", &alpha.join("init.lua"))
        .await
        .expect("load alpha");
    loader
        .execute_plugin("beta", &beta.join("init.lua"))
        .await
        .expect("load beta");
    loader
        .execute_plugin("alpha", &alpha.join("init.lua"))
        .await
        .expect("reload alpha");

    let registry = loader.plugin_handlers();
    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("bash"));
    assert_eq!(
        handlers.len(),
        1,
        "beta registered exactly one bash handler and alpha registered none"
    );

    let event = crucible_core::events::SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({ "tool": "bash", "args": {} }),
    };
    let result = registry
        .execute_runtime_handler(&loader.plugin_lua(), handlers[0].id, &event, Some("s1"))
        .await
        .expect("dispatch beta's handler");

    match result {
        crucible_lua::ScriptHandlerResult::Handled { result, .. } => assert_eq!(
            result,
            serde_json::json!("beta"),
            "beta's handler ran alpha's function — reloading alpha reused beta's handler name"
        ),
        other => panic!("expected Handled, got {other:?}"),
    }
}

/// The user's init.lua handlers carry `plugin: None`, so nothing ever clears
/// them — a plugin reload that reused their names would leave a rebound user
/// handler wrong for the daemon's lifetime. This pins that a reload keeps a
/// user handler bound to its own function, whichever order the user file and
/// the plugins were evaluated in.
#[tokio::test]
async fn reloading_a_plugin_leaves_a_user_init_handler_bound_to_its_own_function() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();

    let alpha = tmp.path().join("alpha");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::write(
        alpha.join("init.lua"),
        r#"
        cru.on("turn:complete", function() return { handled = true, result = "alpha" } end)
        cru.on("turn:complete", function() return { handled = true, result = "alpha" } end)
        return { name = "alpha", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let user_init = tmp.path().join("init.lua");
    std::fs::write(
        &user_init,
        r#"
        cru.on("pre_tool_call", { pattern = "bash" }, function()
            return { handled = true, result = "user" }
        end)
    "#,
    )
    .unwrap();

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .execute_plugin("alpha", &alpha.join("init.lua"))
        .await
        .expect("load alpha");
    // Evaluated with no plugin context, as the boot evaluation runs the
    // user's file, so the `plugin: None` attribution is genuine.
    let user_source = std::fs::read_to_string(&user_init).unwrap();
    loader.eval(&user_source).await.expect("user init");
    loader
        .execute_plugin("alpha", &alpha.join("init.lua"))
        .await
        .expect("reload alpha");

    let registry = loader.plugin_handlers();
    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("bash"));
    assert_eq!(
        handlers.len(),
        1,
        "the user handler is the only bash handler, and nothing clears it"
    );
    assert_ne!(
        handlers[0].owner,
        crucible_lua::Owner::Plugin("alpha".to_string()),
        "the handler belongs to the evaluated source, not to alpha — that is \
         why alpha's reload does not clear it"
    );

    let event = crucible_core::events::SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({ "tool": "bash", "args": {} }),
    };
    let result = registry
        .execute_runtime_handler(&loader.plugin_lua(), handlers[0].id, &event, Some("s1"))
        .await
        .expect("dispatch the user handler");

    match result {
        crucible_lua::ScriptHandlerResult::Handled { result, .. } => assert_eq!(
            result,
            serde_json::json!("user"),
            "the user's handler ran a plugin's function — reloading alpha reused its name"
        ),
        other => panic!("expected Handled, got {other:?}"),
    }
}

/// An installed `cru` has no `runtime/plugins` next to it — the release
/// archive never carried one and the installer would have deleted it — so
/// `kiln-expert`, `oci` and `reflection` reached nobody who did not clone the
/// repo. The tree now comes out of the binary; this asserts the plugins in it
/// are found by the same resolver that serves the dev tree.
#[test]
fn plugins_extracted_from_the_binary_are_discovered() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let extracted = tmp.path().join("runtime-x.y.z");
    crucible_core::runtime_roots::write_bundled_runtime(&extracted).unwrap();

    let paths = daemon_plugin_paths_from(&[crucible_core::runtime_path::RuntimeEntry::root(
        extracted.clone(),
        crucible_core::runtime_path::Origin::Bundled,
    )]);

    assert!(
        paths
            .iter()
            .any(|(p, src)| *p == extracted.join("plugins") && *src == PluginSource::Runtime),
        "extracted plugins must be discovered, got {paths:?}"
    );
    for bundled in ["reflection", "oci", "reflection"] {
        assert!(
            extracted.join("plugins").join(bundled).is_dir(),
            "{bundled} must be in the extracted tree"
        );
    }
}

/// A root with no `plugins/` is skipped rather than offered and then failing
/// to load — the extracted tree is named as a root unconditionally, so this is
/// the common case on any box that never extracted one.
#[test]
fn a_root_without_plugins_is_not_offered() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let path = [crucible_core::runtime_path::RuntimeEntry::root(
        tmp.path().to_path_buf(),
        crucible_core::runtime_path::Origin::Bundled,
    )];
    assert!(daemon_plugin_paths_from(&path).is_empty());
}

/// A kiln on the path contributes no plugin directory, whatever it contains.
///
/// The type-level gate lives in `RuntimeAsset::reaches`; this is the resolver
/// end of it, next to the loader that would have executed what it found.
#[test]
fn a_kiln_entry_offers_the_plugin_loader_nothing() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("plugins").join("evil")).unwrap();
    std::fs::write(
        tmp.path().join("plugins").join("evil").join("init.luau"),
        "-- planted",
    )
    .unwrap();

    let path = [crucible_core::runtime_path::RuntimeEntry::root(
        tmp.path().to_path_buf(),
        crucible_core::runtime_path::Origin::Kiln,
    )];
    assert!(
        daemon_plugin_paths_from(&path).is_empty(),
        "a kiln must never offer a plugin directory, even one that exists"
    );
}

#[test]
fn default_paths_includes_config_dir() {
    let paths = default_daemon_plugin_paths();
    let has_plugins = paths
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains("plugins"));
    assert!(has_plugins, "Expected plugins path in {:?}", paths);
}

#[test]
fn test_default_paths_includes_runtime_when_set() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let runtime_dir = tmp.path();
    std::fs::create_dir(runtime_dir.join("plugins")).unwrap();

    let _guard = crucible_core::test_support::EnvVarGuard::set(
        "CRUCIBLE_RUNTIME",
        runtime_dir.to_string_lossy().to_string(),
    );

    let paths = default_daemon_plugin_paths();

    let has_runtime = paths.iter().any(|(p, src)| {
        p.ends_with("plugins") && p.starts_with(runtime_dir) && *src == PluginSource::Runtime
    });

    assert!(has_runtime, "Expected runtime plugin path in {:?}", paths);
}

/// A configured `runtimepath` ADDS to the shipped runtime; it does not replace
/// it.
///
/// The branches used to be exclusive, so the one thing `runtimepath` is for —
/// putting another tree (a kiln, a shared team checkout) on the search path —
/// silently unloaded all ten bundled plugins. `oci`, `review` and `web-search`
/// would simply stop existing, with a `debug!` line naming only what *was*
/// added as the sole evidence.
#[test]
fn a_configured_runtimepath_adds_to_the_shipped_runtime_rather_than_replacing_it() {
    use tempfile::TempDir;

    let shipped = TempDir::new().unwrap();
    std::fs::create_dir(shipped.path().join("plugins")).unwrap();
    let extra = TempDir::new().unwrap();
    std::fs::create_dir(extra.path().join("plugins")).unwrap();

    let _guard = crucible_core::test_support::EnvVarGuard::set(
        "CRUCIBLE_RUNTIME",
        shipped.path().to_string_lossy().to_string(),
    );

    let paths = daemon_plugin_paths(&[extra.path().to_path_buf()]);

    let has = |root: &std::path::Path| {
        paths
            .iter()
            .any(|(p, src)| p.starts_with(root) && *src == PluginSource::Runtime)
    };

    assert!(
        has(extra.path()),
        "the configured runtimepath entry must be searched: {paths:?}"
    );
    assert!(
        has(shipped.path()),
        "the shipped runtime must STILL be searched — a runtimepath entry adds, \
         it does not replace: {paths:?}"
    );

    // Order matters: the configured entry is the one that can override a
    // bundled plugin by name, so it has to come first.
    let index = |root: &std::path::Path| {
        paths
            .iter()
            .position(|(p, _)| p.starts_with(root))
            .unwrap_or_else(|| panic!("{} absent from {paths:?}", root.display()))
    };
    assert!(
        index(extra.path()) < index(shipped.path()),
        "a runtimepath entry must outrank the shipped runtime so it can shadow \
         a bundled plugin: {paths:?}"
    );
}

/// With no `runtimepath` configured, the shipped runtime is still found — the
/// path everyone who never edits config takes.
#[test]
fn an_empty_runtimepath_still_finds_the_shipped_runtime() {
    use tempfile::TempDir;

    let shipped = TempDir::new().unwrap();
    std::fs::create_dir(shipped.path().join("plugins")).unwrap();

    let _guard = crucible_core::test_support::EnvVarGuard::set(
        "CRUCIBLE_RUNTIME",
        shipped.path().to_string_lossy().to_string(),
    );

    let paths = daemon_plugin_paths(&[]);
    assert!(
        paths
            .iter()
            .any(|(p, src)| p.starts_with(shipped.path()) && *src == PluginSource::Runtime),
        "{paths:?}"
    );
}

#[test]
fn test_runtime_path_resolved_from_exe() {
    // Ensure CRUCIBLE_RUNTIME is not set
    let _guard = crucible_core::test_support::EnvVarGuard::remove("CRUCIBLE_RUNTIME");

    let paths = default_daemon_plugin_paths();

    // Should have at least one path (config dir or exe-relative)
    assert!(!paths.is_empty(), "Expected at least one path");

    // At least one path should contain "plugins"
    let has_plugins = paths
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains("plugins"));
    assert!(has_plugins, "Expected plugins path in {:?}", paths);
}

#[tokio::test]
async fn eval_expression_with_equals_prefix() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    assert_eq!(loader.eval("=1+1").await.unwrap(), "2");
}

#[tokio::test]
async fn eval_string_expression() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    assert_eq!(loader.eval("='hello'").await.unwrap(), "hello");
}

#[tokio::test]
async fn eval_nil_result() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    assert_eq!(loader.eval("=nil").await.unwrap(), "nil");
}

#[tokio::test]
async fn eval_table_as_json() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    let result = loader.eval("={a=1, b=2}").await.unwrap();
    let json: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(json["a"], 1);
    assert_eq!(json["b"], 2);
}

#[tokio::test]
async fn eval_statement_returns_nil() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    assert_eq!(loader.eval("local x = 42").await.unwrap(), "nil");
}

#[tokio::test]
async fn eval_syntax_error_returns_err() {
    let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    assert!(loader.eval("=???").await.is_err());
}

#[test]
fn plugin_name_from_full_https_url() {
    assert_eq!(
        plugin_name_from_url("https://github.com/user/my-plugin.git"),
        Some("my-plugin".to_string())
    );
}

#[test]
fn plugin_name_from_shorthand() {
    assert_eq!(
        plugin_name_from_url("user/my-plugin"),
        Some("my-plugin".to_string())
    );
}

#[test]
fn plugin_name_strips_git_suffix() {
    assert_eq!(
        plugin_name_from_url("git@github.com:user/cool.git"),
        Some("cool".to_string())
    );
}

#[test]
fn plugin_name_no_slash() {
    assert_eq!(
        plugin_name_from_url("standalone"),
        Some("standalone".to_string())
    );
}

#[test]
fn plugin_name_trailing_slash_stripped() {
    assert_eq!(
        plugin_name_from_url("https://github.com/user/repo/"),
        Some("repo".to_string())
    );
}

#[test]
fn plugin_name_empty_url_returns_none() {
    assert_eq!(plugin_name_from_url(""), None);
}

#[test]
fn plugin_name_only_slashes_returns_none() {
    assert_eq!(plugin_name_from_url("///"), None);
}

#[test]
fn plugin_name_dot_returns_none() {
    assert_eq!(plugin_name_from_url("."), None);
}

#[test]
fn plugin_name_dotdot_returns_none() {
    assert_eq!(plugin_name_from_url(".."), None);
}

#[test]
fn plugin_name_bare_git_suffix_returns_none() {
    assert_eq!(plugin_name_from_url(".git"), None);
}

#[test]
fn normalize_passes_https_through() {
    assert_eq!(
        normalize_git_url("https://github.com/user/repo.git").unwrap(),
        "https://github.com/user/repo.git"
    );
}

#[test]
fn normalize_passes_ssh_through() {
    assert_eq!(
        normalize_git_url("git@github.com:user/repo.git").unwrap(),
        "git@github.com:user/repo.git"
    );
}

#[test]
fn normalize_expands_shorthand() {
    assert_eq!(
        normalize_git_url("user/repo").unwrap(),
        "https://github.com/user/repo.git"
    );
}

#[test]
fn normalize_passes_ssh_scheme_through() {
    assert_eq!(
        normalize_git_url("ssh://git@host/repo.git").unwrap(),
        "ssh://git@host/repo.git"
    );
}

#[test]
fn normalize_rejects_leading_dash() {
    // -oProxyCommand=... and similar option-injection forms would be
    // parsed by git as flags rather than a URL.
    assert!(normalize_git_url("-oProxyCommand=whoami").is_err());
    assert!(normalize_git_url("--upload-pack=evil").is_err());
    assert!(normalize_git_url("-bad").is_err());
}

#[test]
fn normalize_rejects_ext_transport() {
    // git's `ext::` transport can execute arbitrary shell commands
    // and was historically chained with submodule init for RCE.
    assert!(normalize_git_url("ext::sh -c id").is_err());
    assert!(normalize_git_url("https://example.com/ext::evil").is_err());
}

#[test]
fn normalize_rejects_file_scheme() {
    // file:// could be used to read arbitrary repos off disk.
    assert!(normalize_git_url("file:///etc/passwd").is_err());
}

#[test]
fn normalize_rejects_unknown_scheme() {
    assert!(normalize_git_url("git://example.com/repo").is_err());
    assert!(normalize_git_url("ftp://example.com/repo").is_err());
    assert!(normalize_git_url("ssh://nobody@host/repo").is_err()); // requires ssh://git@
}

#[test]
fn normalize_rejects_shorthand_with_meta_chars() {
    assert!(normalize_git_url("user/$(whoami)").is_err());
    assert!(normalize_git_url("user/repo;rm -rf").is_err());
    assert!(normalize_git_url("user/with space").is_err());
}

#[test]
fn normalize_rejects_empty() {
    assert!(normalize_git_url("").is_err());
}

#[test]
fn plugin_name_rejects_leading_dash() {
    // A name like "-rf" would be interpreted as an `rm -rf` flag if
    // it ever lands in a non-`exec`-style context.
    assert_eq!(plugin_name_from_url("user/-rf"), None);
    assert_eq!(plugin_name_from_url("-rf"), None);
}

#[test]
fn plugin_name_rejects_shell_meta_chars() {
    assert_eq!(plugin_name_from_url("user/$(whoami)"), None);
    assert_eq!(plugin_name_from_url("user/repo;rm"), None);
    assert_eq!(plugin_name_from_url("user/with space"), None);
    // Newlines were rejected before; keep that invariant.
    assert_eq!(plugin_name_from_url("user/foo\n"), None);
}

#[tokio::test]
async fn bootstrap_skips_disabled_entries() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Override config dir isn't feasible, but we can verify the function
    // doesn't attempt to clone when entry is disabled
    let entries = vec![crucible_core::config::PluginEntry {
        url: "user/disabled-plugin".to_string(),
        branch: None,
        pin: None,
        enabled: false,
    }];
    // Should succeed without attempting any git operations
    let result = bootstrap_plugins(&entries).await;
    assert!(result.is_ok());
    drop(tmp);
}

// ---------------------------------------------------------------------------
// cru.kiln graph functions, through the registration the daemon performs
// ---------------------------------------------------------------------------
//
// These go through `upgrade_with_storage` over a real `SqliteNoteStore`
// because that is the only wiring production has. The `crucible-lua`-side
// tests can only reach a `NoteStore` mock; nothing but this exercises the
// resolved-link index (`note_links`) that the three functions read.

mod kiln_graph {
    use super::*;
    use crate::storage::sqlite::{SqliteConfig, SqliteNoteStore, SqlitePool};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{NoteRecord, Scope};

    /// Path a `Scope::workspace_unchecked` authority is derived from in
    /// these tests. Never touched on disk — `_unchecked` skips canonicalize.
    const KILN: &str = "/kiln";

    fn note(path: &str, links: &[&str]) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(path.trim_end_matches(".md"))
            .with_links(links.iter().map(|s| (*s).to_string()).collect())
    }

    /// `a.md -> b.md -> c.md`, inserted target-first so every wikilink
    /// resolves on write rather than relying on the re-resolution pass.
    async fn chain_store() -> Arc<dyn NoteStore> {
        let pool = SqlitePool::new(SqliteConfig::memory()).expect("pool");
        let store = SqliteNoteStore::new(pool);
        for record in [
            note("c.md", &[]),
            note("b.md", &["c.md"]),
            note("a.md", &["b.md"]),
        ] {
            store.upsert(record).await.expect("upsert");
        }
        Arc::new(store)
    }

    async fn upgraded_lua(store: Arc<dyn NoteStore>) -> Arc<mlua::Lua> {
        let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
        loader
            .upgrade_with_storage(store, Path::new(KILN), None)
            .expect("upgrade with storage");
        loader.plugin_lua()
    }

    async fn eval_paths(lua: &mlua::Lua, script: &str) -> Vec<String> {
        let table: mlua::Table = lua.load(script).eval_async().await.expect("eval");
        table
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .expect("array of strings")
    }

    #[tokio::test]
    async fn kiln_outlinks_returns_links_from_the_note_store() {
        let lua = upgraded_lua(chain_store().await).await;

        let paths = eval_paths(&lua, r#"return cru.kiln.outlinks("a.md")"#).await;

        assert_eq!(paths, ["b.md"]);
    }

    #[tokio::test]
    async fn kiln_backlinks_returns_sources_from_the_note_store() {
        let lua = upgraded_lua(chain_store().await).await;

        let paths = eval_paths(&lua, r#"return cru.kiln.backlinks("b.md")"#).await;

        assert_eq!(paths, ["a.md"]);
    }

    #[tokio::test]
    async fn kiln_neighbors_walks_the_note_store_graph_to_the_requested_depth() {
        let lua = upgraded_lua(chain_store().await).await;

        // depth 1 = direct links only; a.md's only edge is a.md -> b.md.
        let direct = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 1)"#).await;
        assert_eq!(direct, ["b.md"]);

        // depth 2 reaches c.md through b.md, and never re-includes the start.
        let two_hops = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 2)"#).await;
        assert_eq!(two_hops, ["b.md", "c.md"]);
    }

    #[tokio::test]
    async fn kiln_graph_functions_hide_notes_outside_the_authority_scope() {
        let pool = SqlitePool::new(SqliteConfig::memory()).expect("pool");
        let store = SqliteNoteStore::new(pool);
        for record in [
            note("b.md", &[]),
            note("a.md", &["b.md"]),
            // Same kiln database, different workspace: default-deny.
            note("secret.md", &["b.md"]).with_scope(Scope::workspace_unchecked("/other")),
        ] {
            store.upsert(record).await.expect("upsert");
        }
        let lua = upgraded_lua(Arc::new(store)).await;

        let backlinks = eval_paths(&lua, r#"return cru.kiln.backlinks("b.md")"#).await;
        assert_eq!(backlinks, ["a.md"], "secret.md is out of scope");

        let neighbors = eval_paths(&lua, r#"return cru.kiln.neighbors("b.md", 3)"#).await;
        assert_eq!(neighbors, ["a.md"], "scope holds across the BFS");

        let outlinks = eval_paths(&lua, r#"return cru.kiln.outlinks("secret.md")"#).await;
        assert!(
            outlinks.is_empty(),
            "an out-of-scope source is indistinguishable from a missing one: {outlinks:?}"
        );
    }
}

/// `with_kiln_path_resolver` must also bind the fs roots, because a plugin's
/// `cru.fs.read` and `cru.fs.write` raise on every path when no resolver is
/// registered.
///
/// Regression: the merge that deleted the capability system took
/// `bind_fs_roots` with it — the call AND the definition. The whole workspace
/// still compiled and `just ci` still passed, with `cru.fs` bound to nothing
/// in production, because nothing outside that file named either one.
///
/// A grep would in fact have caught this particular loss, and an earlier
/// version of this comment claimed otherwise. What a grep cannot catch is the
/// other shape: a definition that survives while its single call site dies,
/// which is what a whole-file `--theirs` resolution produces most of the
/// time. This asks the running VM, so neither shape gets through.
///
/// The plugin context matters: `scoped` returns the path unchecked when no
/// plugin is running, so a test that skips `enter_plugin` passes either way.
#[tokio::test]
async fn binding_the_kiln_resolver_also_scopes_a_plugins_file_access() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let kiln = dir.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("create kiln dir");

    let registry = Arc::new(
        crate::kiln_registry::KilnRegistry::from_app_config(
            crate::kiln_registry::KilnRegistryContext::new(
                dir.path().to_path_buf(),
                None,
                dir.path().join("data"),
            ),
            Some(&serde_json::json!({ "kiln_path": kiln.to_string_lossy() })),
        )
        .expect("registry"),
    );
    assert!(
        !registry.entries().is_empty(),
        "the fixture must register a kiln, or the positive assertion below proves nothing"
    );

    let loader = DaemonPluginLoader::new(HashMap::new())
        .expect("loader")
        .with_kiln_path_resolver(registry)
        .expect("kiln path resolver");

    let lua = loader.executor().lua();
    crucible_lua::enter_plugin(lua, "probe", false);

    // `cru.fs.write` returns nothing and RAISES when a path lies outside the
    // roots, so `pcall` is what separates the two answers.
    let check = |path: &std::path::Path| -> bool {
        lua.load(format!(
            r#"return (pcall(cru.fs.write, {:?}, "x"))"#,
            path.to_string_lossy()
        ))
        .eval::<bool>()
        .expect("the pcall itself evaluates")
    };

    assert!(
        check(&kiln.join("ticket.md")),
        "a path inside a registered kiln must be writable; the fs roots resolver is not bound"
    );
    // Without the resolver the first assertion already fails, so this one
    // cannot carry the test alone.
    assert!(
        !check(&dir.path().join("elsewhere.md")),
        "a path outside every root must be refused"
    );
}

/// `make_plugin_inert` states the invariant this proves: "Not Active" must
/// imply "nothing of this plugin's is registered or running."
///
/// It cleared five registries and missed four more — the permission hooks,
/// both session-hook maps, the provider auth hooks and the schedules. None of
/// those four had an owner-keyed clear at all, so a plugin leaked one more
/// copy of each on every reload and kept them after it was marked Error.
#[tokio::test]
async fn a_reload_leaves_one_copy_of_every_registration_and_inert_leaves_none() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("leaky");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        cru.on("pre_tool_call", function(ctx, event) end)
        cru.permissions.on_request(function(request) return nil end)
        cru.on_session_start(function(session) end)
        cru.on_session_end(function(session) end)
        cru.on_provider_auth(function(context) return nil end)
        return { name = "leaky", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let init = dir.join("init.lua");

    // Every name the plugin registered for, and how to count it.
    let counts = |loader: &DaemonPluginLoader| {
        let registry = loader.plugin_handlers();
        [
            registry.runtime_handlers_for("pre_tool_call", None).len(),
            registry
                .runtime_handlers_for("permission:request", Some("bash"))
                .len(),
            registry.runtime_handlers_for("session:start", None).len(),
            registry.runtime_handlers_for("session:end", None).len(),
            registry.runtime_handlers_for("provider:auth", None).len(),
        ]
    };

    for load in 1..=3 {
        loader
            .execute_plugin("leaky", &init)
            .await
            .unwrap_or_else(|e| panic!("load {load}: {e}"));
        assert_eq!(
            counts(&loader),
            [1, 1, 1, 1, 1],
            "load {load} left more than one copy of a registration"
        );
    }

    loader.make_plugin_inert("leaky");
    assert_eq!(
        counts(&loader),
        [0, 0, 0, 0, 0],
        "a plugin marked Not Active still holds live registrations"
    );
    assert_eq!(
        loader.plugin_handlers().plugin_handler_count("leaky"),
        0,
        "clear_owner must reach every store, not five of nine"
    );
}

/// `make_plugin_inert` promises "nothing of this plugin's is registered or
/// RUNNING". The two things an owner has that RUN are a `cru.timer.spawn` task
/// and a `cru.schedule` tick.
///
/// Not the declared-service path, which `services.rs` covers: this is the
/// ad-hoc one a plugin reaches for directly, and `runtime/plugins/discord`
/// uses it. `cru.timer.spawn` dropped its `JoinHandle` on the floor, so a
/// plugin marked Not Active kept ticking for the life of the daemon.
///
/// The counters are read twice with a gap, not once at the clear: the clear is
/// not immediate — a tick already in its callback finishes, and a task stops
/// at its next await — so the test settles first, then holds the count still.
#[tokio::test]
async fn a_plugin_marked_inert_stops_its_spawned_task_and_its_schedule() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("ticker");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        cru.timer.spawn(function()
            while true do
                _G.spawn_ticks = (_G.spawn_ticks or 0) + 1
                cru.timer.sleep(0.005)
            end
        end)
        cru.schedule(0.005, function()
            _G.schedule_ticks = (_G.schedule_ticks or 0) + 1
        end)
        return { name = "ticker", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .execute_plugin("ticker", &dir.join("init.lua"))
        .await
        .expect("load");

    let count = |loader: &DaemonPluginLoader, global: &str| -> i64 {
        loader
            .executor
            .lua()
            .globals()
            .get::<Option<i64>>(global)
            .unwrap()
            .unwrap_or(0)
    };

    // Both must be RUNNING, or a clear that did nothing would pass.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while count(&loader, "spawn_ticks") < 1 || count(&loader, "schedule_ticks") < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the task and the schedule must both run before the clear");

    loader.make_plugin_inert("ticker");

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let spawn_stopped_at = count(&loader, "spawn_ticks");
    let schedule_stopped_at = count(&loader, "schedule_ticks");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        count(&loader, "spawn_ticks"),
        spawn_stopped_at,
        "a plugin marked Not Active still runs the task it spawned"
    );
    assert_eq!(
        count(&loader, "schedule_ticks"),
        schedule_stopped_at,
        "a plugin marked Not Active still runs its schedule"
    );
}

/// A `lua.eval` is its own owner, so its registrations are clearable and hold
/// no interception right.
///
/// Before this, an eval ran with no owner at all, which read as the user's own
/// `init.lua`: the interception grant answered `true`, and no clear path could
/// ever remove what it registered.
#[tokio::test]
async fn an_eval_owns_what_it_registers_and_may_not_intercept() {
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .eval(r#"cru.on("pre_tool_call", function(ctx, event) end)"#)
        .await
        .expect("the eval registers");

    let registry = loader.plugin_handlers();
    let handlers = registry.runtime_handlers_for("pre_tool_call", None);
    assert_eq!(handlers.len(), 1);
    assert_eq!(
        handlers[0].owner,
        crucible_lua::Owner::Eval,
        "an eval must not be attributed to the user's own configuration"
    );
    assert!(
        !handlers[0].may_intercept(),
        "an eval must not take a tool call over"
    );

    // And the clear path reaches it, which it could not while the owner was
    // absent.
    crucible_lua::clear_owner(
        loader.executor().lua(),
        &registry,
        &crucible_lua::Owner::Eval,
    );
    assert!(registry
        .runtime_handlers_for("pre_tool_call", None)
        .is_empty());
}
