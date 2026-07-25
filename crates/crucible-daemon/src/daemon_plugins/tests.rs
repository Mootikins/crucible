//! Tests for the daemon plugin loader — split from `mod.rs` for the
//! file-size gate; same module path (`daemon_plugins::tests`) as before.
use super::*;

/// Directory holding the plugins that ship with the repo.
pub(crate) fn shipped_plugins_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/plugins"
    ))
}

/// Names of every shipped plugin directory, read from disk so a newly
/// added plugin is covered without editing this list.
pub(crate) fn shipped_plugin_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(shipped_plugins_dir())
        .expect("runtime/plugins must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

/// Discovery alone proves nothing about a plugin's health — `oci` was
/// discovered `Active` for months while dying on its first `require`,
/// because `execute_plugin` errors were downgraded to a `warn!` on a
/// stdout auto-spawn points at /dev/null. This is the Phase-6 smoke:
/// every shipped plugin must load through the REAL loader and *execute* —
/// state `Active`, no `last_error`, and a spec extracted (proof its
/// `init.lua` ran to completion and returned its table).
#[tokio::test]
async fn every_shipped_plugin_executes() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    let info = loader.loaded_plugin_info();
    for name in shipped_plugin_names() {
        let entry = info
            .iter()
            .find(|p| p["name"].as_str() == Some(name.as_str()))
            .unwrap_or_else(|| panic!("shipped plugin '{name}' missing from plugin info"));

        assert_eq!(
            entry["state"].as_str(),
            Some("Active"),
            "shipped plugin '{name}' did not reach Active: {entry:#}"
        );
        let last_error = entry["last_error"].as_str().unwrap_or("");
        assert!(
            last_error.is_empty(),
            "shipped plugin '{name}' recorded an error: {last_error}"
        );
    }
}

/// A shipped plugin whose manifest doesn't parse is not merely broken —
/// it never enters `PluginManager::plugins` at all, so it is absent from
/// `plugin.list` with no error anywhere but the daemon log.
///
/// `reflection` shipped that way: `plugin.yaml` declared the capabilities
/// `session` and `fs`, neither a `Capability` variant.
#[test]
fn every_shipped_plugin_is_discovered() {
    let mut manager = PluginManager::new();
    manager.add_search_path_with_source(shipped_plugins_dir(), PluginSource::Runtime);

    let mut discovered = manager.discover().expect("discovery");
    discovered.sort();

    assert_eq!(
        discovered,
        shipped_plugin_names(),
        "a shipped plugin failed discovery — it will be invisible in `plugin.list`"
    );
}

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
/// per-session VM and the per-`lua.init_session` VM. Anything a plugin is
/// documented to call must be registered *here* — registering it on the
/// other two is invisible to plugins.
///
/// `crucible.on` was missing for exactly this reason: it was registered on
/// the other two VMs, so every hook-registering plugin (`oci`, the
/// reference interception plugin) raised "attempt to call a nil value" at
/// load and was silently downgraded to a `warn!`.
#[test]
fn plugin_runtime_exposes_the_documented_api_surface() {
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let lua = loader.plugin_lua();

    for symbol in [
        "crucible.on",
        "crucible.on_session_start",
        "crucible.on_session_end",
    ] {
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
/// `oci` — the reference interception plugin — registers its `crucible.on`
/// handlers *inside* `on_session_start` (`init.lua:261,306`). Making
/// `crucible.on` callable is not enough on its own: if nothing fires the
/// plugin runtime's lifecycle hooks, that registration never runs.
///
/// `fire_session_start_hooks` was only ever called at `server/lua.rs:34`,
/// on the per-call `lua.init_session` executor — never on the plugin
/// loader's.
#[tokio::test]
async fn plugin_session_lifecycle_hooks_fire() {
    use crucible_lua::{Session, SessionConfigRpc};

    // `SessionConfigRpc`'s methods all have defaults; the hooks under test
    // never call them.
    struct TestRpc;
    impl SessionConfigRpc for TestRpc {}

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        start_fired = false
        end_fired = false
        crucible.on_session_start(function(s) start_fired = true end)
        crucible.on_session_end(function(s) end_fired = true end)
    "#,
        )
        .exec()
        .expect("register lifecycle hooks");

    let session = Session::new("test-session".to_string());
    session.bind(Box::new(TestRpc));

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

/// A lifecycle hook must be able to call the async `cru.*` APIs.
///
/// `cru.shell.exec`, `cru.http.*` and `cru.timer.sleep` are all
/// `create_async_function`s, which cannot suspend under a plain
/// `Function::call`. Firing hooks synchronously meant a plugin could not
/// start a container from `on_session_start` — the entire point of `oci`.
#[tokio::test]
async fn a_lifecycle_hook_can_call_async_apis() {
    use crucible_lua::{Session, SessionConfigRpc};

    struct TestRpc;
    impl SessionConfigRpc for TestRpc {}

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        async_ok = false
        crucible.on_session_start(function(s)
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
    session.bind(Box::new(TestRpc));
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
    use crucible_lua::{Session, SessionConfigRpc};

    struct TestRpc;
    impl SessionConfigRpc for TestRpc {}

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"
        seen_workspace = nil
        readonly_enforced = false
        crucible.on_session_start(function(s)
            seen_workspace = s.workspace
            readonly_enforced = not pcall(function() s.workspace = "/hijacked" end)
        end)
    "#,
        )
        .exec()
        .expect("register hook");

    let session = Session::new("ws-test".to_string()).with_workspace("/tmp/crucible-ws-fixture");
    session.bind(Box::new(TestRpc));
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
    use crucible_lua::{Session, SessionConfigRpc};

    struct TestRpc;
    impl SessionConfigRpc for TestRpc {}

    // An ordinary plugin that raises — logged, session proceeds.
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(r#"crucible.on_session_start(function(s) error("ordinary boom") end)"#)
        .exec()
        .unwrap();
    let session = Session::new("s1".to_string());
    session.bind(Box::new(TestRpc));
    assert!(
        loader.fire_session_start(&session).await.is_ok(),
        "an ordinary plugin's failure must not refuse the session"
    );

    // A plugin that opted in — refuses.
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .plugin_lua()
        .load(
            r#"crucible.on_session_start(function(s) error("gate boom") end, { required = true })"#,
        )
        .exec()
        .unwrap();
    let session = Session::new("s2".to_string());
    session.bind(Box::new(TestRpc));
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
        crucible.on("pre_tool_call", { pattern = "bash" }, function(ctx, event) end)
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
