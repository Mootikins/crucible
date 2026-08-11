//! `oci` — the reference tool-interception plugin — driven through the real
//! daemon plugin runtime.
//!
//! `AGENTS.md` names `oci` as the reference implementation for generic tool
//! interception, and every part of that claim goes through this file: the
//! plugin loads, `setup()` reaches it, `on_session_start` starts a container,
//! and `pre_tool_call` handlers take the tool over. Nothing here reaches into
//! plugin internals — the assertions are made from where the agent stands.
//!
//! Container-backed cases are `#[ignore]`d and runtime-agnostic: they resolve
//! whichever of podman/docker/nerdctl is on PATH, exactly as the plugin does.

use crucible_core::events::SessionEvent;
use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_lua::{PluginSource, ScriptHandlerResult, Session, SessionConfigRpc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `SessionConfigRpc`'s methods all have defaults; nothing under test calls them.
struct StubRpc;
impl SessionConfigRpc for StubRpc {}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

/// A plugin search path holding a copy of the shipped `oci`, and nothing else.
///
/// Pointing the loader at `runtime/plugins` directly would drag in every other
/// shipped plugin, making this suite fail for reasons that have nothing to do
/// with `oci`. The copy is of the real files, so it cannot drift from what ships.
fn oci_search_path(tmp: &Path) -> PathBuf {
    let root = tmp.join("plugins");
    copy_dir(&repo_root().join("runtime/plugins/oci"), &root.join("oci"));
    root
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

async fn load_oci(tmp: &Path, config: serde_json::Value) -> DaemonPluginLoader {
    let config = match config {
        serde_json::Value::Null => HashMap::new(),
        cfg => HashMap::from([("oci".to_string(), cfg)]),
    };
    let mut loader = DaemonPluginLoader::new(config).expect("loader");
    loader
        .load_plugins(&[(oci_search_path(tmp), PluginSource::EnvPath)])
        .await
        .expect("load plugins");
    loader
}

/// Fire `on_session_start` as the daemon does at `session.create`.
///
/// Returns the loader's result: with `[plugins.oci]` configured, `oci`'s hook
/// is `required`, so a failure here is how the daemon knows to refuse the
/// session.
async fn try_start_session(
    loader: &mut DaemonPluginLoader,
    id: &str,
    workspace: &Path,
) -> anyhow::Result<()> {
    try_start_isolated_session(loader, id, workspace, None).await
}

/// `try_start_session` plus the per-session `isolation` param, carried on the
/// `Session` exactly as `SessionLifecycle::fire_session_start` carries it.
async fn try_start_isolated_session(
    loader: &mut DaemonPluginLoader,
    id: &str,
    workspace: &Path,
    isolation: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let mut session =
        Session::new(id.to_string()).with_workspace(workspace.to_string_lossy().to_string());
    if let Some(isolation) = isolation {
        session = session.with_isolation(isolation);
    }
    session.bind(Box::new(StubRpc));
    loader.fire_session_start(&session).await
}

async fn start_session(loader: &mut DaemonPluginLoader, id: &str, workspace: &Path) {
    try_start_session(loader, id, workspace)
        .await
        .expect("session start");
}

async fn end_session(loader: &mut DaemonPluginLoader, id: &str) {
    let session = Session::new(id.to_string());
    session.bind(Box::new(StubRpc));
    loader.fire_session_end(&session).await.expect("end");
}

/// Dispatch a `pre_tool_call` exactly as `agent_manager::messaging::tool_call`
/// does: tool and args in the flattened event, session id in `ctx`.
async fn pre_tool_call(
    loader: &DaemonPluginLoader,
    session_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> ScriptHandlerResult {
    let registry = loader.plugin_handlers();
    let lua = loader.plugin_lua();
    let event = SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({
            "tool": tool,
            "args": args,
        }),
    };
    let handlers = registry.runtime_handlers_for("pre_tool_call", Some(tool));
    assert!(
        !handlers.is_empty(),
        "oci registered no pre_tool_call handler for '{tool}' — the agent would \
         dispatch it straight to the host executor"
    );
    registry
        .execute_runtime_handler(&lua, &handlers[0].name, &event, Some(session_id))
        .await
        .expect("handler execution")
}

fn handled_result(result: &ScriptHandlerResult) -> String {
    match result {
        ScriptHandlerResult::Handled { result, .. } => match result {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        },
        other => panic!("expected the tool to be handled inside the container, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Load, config, registration
// ---------------------------------------------------------------------------

/// The plugin has to *execute* — `require` resolving, `setup()` landing — before
/// any of the rest matters.
///
/// It did not: `init.lua` said `require("lua.container")` while the loader adds
/// `<plugin_dir>/lua/?.lua` to `package.path`, so the module resolved to
/// `lua/lua/container.lua` and the whole file aborted at line 12. The loader
/// downgrades that to a `warn!`, so `plugin.list` still said `Active`.
#[tokio::test]
async fn oci_registers_interception_handlers_for_every_workspace_tool() {
    let tmp = tempfile::tempdir().unwrap();
    let loader = load_oci(tmp.path(), serde_json::Value::Null).await;

    let registry = loader.plugin_handlers();
    for tool in [
        "bash",
        "read_file",
        "write_file",
        "edit_file",
        "glob",
        "grep",
    ] {
        assert!(
            !registry
                .runtime_handlers_for("pre_tool_call", Some(tool))
                .is_empty(),
            "no pre_tool_call handler registered for '{tool}'"
        );
    }
}

/// Isolation is opt-in. With no `[plugins.oci]` image, every tool must reach the
/// normal host executor untouched.
#[tokio::test]
async fn oci_passes_tools_through_when_no_image_is_configured() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(tmp.path(), serde_json::Value::Null).await;
    start_session(&mut loader, "unconfigured", tmp.path()).await;

    let result = pre_tool_call(
        &loader,
        "unconfigured",
        "bash",
        serde_json::json!({ "command": "echo hi" }),
    )
    .await;

    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "unconfigured oci must not intervene, got {result:?}"
    );
}

/// The `[plugins.oci]` section has to reach the plugin.
///
/// It never did: `resolve_config()` asked `cru.config.get("container")` — the
/// *app* config store, under a key no config file writes — so it returned nil,
/// `on_session_start` bailed, and no container was ever created. Config landing
/// means the plugin *acts* on it: with a bogus runtime the required start hook
/// raises, which is how the daemon knows to refuse the session.
#[tokio::test]
async fn oci_reads_its_config_from_the_plugins_section() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({
            "image": "example.invalid/no-such-image:latest",
            "runtime": "crucible-no-such-container-runtime",
        }),
    )
    .await;

    let err = try_start_session(&mut loader, "configured", tmp.path())
        .await
        .expect_err("a configured oci with an unusable runtime must refuse the session");
    assert!(
        err.to_string()
            .contains("crucible-no-such-container-runtime"),
        "refusal must name the cause; got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Per-session opt-in
// ---------------------------------------------------------------------------
//
// The three shapes of `session.create`'s `isolation` param, driven through the
// real plugin. A bogus runtime name stands in for "a container would be
// started here": the plugin refuses the session naming that runtime whenever it
// resolves a configuration, and does nothing at all when it resolves none — so
// which config won is observable without a container runtime installed.

const UNUSABLE: &str = "crucible-no-such-container-runtime";

/// The case the opt-in exists for: a project that containerizes every session,
/// and one session that says no.
///
/// Anything less than the whole resolution short-circuiting here is a session
/// that gets a container it declined.
#[tokio::test]
async fn isolation_false_suppresses_a_container_the_project_config_would_produce() {
    let tmp = tempfile::tempdir().unwrap();
    let config = serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE });

    // Same config, same workspace, no param: refused, because oci tries to
    // containerize and cannot. This is the control — without it the test below
    // proves nothing.
    let mut loader = load_oci(tmp.path(), config.clone()).await;
    try_start_session(&mut loader, "would-containerize", tmp.path())
        .await
        .expect_err("the project config must containerize by default");

    try_start_isolated_session(
        &mut loader,
        "opted-out",
        tmp.path(),
        Some(serde_json::json!(false)),
    )
    .await
    .expect("isolation = false must suppress the project's container entirely");

    // And "no container" has to mean the tools actually run on the host: a
    // session that opted out but still routes through interception would be
    // executing against a container that does not exist.
    let result = pre_tool_call(
        &loader,
        "opted-out",
        "bash",
        serde_json::json!({ "command": "echo hi" }),
    )
    .await;
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "an opted-out session must not be intercepted, got {result:?}"
    );
}

/// A profile name selects a named `[plugins.oci.profiles]` entry over the
/// project's default image.
#[tokio::test]
async fn an_isolation_profile_name_overrides_the_project_default() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({
            "image": "alpine:latest",
            "runtime": UNUSABLE,
            "profiles": {
                "heavy": { "image": "example.invalid/heavy:latest", "runtime": "crucible-heavy-runtime" }
            }
        }),
    )
    .await;

    let err = try_start_isolated_session(
        &mut loader,
        "profiled",
        tmp.path(),
        Some(serde_json::json!("heavy")),
    )
    .await
    .expect_err("a configured profile with an unusable runtime must refuse the session");
    assert!(
        err.to_string().contains("crucible-heavy-runtime"),
        "the named profile must win over the default image; got: {err}"
    );
}

/// An inline object is the same override without a config entry to name.
#[tokio::test]
async fn an_inline_isolation_object_overrides_the_project_default() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_isolated_session(
        &mut loader,
        "inline",
        tmp.path(),
        Some(serde_json::json!({
            "image": "example.invalid/inline:latest",
            "runtime": "crucible-inline-runtime",
        })),
    )
    .await
    .expect_err("an inline isolation object with an unusable runtime must refuse the session");
    assert!(
        err.to_string().contains("crucible-inline-runtime"),
        "the inline object must win over the default image; got: {err}"
    );
}

/// Isolation asked for and not delivered is never a silently unsandboxed
/// session — the same rule the required start hook exists to enforce.
#[tokio::test]
async fn an_unknown_isolation_profile_refuses_the_session() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_isolated_session(
        &mut loader,
        "no-such-profile",
        tmp.path(),
        Some(serde_json::json!("nonexistent")),
    )
    .await
    .expect_err("an unknown profile must refuse rather than fall back to the default");
    assert!(
        err.to_string().contains("nonexistent"),
        "the refusal must name the profile that could not be resolved; got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Devcontainer resolution
// ---------------------------------------------------------------------------
//
// The devcontainer sits between the session param and the profiles in the
// resolution order. Which leg won is observable without a container runtime:
// a devcontainer key the plugin cannot honour refuses the session naming that
// key, while every other leg refuses naming its unusable runtime — so the two
// outcomes say unambiguously which config was read.
//
// This is also the only coverage that runs the parse against the *real*
// `cru.fs` and `cru.json.decode`; the plugin's own Lua suite stubs both.

/// Run git in `workspace`, asserting it succeeded.
///
/// `-c` overrides rather than `git config` so the run does not depend on (or
/// touch) the developer's global identity, and `--no-gpg-sign` so a globally
/// enabled `commit.gpgsign` cannot fail the commit.
fn git(workspace: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["-c", "user.name=crucible-test"])
        .args(["-c", "user.email=test@crucible.invalid"])
        .args(args)
        .output()
        .expect("git is required to exercise committed-only devcontainer resolution");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A workspace whose `.devcontainer/devcontainer.json` is **committed**.
///
/// Committed, because resolution reads HEAD and not the working tree — an
/// uncommitted devcontainer is refused. See
/// `an_uncommitted_devcontainer_is_refused_rather_than_honoured` for why.
fn workspace_with_devcontainer(tmp: &Path, body: &str) -> PathBuf {
    let workspace = workspace_with_uncommitted_devcontainer(tmp, body);
    git(&workspace, &["add", ".devcontainer/devcontainer.json"]);
    git(
        &workspace,
        &["commit", "--no-gpg-sign", "-m", "add devcontainer"],
    );
    workspace
}

/// The same workspace, with the devcontainer left uncommitted in a git repo.
fn workspace_with_uncommitted_devcontainer(tmp: &Path, body: &str) -> PathBuf {
    let workspace = tmp.join("workspace");
    std::fs::create_dir_all(workspace.join(".devcontainer")).unwrap();
    std::fs::write(workspace.join(".devcontainer/devcontainer.json"), body).unwrap();
    git(&workspace, &["init", "-q", "-b", "main"]);
    workspace
}

/// `remoteEnv` has no native equivalent and no `@devcontainers/cli` fallback,
/// so this refusal is the same on any box.
const UNSUPPORTED_DEVCONTAINER: &str = r#"{
    // JSONC: comments and trailing commas are legal here and common in the wild.
    "name": "sample",
    "image": "example.invalid/dc:latest",
    "remoteEnv": { "TOKEN": "hunter2" },
}"#;

/// The settings tree the plugin declares has to survive the trip to a client:
/// declared in Lua, walked by the daemon, rendered by whichever frontend asks.
///
/// Before this there was no such trip — a client wanting to know what a plugin
/// could be configured with read the plugin's raw TOML and matched on its
/// shape, so every option meant editing Rust in the rendering layer.
#[tokio::test]
async fn oci_declares_settings_a_client_can_render() {
    let tmp = tempfile::tempdir().unwrap();
    let loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let tree = loader
        .options()
        .describe("oci", "web")
        .expect("oci declared no options; nothing to render");

    assert_eq!(tree["type"], "group");
    let args = tree["args"].as_array().expect("args array");
    let keys: Vec<&str> = args.iter().filter_map(|a| a["key"].as_str()).collect();
    for expected in ["image", "runtime", "devcontainer"] {
        assert!(keys.contains(&expected), "missing '{expected}' in {keys:?}");
    }

    // Ordered by `order`, not hash order — a settings pane that reshuffles on
    // every read is worse than none.
    assert_eq!(keys.first(), Some(&"image"));

    // Every leaf carries what a renderer needs without knowing what oci is.
    let image = args.iter().find(|a| a["key"] == "image").unwrap();
    assert_eq!(image["type"], "input");
    assert!(image["name"].is_string());
    assert!(image["desc"].is_string());
    assert_eq!(
        image["writable"], true,
        "the root `set` must reach the leaf"
    );
}

/// Reading and writing go through the plugin's own accessors, so the daemon
/// never learns what an option means.
#[tokio::test]
async fn oci_settings_round_trip_through_the_plugins_accessors() {
    let tmp = tempfile::tempdir().unwrap();
    let loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;
    let options = loader.options();
    let path = vec!["image".to_string()];

    assert_eq!(
        options.get("oci", &path, "web").expect("get"),
        serde_json::json!("alpine:latest")
    );

    options
        .set("oci", &path, serde_json::json!("debian:trixie"), "web")
        .expect("set");
    assert_eq!(
        options.get("oci", &path, "web").expect("get after set"),
        serde_json::json!("debian:trixie"),
        "a write that does not read back is a settings pane that lies"
    );
}

/// A devcontainer is container configuration, and the sandboxed agent can
/// write it: `/workspace` is the host workspace through a rw bind mount, so a
/// `write_file` to `.devcontainer/devcontainer.json` lands on the host and the
/// *next* session in that workspace resolves from it.
///
/// What stops that is not who wrote the file — reading only committed config
/// was tried and did not hold, since `.git` is inside the same mount and the
/// agent can commit — but what the file is allowed to ask for. `runArgs`
/// becomes raw runtime argv, so this is the escape that gate exists for.
///
/// Uncommitted on purpose: the file being unstaged must not be what saves us.
#[tokio::test]
async fn a_devcontainer_asking_to_reach_the_host_is_refused_however_it_was_written() {
    let tmp = tempfile::tempdir().unwrap();
    let escape = r#"{
        "image": "example.invalid/dc:latest",
        "runArgs": ["--privileged", "-v", "/:/host"]
    }"#;
    let workspace = workspace_with_uncommitted_devcontainer(tmp.path(), escape);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_session(&mut loader, "dc-escape", &workspace)
        .await
        .expect_err("a devcontainer asking for --privileged must not configure a container");
    let err = err.to_string();
    assert!(
        err.contains("runArgs"),
        "the refusal must name the key that was refused; got: {err}"
    );
    assert!(
        err.contains("devcontainer_host_access"),
        "and must say how to permit it deliberately; got: {err}"
    );
}

/// ...and the ordinary uncommitted edit still works, which is the whole reason
/// the commit rule went away: you change a devcontainer to test it.
#[tokio::test]
async fn an_uncommitted_devcontainer_that_asks_for_nothing_special_is_honoured() {
    let tmp = tempfile::tempdir().unwrap();
    let benign = r#"{ "image": "example.invalid/dc:latest" }"#;
    let workspace = workspace_with_uncommitted_devcontainer(tmp.path(), benign);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    // The runtime is unusable, so the session still fails — but on the RUNTIME,
    // which proves the devcontainer was read and accepted rather than skipped.
    let err = try_start_session(&mut loader, "dc-benign", &workspace)
        .await
        .expect_err("the configured runtime does not exist")
        .to_string();
    assert!(
        !err.contains("commit") && !err.contains("devcontainer_host_access"),
        "an uncommitted devcontainer asking for nothing special must not be refused; got: {err}"
    );
}

/// The devcontainer is the project's own statement of its environment, so it
/// outranks a `[plugins.oci]` profile describing the same project.
///
/// Refusing rather than falling back is the load-bearing half: an environment
/// asked for and not delivered is the same failure as a sandbox that silently
/// did not start.
#[tokio::test]
async fn a_devcontainer_key_that_cannot_be_honoured_refuses_the_session_naming_it() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_session(&mut loader, "dc-unsupported", &workspace)
        .await
        .expect_err("a devcontainer key that cannot be honoured must refuse the session");
    assert!(
        err.to_string().contains("remoteEnv"),
        "the refusal must name the key that could not be honoured; got: {err}"
    );
    assert!(
        !err.to_string().contains(UNUSABLE),
        "the devcontainer must outrank the configured image, not fall back to it; got: {err}"
    );
}

/// `devcontainer = false` puts the project back on its profile.
#[tokio::test]
async fn a_project_can_opt_out_of_its_devcontainer() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({
            "image": "alpine:latest",
            "runtime": UNUSABLE,
            "devcontainer": false,
        }),
    )
    .await;

    let err = try_start_session(&mut loader, "dc-opted-out", &workspace)
        .await
        .expect_err("the profile must still containerize");
    assert!(
        err.to_string().contains(UNUSABLE),
        "with the devcontainer switched off the profile must win; got: {err}"
    );
}

/// An explicit session param is first in the order, ahead of the devcontainer.
#[tokio::test]
async fn an_inline_isolation_object_outranks_the_devcontainer() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_isolated_session(
        &mut loader,
        "dc-overridden",
        &workspace,
        Some(serde_json::json!({
            "image": "example.invalid/inline:latest",
            "runtime": "crucible-inline-runtime",
        })),
    )
    .await
    .expect_err("the inline object's unusable runtime must refuse the session");
    assert!(
        err.to_string().contains("crucible-inline-runtime"),
        "an explicit session param must be resolved before the devcontainer; got: {err}"
    );
}

/// `isolation = false` short-circuits the whole resolution, devcontainer
/// included — a session that declined a container does not get one from a file
/// it never asked to be read.
#[tokio::test]
async fn isolation_false_suppresses_a_devcontainer_too() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "image": "alpine:latest", "runtime": UNUSABLE }),
    )
    .await;

    try_start_isolated_session(
        &mut loader,
        "dc-declined",
        &workspace,
        Some(serde_json::json!(false)),
    )
    .await
    .expect("an opted-out session must not be refused by a devcontainer it never read");
}

/// The plugin ships enabled in every install, so a repo that merely *contains*
/// a devcontainer must not become a containerized one.
///
/// Without this gate, checking out any repo with a `.devcontainer/` would
/// containerize every session in it — and on a box with no container runtime,
/// refuse every one of them.
#[tokio::test]
async fn a_devcontainer_alone_does_not_containerize_a_project_that_never_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(tmp.path(), serde_json::Value::Null).await;

    start_session(&mut loader, "dc-unasked", &workspace).await;
    let result = pre_tool_call(
        &loader,
        "dc-unasked",
        "bash",
        serde_json::json!({ "command": "echo hi" }),
    )
    .await;
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "a project that configured no isolation must not be containerized by a \
         devcontainer.json alone, got {result:?}"
    );
}

/// The opt-in for a project whose environment is entirely its devcontainer:
/// no image to name in `crucible.toml`, just `devcontainer = true`.
#[tokio::test]
async fn devcontainer_true_opts_a_project_in_with_no_image_configured() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = workspace_with_devcontainer(tmp.path(), UNSUPPORTED_DEVCONTAINER);
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({ "devcontainer": true, "runtime": UNUSABLE }),
    )
    .await;

    let err = try_start_session(&mut loader, "dc-true", &workspace)
        .await
        .expect_err("devcontainer = true must resolve the project's devcontainer");
    assert!(
        err.to_string().contains("remoteEnv"),
        "the devcontainer must have been read; got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Degradation
// ---------------------------------------------------------------------------

/// Isolation that cannot be established must not silently become no isolation.
///
/// The old behaviour was one `cru.log("error", ...)` and a bare `return`: the
/// session ran every tool on the host while the operator believed it was
/// contained. Now the start hook is `required`, so the failure propagates and
/// the caller refuses the session — and the failed start must leave no
/// half-registered state behind, or the *next* session's tools would route
/// into a container that never existed.
#[tokio::test]
async fn oci_fails_closed_when_the_container_runtime_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({
            "image": "example.invalid/no-such-image:latest",
            "runtime": "crucible-no-such-container-runtime",
        }),
    )
    .await;

    let err = try_start_session(&mut loader, "blocked", tmp.path())
        .await
        .expect_err("start must fail when the runtime is missing");
    assert!(
        err.to_string()
            .contains("crucible-no-such-container-runtime"),
        "refusal must name the cause; got: {err}"
    );

    // The session was refused, so nothing should key to it: its tool calls
    // (which the daemon will never make) pass through rather than routing
    // into a phantom container.
    for (tool, args) in [
        ("bash", serde_json::json!({ "command": "id" })),
        (
            "write_file",
            serde_json::json!({ "path": "x", "content": "y" }),
        ),
        ("read_file", serde_json::json!({ "path": "x" })),
    ] {
        let result = pre_tool_call(&loader, "blocked", tool, args).await;
        assert!(
            matches!(result, ScriptHandlerResult::PassThrough),
            "a refused session must leave no interception state, got {result:?}"
        );
    }
}

/// Two sessions share one plugin VM and one set of load-time handlers. A
/// session `oci` never started must not inherit interception state from one it
/// refused — state keyed by anything but the session id would do exactly that.
#[tokio::test]
async fn oci_keeps_sessions_isolated_from_each_other() {
    let tmp = tempfile::tempdir().unwrap();
    let mut loader = load_oci(
        tmp.path(),
        serde_json::json!({
            "image": "example.invalid/no-such-image:latest",
            "runtime": "crucible-no-such-container-runtime",
        }),
    )
    .await;

    try_start_session(&mut loader, "refused", tmp.path())
        .await
        .expect_err("bogus runtime must refuse the session");

    for id in ["refused", "never-started"] {
        let result =
            pre_tool_call(&loader, id, "bash", serde_json::json!({ "command": "id" })).await;
        assert!(
            matches!(result, ScriptHandlerResult::PassThrough),
            "session '{id}' must not inherit another session's state, got {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Real containers
// ---------------------------------------------------------------------------

fn available_runtime() -> Option<&'static str> {
    ["podman", "docker", "nerdctl"]
        .into_iter()
        .find(|rt| which(rt))
}

fn which(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(cmd).exists()))
}

/// An image the runtime already has locally, so the test does not depend on a
/// registry being reachable.
const TEST_IMAGE: &str = "docker.io/library/alpine:latest";

fn image_present(runtime: &str) -> bool {
    std::process::Command::new(runtime)
        .args(["image", "inspect", TEST_IMAGE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

async fn load_containerised(tmp: &Path, runtime: &str) -> DaemonPluginLoader {
    load_oci(
        tmp,
        serde_json::json!({ "image": TEST_IMAGE, "runtime": runtime }),
    )
    .await
}

/// The whole point of the plugin: `bash` runs in the container, not on the host.
#[tokio::test]
#[ignore = "requires: container runtime"]
async fn oci_runs_bash_inside_the_container() {
    let Some(runtime) = available_runtime() else {
        panic!("no container runtime on PATH");
    };
    assert!(image_present(runtime), "pull {TEST_IMAGE} first");

    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("marker.txt"), "from-host\n").unwrap();

    let mut loader = load_containerised(tmp.path(), runtime).await;
    start_session(&mut loader, "container-bash", &workspace).await;

    let out = handled_result(
        &pre_tool_call(
            &loader,
            "container-bash",
            "bash",
            serde_json::json!({ "command": "cat /etc/os-release; cat marker.txt" }),
        )
        .await,
    );
    end_session(&mut loader, "container-bash").await;

    assert!(
        out.contains("Alpine"),
        "command did not run in the container image; got: {out}"
    );
    assert!(
        out.contains("from-host"),
        "workspace was not mounted at /workspace; got: {out}"
    );
}

/// A file the agent writes has to be usable by the human afterwards.
///
/// This is the uid-mapping case: under rootless podman a bind-mounted write can
/// land owned by a subuid the host user cannot touch.
#[tokio::test]
#[ignore = "requires: container runtime"]
async fn oci_writes_workspace_files_owned_by_the_host_user() {
    let Some(runtime) = available_runtime() else {
        panic!("no container runtime on PATH");
    };
    assert!(image_present(runtime), "pull {TEST_IMAGE} first");

    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let mut loader = load_containerised(tmp.path(), runtime).await;
    start_session(&mut loader, "container-write", &workspace).await;

    pre_tool_call(
        &loader,
        "container-write",
        "write_file",
        serde_json::json!({ "path": "written.txt", "content": "hello from the container\n" }),
    )
    .await;
    end_session(&mut loader, "container-write").await;

    let written = workspace.join("written.txt");
    assert!(
        written.exists(),
        "write_file did not reach the host workspace"
    );
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        "hello from the container\n"
    );

    // A file this process created is the reference for "the host user's uid" —
    // no libc needed.
    use std::os::unix::fs::MetadataExt;
    let reference = tmp.path().join("reference.txt");
    std::fs::write(&reference, "").unwrap();
    let me = std::fs::metadata(&reference).unwrap().uid();
    let owner = std::fs::metadata(&written).unwrap().uid();
    assert_eq!(
        owner, me,
        "file written from the container is owned by uid {owner}, not the host user {me} — \
         the workspace mount needs a uid mapping"
    );
}

/// Teardown has to actually remove the container, or a long-lived daemon leaks
/// one per session.
#[tokio::test]
#[ignore = "requires: container runtime"]
async fn oci_removes_the_container_on_session_end() {
    let Some(runtime) = available_runtime() else {
        panic!("no container runtime on PATH");
    };
    assert!(image_present(runtime), "pull {TEST_IMAGE} first");

    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let mut loader = load_containerised(tmp.path(), runtime).await;
    start_session(&mut loader, "container-teardown", &workspace).await;

    let name = "crucible-container-teardown";
    let running = |name: &str| {
        std::process::Command::new(runtime)
            .args(["inspect", "--format", "{{.State.Running}}", name])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("true"))
    };
    assert!(running(name), "container was never started");

    end_session(&mut loader, "container-teardown").await;
    assert!(!running(name), "container survived session end");
}
