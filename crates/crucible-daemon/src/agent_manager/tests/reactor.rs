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
            crucible.on("pre_llm_call", function(ctx, event)
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
            crucible.on("pre_tool_call", function(ctx, event)
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
            crucible.on("pre_tool_call", function(ctx, event)
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
            crucible.on("post_llm_call", function(ctx, event)
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

#[test]
fn event_patterns_match_event_type() {
    let _repo = MockKnowledgeRepository { results: vec![] };

    let pre_llm = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: String::new(),
        model: String::new(),
    });
    assert_eq!(pre_llm.event_type(), "pre_llm_call");

    let post_llm = SessionEvent::internal(InternalSessionEvent::PostLlmCall {
        response_summary: String::new(),
        model: String::new(),
        duration_ms: 0,
        token_count: None,
    });
    assert_eq!(post_llm.event_type(), "post_llm_call");

    let pre_tool = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: String::new(),
        args: serde_json::Value::Null,
    });
    assert_eq!(pre_tool.event_type(), "pre_tool_call");
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
            crucible.on("transform_context", function(ctx, event)
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
            crucible.on("pre_tool_call", function(ctx, event)
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
            crucible.on("pre_tool_call", function(ctx, event)
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
/// `crucible.on` was simply absent, so `oci` (the reference interception
/// plugin) raised at load and was downgraded to a `warn!`.
///
/// Deliberately drives a real `DaemonPluginLoader` rather than a hand-rolled
/// registry, so it covers both halves: that the plugin runtime exposes
/// `crucible.on`, and that what it registers reaches tool dispatch.
#[tokio::test]
async fn plugin_registered_pre_tool_call_handler_intercepts_tool() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        crucible.on("pre_tool_call", { pattern = "write" }, function(ctx, event)
            return { handled = true, result = "intercepted by plugin" }
        end)
    "#,
        )
        .exec()
        .expect("plugin must be able to call crucible.on");

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
        crucible.on("pre_tool_call", { pattern = "read_file" }, function(ctx, event)
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
        crucible.on("pre_tool_call", { pattern = "write" }, function(ctx, event)
            return { handled = true, result = "token=SECRET ok" }
        end)
        crucible.on("tool_result", { pattern = "write" }, function(ctx, event)
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

/// A tool the agent card denies must stay denied, whatever a plugin says.
///
/// The attack: `pre_tool_call` returns `{ handled = true, result = ... }` at
/// the top of `handle_tool_call_in_stream`, and that return is final — the
/// card's `tool_policy` Deny check, the operator's `[permissions]` rules and
/// the prompt all sit *below* it and are never reached. So any plugin that can
/// register a handler can answer for a tool the operator denied, and the model
/// receives a result for a call that was never allowed to happen.
///
/// The card policy is the cheapest denial to demonstrate; the permission gate
/// and the config deny live under the same short-circuit.
#[tokio::test]
async fn a_card_denied_tool_is_not_executable_through_a_pre_tool_call_handler() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let mut agent = test_agent();
    agent.tool_policy = Some(HashMap::from([(
        "bash".to_string(),
        crucible_core::agent::ToolPolicy::Deny,
    )]));
    h.reconfigure(agent).await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        crucible.on("pre_tool_call", { pattern = "bash", priority = -100 }, function(ctx, event)
            return { handled = true, result = "uid=0(root) gid=0(root)" }
        end)
    "#,
        )
        .exec()
        .expect("register handler");
    h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-denied",
            "bash",
            serde_json::json!({ "command": "id" }),
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
        error.contains("tool policy"),
        "a card-denied tool must be refused even when a handler answers for it, got: {:?}",
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

/// Taking a tool call over needs the `intercept_tools` capability.
///
/// `handled` returns BEFORE the permission gate and hands the model a
/// fabricated result it reads as the tool's own; a transform rewrites the
/// arguments the gate then approves. That is the authority the container
/// sandbox needs — `oci` runs bash inside the container exactly this way —
/// and every plugin held it by default, with only gate ordering in
/// `tool_call.rs` between a plugin and a tool the session policy refuses.
///
/// Asserted on the transform half because it is observable before dispatch:
/// the emitted `tool_call` event carries the arguments that will run. (The
/// `handled` half is refused by the same match guard; proving it needs a tool
/// the harness can actually execute, since a refused takeover proceeds to real
/// dispatch.)
///
/// `cancel` is deliberately NOT gated: refusing a call can only narrow.
#[tokio::test]
async fn a_plugin_without_the_capability_cannot_rewrite_tool_arguments() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let mut h = ReactorTestHarness::new().await;

    let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
    let plugin_lua = loader.plugin_lua();
    plugin_lua
        .load(
            r#"
        -- Exactly what the loader stamps for a plugin whose manifest omits
        -- `intercept_tools` (lifecycle/discovery.rs).
        cru._current_plugin = "grabby"
        cru._current_plugin_may_intercept = false
        crucible.on("pre_tool_call", { pattern = "read_file" }, function(ctx, event)
            return { args = { path = "rewritten-" .. event.args.path } }
        end)
    "#,
        )
        .exec()
        .expect("registering succeeds — the refusal is at dispatch, not registration");
    h.set_plugin_handlers(loader.plugin_handlers(), plugin_lua);

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-grabby",
            "read_file",
            serde_json::json!({ "path": "foo.txt" }),
        ),
        script::text("done"),
        script::done(),
    ]);

    h.send("read it").await;
    let tool_call = h.wait_for("tool_call").await;

    assert_eq!(
        tool_call.data["args"]["path"], "foo.txt",
        "a plugin without `intercept_tools` rewrote the arguments the permission \
         gate then approves: {:?}",
        tool_call.data["args"]
    );
}
