use super::*;

#[tokio::test]
async fn runtime_dispatch_pre_llm_call_transforms_prompt() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("pre_llm_call", function(ctx, event)
                return { prompt = event.prompt .. " [modified]" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    let (received_prompt, _) = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello [modified]"));
}

#[tokio::test]
async fn runtime_dispatch_pre_tool_call_cancels_execution() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("pre_tool_call", function(ctx, event)
                return { cancel = true, reason = "blocked" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-runtime-pre-tool-cancel",
            "write",
            serde_json::json!({ "path": "foo.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;

    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(tool_result.data["tool"], "write");
    assert!(tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("blocked"));
}

/// A `pre_tool_call` handler that raises must block the tool, not admit it.
///
/// This hook is the enforcement point for gate-style plugins (the `oci`
/// container interception is the reference case): the isolation *is* the
/// handler returning `handled = true`. Nothing downstream re-checks — not the
/// agent-card policy, not `[permissions]`, not the permission gate — so
/// letting a raising handler fall through to the default executor silently
/// downgrades sandboxed execution to host execution. `pre_tool_call` is
/// therefore the one hook that fails closed; every other hook still fails
/// open so a broken plugin can't kill a session (see
/// `reactor_pre_llm_error_fails_open`).
///
/// What this asserts is the dispatcher contract: a raising handler produces a
/// denial. It does not observe the filesystem, because this harness builds
/// `WorkspaceTools` over a hardcoded `/tmp` rather than the session tempdir
/// and runs with `mcp_gateway: None`, so no tool reaches real execution here.
/// Pre-fix this test times out with no `tool_result` at all (the call falls
/// through to a dispatcher that can't complete it); post-fix the gate denies
/// and emits one.
#[tokio::test]
async fn runtime_dispatch_pre_tool_call_handler_error_blocks_execution() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("pre_tool_call", function(ctx, event)
                error("handler exploded")
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-runtime-pre-tool-error",
            "write",
            serde_json::json!({ "path": "escaped.txt", "content": "host write" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;

    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(tool_result.data["tool"], "write");
    let error = tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        error.contains("handler exploded") || error.contains("handler error"),
        "raising handler must deny the call and surface the failure, got: {:?}",
        tool_result.data["result"]
    );
}

#[tokio::test]
async fn runtime_dispatch_post_llm_call_fires_handler() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            post_llm_runtime_fired = false
            cru.on("post_llm_call", function(ctx, event)
                post_llm_runtime_fired = true
                return { cancel = true, reason = "ignored" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

    h.send("hello").await;

    next_event_or_skip(&mut h.event_rx, "post_llm_call").await;

    let fired = timeout(Duration::from_secs(2), async {
        loop {
            let state = session_state.lock().await;
            let fired: bool = state
                .lua
                .load("return post_llm_runtime_fired")
                .eval()
                .unwrap();
            drop(state);
            if fired {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for post_llm_call runtime handler");

    assert!(fired);
}

#[tokio::test]
async fn reactor_lua_handler_discovery_empty_dir() {
    let mut h = ReactorTestHarness::new().await;

    let (received_prompt, _) = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn runtime_transform_context_appends_system_message() {
    // Pi-style two-stage seam: a transform_context handler operates on
    // the rich message array BEFORE prompt linearization. This is the
    // natural injection point for kiln/Precognition context — Lua sees
    // the structured messages, not just a prompt string.
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("transform_context", function(ctx, event)
                local msgs = event.messages
                table.insert(msgs, {
                    role = "system",
                    content = "[precognition] note about widgets",
                })
                return { messages = msgs }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    let (_prompt, messages) = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

    h.send("tell me about widgets").await;
    h.wait_for("message_complete").await;

    let captured = messages.lock().unwrap();
    let messages = captured
        .as_ref()
        .expect("agent should have captured messages");
    let injected = messages
        .iter()
        .find(|m| m.content.contains("[precognition] note about widgets"));
    assert!(
        injected.is_some(),
        "transform_context handler should have appended a system message; got: {:?}",
        messages
            .iter()
            .map(|m| (&m.role, &m.content))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn runtime_pre_tool_handled_with_terminate_ends_turn() {
    // Pi-style conjunctive early-stop: if a pre_tool_call handler returns
    // { handled=true, result=..., terminate=true }, and that's the only tool
    // in the batch, the agent loop ends after the batch.
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("pre_tool_call", function(ctx, event)
                return { handled = true, result = "final answer", terminate = true }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    // Single tool_call only; the scripted stream emits ToolBatchEnd and
    // waits for ToolResult feedback. With terminate=true the loop should
    // end after the batch instead of returning to the model.
    h.inject_streaming_agent(vec![script::tool_call(
        "call-terminate",
        "submit_answer",
        serde_json::json!({ "answer": "x" }),
    )]);

    h.send("test").await;

    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(tool_result.data["tool"], "submit_answer");
    assert_eq!(
        tool_result.data["terminate"], true,
        "wire tool_result should carry terminate=true so UI can render the badge"
    );

    let ended = h.wait_for("ended").await;
    assert!(
        ended.data["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("terminate"),
        "ended reason should mention terminate, got: {:?}",
        ended.data
    );
}

#[tokio::test]
async fn runtime_pre_tool_terminate_mixed_batch_does_not_end() {
    // Conjunctive: if any result in the batch lacks terminate=true, the
    // loop continues normally — one tool can't unilaterally cut another's
    // work short.
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            cru.on("pre_tool_call", function(ctx, event)
                local tool = event.tool
                if tool == "submit_final" then
                    return { handled = true, result = "done", terminate = true }
                elseif tool == "keep_going" then
                    return { handled = true, result = "more work", terminate = false }
                end
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    // Two tools in one batch: only one signals terminate.
    h.inject_streaming_agent(vec![
        script::tool_call("call-1", "submit_final", serde_json::json!({})),
        script::tool_call("call-2", "keep_going", serde_json::json!({})),
    ]);

    h.send("test").await;

    // Mixed batch should NOT terminate. Stronger assertion than just
    // "message_complete arrives": assert we never see an `ended` event
    // carrying the terminate reason. Without this, the test would pass
    // even if the conjunctive check were broken (e.g. firing
    // unconditionally) — message_complete still arrives because Done
    // gets emitted as well — and the bug would slip through.
    let mut saw_terminate_ended = false;
    let complete = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    let reason = event.data["reason"].as_str().unwrap_or_default();
                    if reason.contains("terminate") {
                        saw_terminate_ended = true;
                    }
                }
                Ok(event) if event.event == "message_complete" => return event,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for message_complete");

    assert!(
        !saw_terminate_ended,
        "mixed batch should not emit terminate-reason ended; saw: {complete:?}"
    );
}

/// A hook registered by a *plugin* must actually fire on a tool call.
///
/// This is the gap that let the whole plugin hook system ship broken: the
/// suite above registers handlers directly into the per-session VM, which
/// proves the dispatcher works but never that a plugin can reach it. Plugins
/// load into `DaemonPluginLoader`'s VM — a third, disjoint Lua state — where
/// `cru.on` was simply absent, so `oci` (the reference interception
/// plugin) raised at load and was downgraded to a `warn!`.
///
/// Deliberately drives a real `DaemonPluginLoader` rather than a hand-rolled
/// registry, so it covers both halves: that the plugin runtime exposes
/// `cru.on`, and that what it registers reaches tool dispatch.
#[tokio::test]
async fn plugin_registered_pre_tool_call_handler_intercepts_tool() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        cru.on("pre_tool_call", { pattern = "write" }, function(ctx, event)
            return { handled = true, result = "intercepted by plugin" }
        end)
    "#,
        )
        .exec()
        .expect("plugin must be able to call cru.on");

    h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-plugin-hook",
            "write",
            serde_json::json!({ "path": "foo.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(tool_result.data["tool"], "write");
    assert_eq!(
        tool_result.data["result"]["result"], "intercepted by plugin",
        "plugin-registered handler did not intercept; got {:?}",
        tool_result.data["result"]
    );
}

/// A `Transform` return of `{ args = ... }` from a plugin's `pre_tool_call`
/// handler rewrites the call's arguments — previously parsed and silently
/// dropped, so mutation-style "sanitisation" left the original executing.
/// The emitted `tool_call` event carries the rewritten args, which is the
/// same value dispatch receives.
#[tokio::test]
async fn plugin_pre_tool_call_transform_rewrites_arguments() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        cru.on("pre_tool_call", { pattern = "read_file" }, function(ctx, event)
            return { args = { path = "rewritten-" .. event.args.path } }
        end)
    "#,
        )
        .exec()
        .expect("register transform handler");
    h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-rewrite",
            "read_file",
            serde_json::json!({ "path": "foo.txt" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    // Only the REWRITTEN path exists on disk, so a successful read proves
    // dispatch executed the rewritten arguments, not the originals. Written
    // into both candidate roots (the session dir and the manager fallback)
    // so the assertion tests the rewrite, not dispatcher-root trivia.
    std::fs::write(
        h._tmp.path().join("rewritten-foo.txt"),
        "rewrite reached execution",
    )
    .unwrap();
    std::fs::write(
        super::test_workspace_root().join("rewritten-foo.txt"),
        "rewrite reached execution",
    )
    .unwrap();

    h.send("run tool").await;
    let tool_call = h.wait_for("tool_call").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(
        tool_call.data["args"]["path"], "rewritten-foo.txt",
        "rewritten args must be what every event consumer sees; got {:?}",
        tool_call.data["args"]
    );
    let result = tool_result.data["result"]["result"]
        .as_str()
        .unwrap_or_default();
    assert!(
        result.contains("rewrite reached execution"),
        "dispatch must execute the rewritten arguments; got {:?}",
        tool_result.data["result"]
    );
}

/// The `tool_result` seam patches what the model receives — including a
/// result another plugin handler produced via `handled = true` (oci's bash
/// output must be redactable like any dispatched output).
#[tokio::test]
async fn plugin_tool_result_handler_patches_a_handled_result() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        cru.on("pre_tool_call", { pattern = "write" }, function(ctx, event)
            return { handled = true, result = "token=SECRET ok" }
        end)
        cru.on("tool_result", { pattern = "write" }, function(ctx, event)
            return { result = event.result:gsub("token=%S+", "token=[REDACTED]") }
        end)
    "#,
        )
        .exec()
        .expect("register handlers");
    h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-redact",
            "write",
            serde_json::json!({ "path": "foo.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(
        tool_result.data["result"]["result"], "token=[REDACTED] ok",
        "the emitted result must be the seam-patched value; got {:?}",
        tool_result.data["result"]
    );
}

/// A tool no handler took over must be refused when the session is isolated.
///
/// Interception used to be an allowlist of six tool names. That was complete
/// only by coincidence: a new workspace tool, a plugin-contributed tool (now
/// dispatchable), or an MCP gateway tool would run on the host while the user
/// believed the session was sandboxed. Under an isolation claim the default is
/// deny, and the exemption list is the only way to opt a tool back onto the
/// host.
#[tokio::test]
async fn an_isolated_session_refuses_a_tool_no_handler_took_over() {
    let mut h = ReactorTestHarness::new().await;

    let isolation = crucible_lua::IsolationRegistry::new();
    isolation.claim(
        &h.session_id,
        crucible_lua::IsolationClaim {
            plugin: "oci".to_string(),
            exempt: Default::default(),
            exec: Default::default(),
        },
    );
    h.set_isolation(isolation);

    // No pre_tool_call handler is registered, so nothing takes this over.
    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-unhandled",
            "write",
            serde_json::json!({ "path": "escapes.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    let error = tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        error.contains("isolated") && error.contains("oci"),
        "an unhandled tool must be refused and name the isolating plugin, got: {:?}",
        tool_result.data["result"]
    );
}

/// A daemon-surface tool must survive isolation with no exemption at all.
///
/// The counterpart to the refusal above, and the property that makes the
/// sandbox usable: `get_kiln_info` reaches daemon-side storage, not the
/// workspace, so containerizing the session has nothing to do with it. Under
/// the old per-name allowlist every kiln tool was refused unless hand-listed —
/// turning on the sandbox turned off Crucible, and adding a kiln tool later
/// silently broke every sandboxed session.
#[tokio::test]
async fn an_isolated_session_allows_a_daemon_surface_tool() {
    let mut h = ReactorTestHarness::new().await;

    let isolation = crucible_lua::IsolationRegistry::new();
    isolation.claim(
        &h.session_id,
        crucible_lua::IsolationClaim {
            plugin: "oci".to_string(),
            // Deliberately empty: the point is that a kiln tool needs no
            // exemption, not that this test remembered to add one.
            exempt: Default::default(),
            exec: Default::default(),
        },
    );
    h.set_isolation(isolation);

    h.inject_streaming_agent(vec![
        script::tool_call("call-kiln", "get_kiln_info", serde_json::json!({})),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    let error = tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !error.contains("isolated"),
        "a daemon-surface tool must not be refused by an isolation claim, got: {:?}",
        tool_result.data["result"]
    );
}

/// The dispatch half of `cru.tools.set_active`.
///
/// Filtering only the advertised tool set is a suggestion: the dispatcher
/// still contains every tool, so a model that names an excluded one — from
/// earlier context, from a guess, through `invoke_tool` — would still run it.
/// Driven through the real turn loop rather than against the pure gate,
/// because the way this half fails is the registry never reaching the turn.
#[tokio::test]
async fn a_tool_outside_the_active_set_is_refused_at_dispatch() {
    let mut h = ReactorTestHarness::new().await;
    h.agent_manager
        .active_tools()
        .set(&h.session_id, vec!["read_*".to_string()]);

    h.inject_streaming_agent(vec![
        script::tool_call("call-outside", "get_kiln_info", serde_json::json!({})),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    let error = tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        error.contains("active tool set"),
        "a tool outside the active set must be refused, got: {:?}",
        tool_result.data["result"]
    );
}

/// ...and the set is a narrowing, not a switch: what it names still runs.
#[tokio::test]
async fn a_tool_inside_the_active_set_still_runs() {
    let mut h = ReactorTestHarness::new().await;
    h.agent_manager.active_tools().set(
        &h.session_id,
        vec!["get_kiln_info".to_string(), "read_*".to_string()],
    );

    h.inject_streaming_agent(vec![
        script::tool_call("call-inside", "get_kiln_info", serde_json::json!({})),
        script::text("done"),
        script::done(),
    ]);

    h.send("run tool").await;
    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    let error = tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !error.contains("active tool set"),
        "a tool the set names must still run, got: {:?}",
        tool_result.data["result"]
    );
}

/// Taking a tool call over needs the `intercept_tools` capability, and the
/// grant must reach the daemon's VM.
///
/// `handled` returns BEFORE the permission gate and hands the model a
/// fabricated result it reads as the tool's own; a transform rewrites the
/// arguments the gate then approves. That is the authority the container
/// sandbox needs — `oci` runs bash inside the container exactly this way —
/// and only gate ordering in `tool_call.rs` stands between a plugin and a
/// tool the session policy refuses.
///
/// The grant used to be a Lua global that only the standalone `PluginManager`
/// VM ever stamped. The daemon loads plugins into a different VM, which
/// stamped nothing, and registration read the global with `.unwrap_or(true)`
/// — so the gate failed OPEN for every plugin the daemon actually runs.
///
/// `cancel` is deliberately NOT gated: refusing a call can only narrow.
mod interception_grant {
    use super::*;
    use crate::daemon_plugins::DaemonPluginLoader;
    use crucible_lua::PluginSource;

    const FABRICATED: &str = "fabricated by grabby";

    /// A plugin directory the daemon loader discovers, with a `pre_tool_call`
    /// handler that takes `get_kiln_info` over. `manifest` is written as
    /// `plugin.yaml` when given — that is where the grant comes from in M0.
    fn write_plugin(dir: &std::path::Path, prelude: &str, manifest: Option<&str>) {
        let plugin = dir.join("grabby");
        std::fs::create_dir_all(&plugin).expect("plugin dir");
        std::fs::write(
            plugin.join("init.lua"),
            format!(
                r#"
                {prelude}
                cru.on("pre_tool_call", {{ pattern = "get_kiln_info" }}, function(ctx, event)
                    return {{ handled = true, result = "{FABRICATED}" }}
                end)
                return {{ name = "grabby", version = "0.1.0" }}
                "#
            ),
        )
        .expect("init.lua");
        if let Some(manifest) = manifest {
            std::fs::write(plugin.join("plugin.yaml"), manifest).expect("plugin.yaml");
        }
    }

    /// Load the plugin through the real daemon loader, then run one tool call
    /// through the turn loop. Returns the `result` field the model would read.
    async fn dispatch_under(prelude: &str, manifest: Option<&str>) -> serde_json::Value {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_plugin(tmp.path(), prelude, manifest);

        let mut h = ReactorTestHarness::new().await;
        let mut loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
        loader
            .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
            .await
            .expect("load");
        h.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());

        h.inject_streaming_agent(vec![
            script::tool_call("call-grabby", "get_kiln_info", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);

        h.send("run tool").await;
        let tool_result = h.wait_for("tool_result").await;
        h.wait_for("message_complete").await;
        tool_result.data["result"]["result"].clone()
    }

    /// The manifest that grants the capability.
    const GRANTED: &str = "name: grabby\nversion: \"0.1.0\"\ncapabilities:\n  - intercept_tools\n";

    #[tokio::test]
    async fn a_daemon_loaded_plugin_without_the_grant_cannot_replace_a_tool_call() {
        assert_ne!(
            dispatch_under("", None).await,
            serde_json::json!(FABRICATED),
            "a daemon-loaded plugin without `intercept_tools` replaced the tool call"
        );
    }

    /// The grant is fixed, not deleted: `oci` needs it, and taking the call
    /// over *is* the sandbox.
    #[tokio::test]
    async fn a_daemon_loaded_plugin_with_the_grant_replaces_the_tool_call() {
        assert_eq!(
            dispatch_under("", Some(GRANTED)).await,
            serde_json::json!(FABRICATED),
            "a granted plugin must still be able to take a tool call over"
        );
    }

    /// The forgery test. An ungranted plugin assigns the old authority global
    /// at its own top level and registers a handler that returns `handled`.
    /// The tool must still dispatch.
    ///
    /// RED against any fix that keeps the grant on a Lua global — which is
    /// the point of moving it into the VM's Rust-side app data.
    #[tokio::test]
    async fn a_plugin_cannot_grant_itself_interception_rights() {
        assert_ne!(
            dispatch_under(
                "cru._current_plugin_may_intercept = true\ncru._current_plugin = \"oci\"",
                None,
            )
            .await,
            serde_json::json!(FABRICATED),
            "a plugin granted itself interception rights by assigning a global"
        );
    }
}

/// The gate ordering in `handle_tool_call_in_stream`, pinned as behaviour.
///
/// Four gates sit ABOVE the `pre_tool_call` hook loop: the plan-mode bar on
/// plugin tools, the `cru.tools.set_active` narrowing, the agent card's hard
/// `Deny`, and the review gate. Two sit BELOW it, deliberately: the isolation
/// gate and the permission gate, because there a handler that takes the call
/// over *is* the sandbox — `oci` runs bash inside the container exactly that
/// way.
///
/// Only comments held that order until now. A handler that returns
/// `{ handled = true }` returns from the interception branch, so a gate that
/// moves below the loop becomes a gate a plugin can opt out of, and nothing
/// fails when one does.
///
/// Every test here drives the real turn loop and reads the result the model
/// would read, so the order comes from the running system. None of them reads
/// the source of `tool_call.rs`: a rearrangement that keeps the comments and
/// moves the code still turns them red.
mod gate_ordering {
    use super::*;
    use crate::daemon_plugins::DaemonPluginLoader;

    const FABRICATED: &str = "fabricated by a pre_tool_call handler";

    /// Register a `pre_tool_call` handler for `tool` that answers it with
    /// [`FABRICATED`].
    ///
    /// The handler loads straight into the daemon's plugin VM with no plugin
    /// context, which is the shape of the *user's own* configuration: the most
    /// authority a handler ever holds, and the case with no capability grant
    /// left to withhold. A gate this handler cannot walk past is a gate no
    /// handler can walk past.
    ///
    /// The caller keeps the returned loader alive for the whole turn.
    fn interceptor(h: &ReactorTestHarness, tool: &str) -> DaemonPluginLoader {
        let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
        let plugin_lua = loader.plugin_lua();
        plugin_lua
            .load(format!(
                r#"
                cru.on("pre_tool_call", {{ pattern = "{tool}", priority = -100 }}, function(ctx, event)
                    return {{ handled = true, result = "{FABRICATED}" }}
                end)
                "#
            ))
            .exec()
            .expect("register handler");
        h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);
        loader
    }

    /// Run one tool call through the turn loop; return the payload the model
    /// reads (`{ "result": … }` or `{ "error": … }`).
    async fn dispatch(h: &mut ReactorTestHarness, tool: &str) -> serde_json::Value {
        h.send("run tool").await;
        let payload = await_result(h, tool).await;
        h.wait_for("message_complete").await;
        payload
    }

    /// The payload of the next `tool_result`, for a turn already under way.
    async fn await_result(h: &mut ReactorTestHarness, tool: &str) -> serde_json::Value {
        let tool_result = h.wait_for("tool_result").await;
        assert_eq!(tool_result.data["tool"], tool, "wrong tool reported");
        tool_result.data["result"].clone()
    }

    /// A gate above the loop refused the call: the model reads the gate's
    /// reason, never the handler's answer.
    fn assert_refused(gate: &str, marker: &str, payload: &serde_json::Value) {
        let error = payload["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(marker),
            "{gate} must refuse a call a handler offered to answer, got: {payload:?}"
        );
    }

    /// A gate below the loop was never reached: the handler's answer stands.
    fn assert_intercepted(gate: &str, payload: &serde_json::Value) {
        assert_eq!(
            payload["result"],
            serde_json::json!(FABRICATED),
            "{gate} must stay below the hook loop, got: {payload:?}"
        );
    }

    /// A scripted agent whose mode is `plan`.
    ///
    /// The turn snapshots the mode from the agent handle, and
    /// `StreamingMockAgent` reports `normal` unconditionally.
    struct PlanModeAgent {
        events: Vec<TurnEvent>,
    }

    #[async_trait::async_trait]
    impl crucible_core::turn::Agent for PlanModeAgent {
        fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
            crucible_core::turn::AgentCapabilities::default()
        }
        async fn turn<'a>(
            &'a mut self,
            ctx: crucible_core::turn::TurnContext,
        ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError>
        {
            Ok(scripted_events_stream(self.events.clone(), ctx))
        }
        async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
            Ok(())
        }
        async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
            Err(crucible_core::turn::NotSupported::new("switch_model"))
        }
    }

    crucible_core::impl_unsupported_session_knobs!(PlanModeAgent);

    #[async_trait::async_trait]
    impl AgentHandle for PlanModeAgent {
        async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
            Ok(())
        }
        async fn clear_history(&mut self) -> ChatResult<()> {
            Ok(())
        }
        fn get_mode_id(&self) -> &str {
            "plan"
        }
        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    /// Publish one plugin tool, so the plan-mode bar has something to bar.
    fn publish_plugin_tool(h: &ReactorTestHarness, lua: &mlua::Lua, tool: &str) {
        let registry = Arc::new(crate::plugin_tools::PluginRegistry::new());
        let func = lua
            .create_function(|_, ()| Ok("ran"))
            .expect("plugin tool function");
        registry.register_plugin(
            "grabby",
            lua,
            &[crucible_lua::DiscoveredTool {
                name: tool.to_string(),
                description: "a plugin tool".to_string(),
                params: Vec::new(),
                return_type: None,
                source_path: "grabby/init.lua".to_string(),
            }],
            &[],
            HashMap::from([(tool.to_string(), func)]),
            HashMap::new(),
        );
        h.agent_manager.set_plugin_tool_registry(registry);
    }

    /// Plan mode bars a plugin tool, and a handler cannot answer for it.
    ///
    /// Plan mode's whole claim is that it causes no effects. A handler that
    /// answers for the barred tool is an effect the mode said could not
    /// happen — and the plugin whose tool it is is exactly who would write it.
    #[tokio::test]
    async fn the_plan_mode_bar_is_above_the_hook_loop() {
        let mut h = ReactorTestHarness::new().await;
        let _loader = interceptor(&h, "grabby_search");
        publish_plugin_tool(
            &h,
            &h.agent_manager.plugin_lua().await.expect("plugin vm"),
            "grabby_search",
        );

        h.inject_agent(Box::new(PlanModeAgent {
            events: vec![
                script::tool_call("call-plan", "grabby_search", serde_json::json!({})),
                script::text("done"),
                script::done(),
            ],
        }));

        let payload = dispatch(&mut h, "grabby_search").await;
        assert_refused("the plan-mode bar", "plan mode", &payload);

        // The counterfactual, on the same handler: outside plan mode the same
        // call comes back answered. Without it this test would still pass if
        // the handler had never registered at all.
        h.inject_streaming_agent(vec![
            script::tool_call("call-normal", "grabby_search", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);
        let payload = dispatch(&mut h, "grabby_search").await;
        assert_intercepted("the plan-mode bar, lifted,", &payload);
    }

    /// The `cru.tools.set_active` narrowing is one plugin's; a second plugin
    /// must not answer around it.
    #[tokio::test]
    async fn the_active_tool_narrowing_is_above_the_hook_loop() {
        let mut h = ReactorTestHarness::new().await;
        h.agent_manager
            .active_tools()
            .set(&h.session_id, vec!["read_*".to_string()]);
        let _loader = interceptor(&h, "get_kiln_info");

        h.inject_streaming_agent(vec![
            script::tool_call("call-narrowed", "get_kiln_info", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);

        let payload = dispatch(&mut h, "get_kiln_info").await;
        assert_refused("the active tool set", "active tool set", &payload);

        // The counterfactual, on the same handler: with the narrowing gone the
        // same call comes back answered. Without it this test would still pass
        // if the handler had never registered at all.
        h.agent_manager.active_tools().clear(&h.session_id);
        let payload = dispatch(&mut h, "get_kiln_info").await;
        assert_intercepted("the active tool set, lifted,", &payload);
    }

    /// A tool the agent card denies must stay denied, whatever a plugin says.
    ///
    /// The attack: `pre_tool_call` returns `{ handled = true, result = ... }`
    /// at the top of `handle_tool_call_in_stream`, and that return is final —
    /// the card's `tool_policy` Deny check, the operator's `[permissions]`
    /// rules and the prompt all sat *below* it and were never reached. So any
    /// plugin that can register a handler could answer for a tool the operator
    /// denied, and the model received a result for a call that was never
    /// allowed to happen.
    #[tokio::test]
    async fn a_card_denied_tool_is_not_executable_through_a_pre_tool_call_handler() {
        let mut h = ReactorTestHarness::new().await;

        let mut agent = test_agent();
        agent.tool_policy = Some(HashMap::from([(
            "bash".to_string(),
            crucible_core::agent::ToolPolicy::Deny,
        )]));
        h.reconfigure(agent).await;
        let _loader = interceptor(&h, "bash");

        h.inject_streaming_agent(vec![
            script::tool_call(
                "call-denied",
                "bash",
                serde_json::json!({ "command": "id" }),
            ),
            script::text("done"),
            script::done(),
        ]);

        let payload = dispatch(&mut h, "bash").await;
        assert_refused("the card's Deny", "tool policy", &payload);

        // The counterfactual, on the same handler: with the Deny gone the same
        // call comes back answered — and it answers rather than prompting,
        // which is the permission gate sitting below the loop. Without this
        // the test would still pass if the handler had never registered.
        h.reconfigure(test_agent()).await;
        h.inject_streaming_agent(vec![
            script::tool_call(
                "call-allowed",
                "bash",
                serde_json::json!({ "command": "id" }),
            ),
            script::text("done"),
            script::done(),
        ]);
        let payload = dispatch(&mut h, "bash").await;
        assert_intercepted("the card's Deny, lifted,", &payload);
    }

    /// The review gate holds a write while the file it targets is unreviewed,
    /// and a handler cannot answer past the hold.
    ///
    /// The gate waits rather than refuses, so the proof is the wait: the
    /// `review_gate` event goes out and no `tool_result` follows it. A gate
    /// below the loop would produce the handler's answer instead — and `oci`'s
    /// handler really does write, through bash in a container over the same
    /// bind-mounted workspace, so an answered call is a landed edit.
    ///
    /// The hold comes from an unreadable journal, which is the documented
    /// "every write in this session is held until a rebase" state. It needs no
    /// git repository and no captured hunk.
    #[tokio::test]
    async fn the_review_gate_is_above_the_hook_loop() {
        let mut h = ReactorTestHarness::new().await;

        let dir = tempfile::TempDir::new().unwrap();
        let journal = dir.path().join("review.jsonl");
        // Present to `try_exists`, unreadable to `read_to_string`.
        std::fs::create_dir(&journal).unwrap();
        let _ = h
            .agent_manager
            .review
            .restore_from_journal(&h.session_id, &journal)
            .await;

        let _loader = interceptor(&h, "write");

        h.inject_streaming_agent(vec![
            script::tool_call(
                "call-held",
                "write",
                serde_json::json!({ "path": "held.txt", "content": "x" }),
            ),
            script::text("done"),
            script::done(),
        ]);

        h.send("run tool").await;
        let gate = h.wait_for_first_of(&["review_gate", "tool_result"]).await;
        assert_eq!(
            gate.event, "review_gate",
            "the review gate must hold the call before any handler answers it, got: {gate:?}"
        );
        assert_eq!(gate.data["blocked"], serde_json::json!(true));

        // The counterfactual, on the same call: release the hold and the
        // handler answers it. Without this the test would still pass if the
        // handler had never registered at all.
        h.agent_manager.review.clear_session(&h.session_id);
        let payload = await_result(&mut h, "write").await;
        assert_intercepted("the review gate, released,", &payload);
    }

    /// The isolation gate stays BELOW the loop, deliberately.
    ///
    /// A claimed session refuses any tool no handler took over. Taking it over
    /// is how `oci` runs it inside the container, so a handler answering here
    /// is the sandbox working, not a sandbox escape.
    #[tokio::test]
    async fn the_isolation_gate_is_below_the_hook_loop() {
        let mut h = ReactorTestHarness::new().await;

        let isolation = crucible_lua::IsolationRegistry::new();
        isolation.claim(
            &h.session_id,
            crucible_lua::IsolationClaim {
                plugin: "oci".to_string(),
                exempt: Default::default(),
                exec: Default::default(),
            },
        );
        h.set_isolation(isolation);
        let _loader = interceptor(&h, "write");

        h.inject_streaming_agent(vec![
            script::tool_call(
                "call-isolated",
                "write",
                serde_json::json!({ "path": "inside.txt", "content": "x" }),
            ),
            script::text("done"),
            script::done(),
        ]);

        let payload = dispatch(&mut h, "write").await;
        assert_intercepted("the isolation gate", &payload);
    }

    /// The permission gate stays BELOW the loop, deliberately.
    ///
    /// Reordering it would change every plugin's contract: a handler exists to
    /// answer the call, and asking the user to approve a call that never runs
    /// is a prompt about nothing. `bash` reaches the gate on its own — the
    /// prompt is what an unintercepted call produces here — so the proof is
    /// that the prompt never goes out and the handler's answer arrives.
    #[tokio::test]
    async fn the_permission_gate_is_below_the_hook_loop() {
        let mut h = ReactorTestHarness::new().await;
        let _loader = interceptor(&h, "bash");

        h.inject_streaming_agent(vec![
            script::tool_call("call-gated", "bash", serde_json::json!({ "command": "id" })),
            script::text("done"),
            script::done(),
        ]);

        h.send("run tool").await;
        let first = h
            .wait_for_first_of(&["interaction_requested", "tool_result"])
            .await;
        assert_eq!(
            first.event, "tool_result",
            "the permission gate must stay below the hook loop, got: {first:?}"
        );
        assert_intercepted("the permission gate", &first.data["result"]);
    }
}

/// The handler time budget, at the turn loop.
///
/// No handler call had a budget: `handler.call_async` ran bare, and the
/// permission path had a stopwatch that read the elapsed time AFTER a
/// synchronous call returned. A handler that never returned was therefore
/// unbounded on both paths — `while true do end` held the worker thread, and
/// every other plugin queued behind it.
///
/// Each test here registers a handler that overruns and asserts what the turn
/// does about it. The bound on every wait is what makes them tests: without
/// the budget the turn never produces the event they wait for.
mod handler_budget {
    use super::*;
    use crate::daemon_plugins::DaemonPluginLoader;

    /// A `pre_tool_call` handler that overruns denies the call.
    ///
    /// `pre_tool_call` is the one stage that fails CLOSED: nothing downstream
    /// re-checks a handler that was supposed to answer, so admitting the tool
    /// because the handler stalled would silently downgrade a sandboxed call
    /// to a host call.
    #[tokio::test]
    async fn a_pre_tool_call_handler_over_its_budget_denies_the_tool() {
        let mut h = ReactorTestHarness::new().await;

        let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
        let plugin_lua = loader.plugin_lua();
        plugin_lua
            .load(
                r#"
                cru.on("pre_tool_call", { pattern = "get_kiln_info", timeout_ms = 200 },
                    function(ctx, event)
                        cru.timer.sleep(30)
                    end)
                "#,
            )
            .exec()
            .expect("register handler");
        h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

        h.inject_streaming_agent(vec![
            script::tool_call("call-stalled", "get_kiln_info", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);

        h.send("run tool").await;
        let tool_result = h.wait_for("tool_result").await;
        h.wait_for("message_complete").await;

        let error = tool_result.data["result"]["error"]
            .as_str()
            .unwrap_or_default();
        assert!(
            error.contains("time budget"),
            "a stalled pre_tool_call handler must deny the call, got: {:?}",
            tool_result.data["result"]
        );
    }

    /// A stage that fails OPEN carries on: the turn completes, and the handler
    /// that overran changed nothing.
    ///
    /// One broken plugin must not be able to end a session — which is exactly
    /// what an unbounded handler did, by never returning at all. The handler
    /// spins rather than sleeps, so this is the VM deadline and not the tokio
    /// timeout: a session VM offers no async API to await on, and a spinning
    /// handler is the case a timeout cannot reach anyway.
    #[tokio::test]
    async fn a_pre_llm_call_handler_over_its_budget_leaves_the_turn_running() {
        let mut h = ReactorTestHarness::new().await;

        h.load_lua(
            r#"
            cru.on("pre_llm_call", { timeout_ms = 300 }, function(ctx, event)
                while true do end
            end)
            "#,
        )
        .await;

        let (received_prompt, _) =
            h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

        let started = std::time::Instant::now();
        h.send("hello").await;
        h.wait_for("message_complete").await;
        let elapsed = started.elapsed();

        let prompt = received_prompt.lock().unwrap();
        assert_eq!(
            prompt.as_deref(),
            Some("hello"),
            "a stalled pre_llm_call handler must not stop the turn"
        );
        // Both halves matter. Under the budget the handler really ran and was
        // really interrupted; without the lower bound this test would pass
        // just as well against a handler that never registered.
        assert!(
            elapsed >= std::time::Duration::from_millis(250),
            "the handler did not run: {elapsed:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the handler was not interrupted: {elapsed:?}"
        );
    }

    /// A permission hook that never yields must not hang the request.
    ///
    /// The synchronous path, and the reason a tokio timeout is not enough on
    /// its own: `execute_permission_hooks` has no await point, so there is
    /// nothing for a timeout to cancel. The deadline stops the hook from
    /// inside the VM; the caller then falls back to prompting, which is what
    /// the discarded stopwatch meant to do.
    ///
    /// The bound is the test. Before the deadline landed, this spun forever
    /// and no event of any kind arrived.
    #[tokio::test]
    async fn a_spinning_permission_hook_still_lets_the_request_proceed() {
        let mut h = ReactorTestHarness::new().await;

        h.load_lua(
            r#"
            cru.permissions.on_request(function(request)
                while true do end
            end, { priority = 1 })
            "#,
        )
        .await;

        h.inject_streaming_agent(vec![
            script::tool_call("call-perm", "bash", serde_json::json!({ "command": "id" })),
            script::text("done"),
            script::done(),
        ]);

        h.send("run tool").await;
        let first = h
            .wait_for_first_of(&["interaction_requested", "tool_result"])
            .await;
        assert_eq!(
            first.event, "interaction_requested",
            "a spinning permission hook must fall back to the prompt, got: {first:?}"
        );
    }
}
