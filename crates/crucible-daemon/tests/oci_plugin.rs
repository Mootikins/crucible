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
    let session =
        Session::new(id.to_string()).with_workspace(workspace.to_string_lossy().to_string());
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
#[ignore = "requires podman or docker"]
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
#[ignore = "requires podman or docker"]
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
#[ignore = "requires podman or docker"]
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
