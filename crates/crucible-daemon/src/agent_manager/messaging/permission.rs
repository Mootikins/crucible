use super::super::*;
use crucible_core::config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_lua::StageId;
use std::future::Future;
use std::ops::ControlFlow;

use crate::agent_manager::vm_pass::run_handlers;

/// The MCP prefixes an agent puts in front of a Crucible tool's name.
///
/// The prefix comes from the server name Crucible announces in
/// `session/new`, and each agent has its own form for it.
const CRUCIBLE_MCP_PREFIXES: &[&str] = &[
    // claude-agent-acp, and every MCP client that follows the Claude Code
    // naming: `mcp__<server>__<tool>`.
    "mcp__crucible__",
    // codex-acp: `mcp.<server>.<tool>`.
    "mcp.crucible.",
];

/// The tool name the policy keys an ACP tool call on.
///
/// **The agent sends the name.** Schema 1.9.1 makes `ToolCall.name` a stable
/// field, and an ACP agent puts the programmatic tool name there: `Bash`,
/// `Edit`, `mcp__crucible__read_note`. ONE key answers for every tool an
/// agent asks about — its own, Crucible's, another MCP server's — so the
/// operator writes a rule against the name they see, and the daemon needs no
/// translation table.
///
/// A Crucible tool loses its MCP prefix here, so the card and the
/// `[permissions]` rules use one key, `read_note`, whether an ACP agent or
/// the daemon's own tool path runs it. Another server's tool keeps its whole
/// name: Crucible does not own that name, and stripping the prefix would let
/// `mcp__evil__read_note` take a rule written about Crucible's `read_note`.
///
/// The name IS grounds for the read-only exemption, unlike the `kind` below:
/// keyed by its internal name, Crucible's `read_note` is the same tool the
/// daemon's own agents run, and it skips the prompt for the same reason. An
/// agent that mislabels a call buys nothing by it — it runs its own tools in
/// its own process, so it would simply not ask.
///
/// The fallbacks, in order:
///
/// 1. the `name` the agent sent, less a Crucible MCP prefix;
/// 2. failing that, `kind`.
///
/// The step between the two is not here: an agent that asks about an MCP
/// tool without naming it (codex-acp does) is answered from the `tool_call`
/// frame that announced the same `toolCallId`, and the ACP client fills
/// `name` in from what it saw before this runs. See
/// `acp::client::announced_tool_name`.
///
/// `title` is never read as a name. It is prose for a person — `"Read
/// src/main.rs"` — and keying on it made every `[permissions]` rule inert.
fn acp_permission_tool_name(
    fields: &agent_client_protocol::schema::v1::ToolCallUpdateFields,
) -> String {
    let Some(name) = fields.name.as_deref() else {
        return acp_tool_name(fields);
    };
    CRUCIBLE_MCP_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .unwrap_or(name)
        .to_string()
}

/// The coarse name of a tool call the agent did not name.
///
/// `kind` is then the only tool identity on the wire, so rules match against
/// it. The names chosen are the engine's own file-operation grammar (`read`,
/// `edit`, `write`, `delete` — see `is_file_tool`) plus `bash`, so an
/// operator writes the same patterns they already write elsewhere.
///
/// A `kind` the agent supplies is **not** grounds for the read-only
/// exemption: see [`crate::agent_manager::is_safe`], which may only widen on
/// something the daemon itself knows. An ACP tool matches rules and is
/// otherwise asked about.
fn acp_tool_name(fields: &agent_client_protocol::schema::v1::ToolCallUpdateFields) -> String {
    use agent_client_protocol::schema::v1::ToolKind;
    match fields.kind {
        Some(ToolKind::Read) => "read",
        Some(ToolKind::Edit) => "edit",
        Some(ToolKind::Delete) => "delete",
        // No `move` in the engine's grammar, and a move is a write.
        Some(ToolKind::Move) => "write",
        Some(ToolKind::Search) => "search",
        Some(ToolKind::Execute) => "bash",
        Some(ToolKind::Fetch) => "fetch",
        Some(ToolKind::Think) => "think",
        Some(ToolKind::SwitchMode) => "switch_mode",
        // `Other` is ACP's default, so an agent that sets no kind lands here
        // too — as does any variant a later schema adds, since `ToolKind` is
        // `#[non_exhaustive]` and a kind we cannot classify must not be
        // guessed at. Deliberately not a file-operation name: an unidentified
        // call must not inherit a rule written for one that is identified.
        Some(ToolKind::Other) | Some(_) | None => "acp_tool",
    }
    .to_string()
}

/// Serializer that ensures only one permission prompt is in-flight at a time
/// per ACP session.
///
/// **Why:** ACP clients (Claude Code, etc.) invoke our permission handler
/// concurrently for parallel tool batches. Without serialization, all N
/// `interaction_requested` events emit at once and pile up in the TUI's
/// queue — the user perceives them as "batched" instead of as "each
/// permission prompt arriving as the corresponding tool finishes".
///
/// Holding the lock across the entire prompt+await window means that
/// caller N+1 waits for caller N's response before its own
/// `interaction_requested` event is emitted, so the TUI sees prompts
/// one-at-a-time even though the ACP client called us in parallel.
///
/// **UX consequence:** if the user walks away from a prompt and it hits
/// the 300 s timeout, queued callers stay blocked for the full timeout
/// before they get a chance to fire. That's the deliberate tradeoff —
/// silent batching was worse — but worth knowing if you're debugging
/// "why did my second prompt take 5 minutes to appear?".
#[derive(Clone, Default)]
pub(super) struct PermissionSerializer {
    inner: Arc<tokio::sync::Mutex<()>>,
}

impl PermissionSerializer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `fut` while holding the serializer lock. Subsequent calls on the
    /// same serializer queue behind this one.
    pub async fn run<F, R>(&self, fut: F) -> R
    where
        F: Future<Output = R>,
    {
        let _guard = self.inner.lock().await;
        fut.await
    }
}

/// The tools whose permission patterns match on a path, not on the tool name.
const FILE_TOOLS: &[&str] = &[
    "write_file",
    "edit_file",
    "create_note",
    "update_note",
    "delete_note",
];

/// Which of a [`PatternStore`]'s three rule tables owns a tool call.
///
/// A closed set: the store has these three tables and no fourth, so the
/// matches below are exhaustive and there is no `Default` — a name that fits
/// nowhere must be decided here, not fall into a table by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternKind {
    /// A bash rule, matched against the shell command line carried here.
    Bash(String),
    /// A file rule, matched against a path taken from the arguments.
    File,
    /// A tool rule, matched against the tool name.
    Tool,
}

/// Classify a tool call for the pattern store.
///
/// **One classifier, read by the writer and the reader.** Storing a grant and
/// checking it are two halves of one "always allow" click; if they disagree
/// about what a call is, the click writes a rule that can never match and the
/// user is prompted again for ever, with no way to see why.
///
/// They did disagree. [`PermRequest::suggested_pattern`] asks
/// [`CanonicalToolCall`], which calls `shell`, `Bash` and `myserver__bash` shell
/// tools, so it offered a command line; the routing here compared the name to
/// the literal `"bash"`, so every other command tool filed that command line
/// as a tool-name rule. `CanonicalToolCall` is the single source of truth for
/// what a command is, and both halves now read it. Never restate its list here —
/// a second list is exactly what drifted.
///
/// The projection reads the arguments, not the name alone, which is what
/// keeps the two halves aligned in the awkward case too: a `shell` call with
/// no `command` argument is not a command to `suggested_pattern` either, so
/// both file it as a tool rule.
fn pattern_kind(tool_name: &str, args: &serde_json::Value) -> PatternKind {
    use crucible_core::types::CanonicalToolCall;

    // Not a command: a path tool matches on its path, anything else on its
    // name. Both are decided by the name alone.
    let by_name = || {
        if FILE_TOOLS.contains(&tool_name) {
            PatternKind::File
        } else {
            PatternKind::Tool
        }
    };

    CanonicalToolCall::crucible_tool(tool_name, args)
        .command
        .map_or_else(by_name, PatternKind::Bash)
}

/// The text the permission engine matches its rules against: the shell
/// command for `bash`, the full JSON arguments for every other tool.
pub(super) fn engine_input(tool_name: &str, args: &serde_json::Value) -> String {
    if tool_name == "bash" {
        args.get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    } else {
        args.to_string()
    }
}

/// Which ACP option a gate decision corresponds to.
///
/// `AllowAlways` only when the decision is one the user asked to be
/// remembered — a saved pattern, or a scope wider than this single call.
/// Anything else that was allowed is allowed once.
fn outcome_kind(
    response: &PermResponse,
) -> agent_client_protocol::schema::v1::PermissionOptionKind {
    use agent_client_protocol::schema::v1::PermissionOptionKind;

    if !response.allowed {
        return PermissionOptionKind::RejectOnce;
    }
    let remembered = response.pattern.is_some()
        || matches!(
            response.scope,
            PermissionScope::Project | PermissionScope::User | PermissionScope::Session
        );
    if remembered {
        PermissionOptionKind::AllowAlways
    } else {
        PermissionOptionKind::AllowOnce
    }
}

/// The agent option that carries the gate decision `desired`.
///
/// An agent does not have to offer all four kinds. When the exact kind is
/// absent, a narrower kind of the same decision stands in: `allow_always`
/// falls back to `allow_once`, and one reject kind to the other. A one-time
/// allow never takes `allow_always`, because that grant is wider than the
/// user chose. `Cancelled` remains only for no usable option, because the
/// agent then stops the whole turn.
fn select_option(
    options: &[agent_client_protocol::schema::v1::PermissionOption],
    desired: agent_client_protocol::schema::v1::PermissionOptionKind,
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    use agent_client_protocol::schema::v1::{
        PermissionOptionKind as Kind, RequestPermissionOutcome, SelectedPermissionOutcome,
    };

    let acceptable: &[Kind] = match desired {
        Kind::AllowAlways => &[Kind::AllowAlways, Kind::AllowOnce],
        Kind::AllowOnce => &[Kind::AllowOnce],
        Kind::RejectOnce => &[Kind::RejectOnce, Kind::RejectAlways],
        Kind::RejectAlways => &[Kind::RejectAlways, Kind::RejectOnce],
        // `outcome_kind` produces only the four kinds above.
        _ => &[],
    };

    acceptable
        .iter()
        .find_map(|kind| options.iter().find(|opt| opt.kind == *kind))
        .map(|opt| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                opt.option_id.clone(),
            ))
        })
        .unwrap_or(RequestPermissionOutcome::Cancelled)
}

/// Answer one `session/request_permission`.
///
/// A free function, not a closure body: this IS the ACP half of the tool
/// policy, and a test drives it with the frames the real agents send.
///
/// The card policy goes INTO the gate rather than answering ahead of it. The
/// two halves used to disagree about the same session's `tool_policy` — an
/// ACP agent's card `allow` walked past an operator `deny` that the daemon's
/// own agents obeyed.
///
/// A call the agent never asks about is the agent's own decision. It runs its
/// own tools in its own process, so a refusal that arrives after the fact
/// stops nothing; Crucible answers what it is asked, and nothing more.
async fn decide_acp_permission(
    gate: &DaemonPermissionGate,
    tool_policy: Option<&crucible_core::agent::ToolPolicyMap>,
    request: agent_client_protocol::schema::v1::RequestPermissionRequest,
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    let tool_name = acp_permission_tool_name(&request.tool_call.fields);
    let card_policy = tool_policy.and_then(|map| map.get(&tool_name)).copied();

    let args = request
        .tool_call
        .fields
        .raw_input
        .clone()
        .unwrap_or(serde_json::Value::Null);
    let diffs = crate::tools::diff_synth::synthesize_diffs(&tool_name, &args);
    let permission = PermRequest::tool(tool_name, args).with_diffs(diffs);

    let response = gate.request_permission(permission, card_policy).await;
    select_option(&request.options, outcome_kind(&response))
}

impl AgentManager {
    pub(super) fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
        agent_permissions: Option<PermissionConfig>,
        tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    ) -> crate::acp::client::PermissionRequestHandler {
        let slot = self.slot(session_id);
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();
        let serializer = PermissionSerializer::new();

        let ask_callback: PermissionPromptCallback = Arc::new(move |perm_request: PermRequest| {
            let slot = slot.clone();
            let session_id_owned = session_id_owned.clone();
            let event_tx_owned = event_tx_owned.clone();
            let serializer = serializer.clone();

            Box::pin(async move {
                serializer
                    .run(async move {
                        let (permission_id, response_rx) =
                            slot.register_permission(perm_request.clone());

                        let interaction_request = InteractionRequest::Permission(perm_request);
                        if !emit_event(
                            &event_tx_owned,
                            SessionEventMessage::interaction_requested(
                                &session_id_owned,
                                &permission_id,
                                &interaction_request,
                            ),
                        ) {
                            tracing::debug!(
                                "Failed to emit interaction_requested event (no subscribers)"
                            );
                        }

                        let result =
                            tokio::time::timeout(std::time::Duration::from_secs(300), response_rx)
                                .await;

                        match result {
                            Ok(Ok(response)) => response,
                            Ok(Err(_)) => {
                                slot.take_permission(&permission_id);
                                tracing::debug!(
                                    permission_id = %permission_id,
                                    session_id = %session_id_owned,
                                    "permission channel closed before response"
                                );
                                PermResponse::deny_with_reason(
                                    "Permission request channel closed before response".to_string(),
                                )
                            }
                            Err(_) => {
                                slot.take_permission(&permission_id);
                                tracing::debug!(
                                    permission_id = %permission_id,
                                    session_id = %session_id_owned,
                                    "permission request timed out"
                                );
                                PermResponse::deny_with_reason(
                                    "Permission request timed out".to_string(),
                                )
                            }
                        }
                    })
                    .await
            })
        });

        // Priority: CLI override > agent-specific > global config.
        // For Allow and Deny overrides, the user's intent is unconditional —
        // ignore base-config rules entirely. For Ask, preserve rules (interactive default).
        let effective_config = resolve_effective_permission_config(
            permission_override,
            agent_permissions,
            self.permission_config.clone(),
        );

        let gate = Arc::new(
            DaemonPermissionGate::new(effective_config, is_interactive)
                .with_prompt_callback(ask_callback),
        );

        let tool_policy = tool_policy.map(Arc::new);

        Arc::new(
            move |request: agent_client_protocol::schema::v1::RequestPermissionRequest| {
                let gate = gate.clone();
                let tool_policy = tool_policy.clone();

                Box::pin(async move {
                    decide_acp_permission(&gate, tool_policy.as_deref(), request).await
                })
            },
        )
    }

    /// Run one registry's `pre_llm_call` handlers over the prompt, chained.
    ///
    /// Returns the transformed prompt and whether a handler cancelled — which
    /// cancels the TURN: the caller returns `None`,
    /// same as the reactor path and `transform_context`. (The old loop
    /// `break`-ed on Cancel and sent the prompt anyway — a cancel that
    /// didn't cancel.) Extracted so every caller shares
    /// one loop body: plugins registering this event used to get documented
    /// silence, because only the per-session registry was ever dispatched.
    async fn run_pre_llm_call_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current_content: String,
    ) -> (String, bool) {
        for handler in registry.runtime_handlers_for(
            StageId::PreLlmCall.as_str(),
            None,
            crucible_lua::Firing::InSession(&stream_ctx.session_id),
        ) {
            let event = SessionEvent::Custom {
                name: "pre_llm_call".to_string(),
                payload: serde_json::json!({
                    "prompt": &current_content,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
                .await
            {
                Ok(crucible_lua::ScriptHandlerResult::Transform(val)) => {
                    if let Some(prompt) = val.get("prompt").and_then(|v| v.as_str()) {
                        current_content = prompt.to_string();
                    }
                }
                Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        reason = %reason,
                        "pre_llm_call handler cancelled"
                    );
                    return (current_content, true);
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        error = %error,
                        "pre_llm_call handler error (fail-open)"
                    );
                }
            }
        }
        (current_content, false)
    }

    pub(super) async fn apply_pre_llm_call_handlers(
        content: String,
        stream_ctx: &StreamContext,
        stream_config: &AgentStreamConfig,
    ) -> Option<String> {
        // Session-scoped handlers first (more specific), under the state
        // lock; then plugin handlers with the lock RELEASED — plugin Lua can
        // call `cru.shell`/`cru.http` for seconds, and holding the session's
        // whole state across that starves everything else on the session.
        let (current_content, cancelled) = run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            (content, false),
            |registry, lua, (content, _)| {
                Box::pin(async move {
                    let (content, cancelled) = Self::run_pre_llm_call_handlers(
                        stream_ctx,
                        &registry,
                        &lua,
                        &stream_config.model,
                        content,
                    )
                    .await;
                    if cancelled {
                        ControlFlow::Break((content, true))
                    } else {
                        ControlFlow::Continue((content, false))
                    }
                })
            },
        )
        .await;

        // A cru.on Cancel cancels the TURN — same as the reactor path
        // above and transform_context. It used to merely stop the handler
        // chain and send the prompt anyway.
        if cancelled {
            return None;
        }

        Some(current_content)
    }

    /// Run one registry's `transform_context` handlers over the messages,
    /// chained. `Err(())` means a handler cancelled the turn.
    async fn run_transform_context_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current: Vec<crucible_core::traits::ContextMessage>,
    ) -> Result<Vec<crucible_core::traits::ContextMessage>, ()> {
        for handler in registry.runtime_handlers_for(
            StageId::TransformContext.as_str(),
            None,
            crucible_lua::Firing::InSession(&stream_ctx.session_id),
        ) {
            let event = SessionEvent::Custom {
                name: "transform_context".to_string(),
                payload: serde_json::json!({
                    "messages": &current,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
                .await
            {
                Ok(crucible_lua::ScriptHandlerResult::Transform(val)) => {
                    if let Some(msgs_val) = val.get("messages") {
                        match serde_json::from_value::<Vec<crucible_core::traits::ContextMessage>>(
                            msgs_val.clone(),
                        ) {
                            Ok(new_messages) => current = new_messages,
                            Err(e) => warn!(
                                session_id = %stream_ctx.session_id,
                                handler = handler.id,
                                error = %e,
                                "transform_context handler returned invalid messages, keeping previous"
                            ),
                        }
                    }
                }
                Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        reason = %reason,
                        "transform_context handler cancelled"
                    );
                    return Err(());
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        handler = handler.id,
                        error = %error,
                        "transform_context handler error (fail-open)"
                    );
                }
            }
        }
        Ok(current)
    }

    /// Fire `transform_context` for handlers that want to mutate the
    /// `Vec<ContextMessage>` going to the provider. This is the rich-
    /// message-array seam (Pi's `transformContext`); the existing
    /// `pre_llm_call` is the later string-level seam. Both fire per turn.
    ///
    /// Returns the (possibly mutated) message vec. `None` means a
    /// handler cancelled and the caller should abort the turn.
    pub(super) async fn apply_transform_context_handlers(
        messages: Vec<crucible_core::traits::ContextMessage>,
        stream_ctx: &StreamContext,
        stream_config: &AgentStreamConfig,
    ) -> Option<Vec<crucible_core::traits::ContextMessage>> {
        let mut current = messages;

        // Built-in producer: prepend the pre-computed Precognition
        // system block, if any. Runs *before* Lua handlers so plugins
        // can observe and mutate it via the same `transform_context`
        // seam they'd use to inject their own context.
        //
        // We protect against accidental drop: a buggy handler that
        // returns `{messages = ...}` without including the precog block
        // would otherwise silently strip kiln context. Below, after
        // Lua handlers run, we check whether the block is still
        // present and re-prepend if not.
        // `@file` attachments go in first so the Precognition block lands
        // above them: kiln context frames the conversation, an attached file
        // is about this message.
        if let Some(ref attachment) = stream_ctx.attachment_message {
            let mut with_attachment = Vec::with_capacity(current.len() + 1);
            with_attachment.push(attachment.clone());
            with_attachment.extend(current);
            current = with_attachment;
        }

        if let Some(ref precog_msg) = stream_ctx.precognition_message {
            let mut with_precog = Vec::with_capacity(current.len() + 1);
            with_precog.push(precog_msg.clone());
            with_precog.extend(current);
            current = with_precog;
        }

        // Lua runtime handlers can replace the message array entirely by
        // returning `{ messages = ... }`. Session-scoped handlers first,
        // under the state lock; then plugin handlers with the lock released
        // (same rationale as `apply_pre_llm_call_handlers`).
        current = run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            current,
            |registry, lua, current| {
                Box::pin(async move {
                    match Self::run_transform_context_handlers(
                        stream_ctx,
                        &registry,
                        &lua,
                        &stream_config.model,
                        current,
                    )
                    .await
                    {
                        Ok(messages) => ControlFlow::Continue(messages),
                        Err(()) => ControlFlow::Break(None),
                    }
                })
            },
        )
        .await?;

        // Defensive: if a Lua handler returned a new `messages` array
        // that dropped the built-in Precognition block, re-prepend it.
        //
        // Identity is by `metadata.tags` containing PRECOGNITION_TAG,
        // not by content. This lets a Lua handler legitimately mutate
        // the precog content (translate, redact, summarize) as long as
        // it preserves the tag — only handlers that fully rebuild the
        // array from scratch (losing metadata) or remove the message
        // entirely trigger the re-prepend.
        //
        // A plugin that explicitly wants to suppress Precognition
        // should configure that at the agent layer
        // (`agent_config.precognition_enabled = false`), not via the
        // messages array — silent stripping is what we're guarding
        // against, not legitimate config.
        if let Some(ref precog_msg) = stream_ctx.precognition_message {
            let still_present = current.iter().any(|m| {
                m.metadata
                    .tags
                    .iter()
                    .any(|t| t == crate::agent_manager::precognition::PRECOGNITION_TAG)
            });
            if !still_present {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "transform_context handler dropped Precognition message; re-prepending. \
                     To suppress legitimately, set precognition_enabled=false in agent config."
                );
                let mut with_precog = Vec::with_capacity(current.len() + 1);
                with_precog.push(precog_msg.clone());
                with_precog.extend(current);
                current = with_precog;
            }
        }

        Some(current)
    }

    /// Evaluate a mode's own rule lists with the shared permission engine.
    ///
    /// Built per call rather than cached: a mode is redefinable at any time,
    /// and the rule lists are a handful of strings. If that ever shows up in a
    /// profile, cache on the mode's identity, not on the session.
    pub(in crate::agent_manager) fn evaluate_mode_rules(
        permissions: &crucible_lua::ModePermissions,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> PermissionDecision {
        use crucible_core::config::components::permissions::PermissionConfig;
        let config = PermissionConfig {
            default: match permissions.default {
                crucible_lua::ModeStance::Allow => PermissionMode::Allow,
                crucible_lua::ModeStance::Deny => PermissionMode::Deny,
                crucible_lua::ModeStance::Ask => PermissionMode::Ask,
            },
            allow: permissions.allow.clone(),
            deny: permissions.deny.clone(),
            ask: permissions.ask.clone(),
        };
        let engine = PermissionEngine::new(Some(&config));
        let input = engine_input(tool_name, args);
        // `is_interactive: true` deliberately: the non-interactive ask→deny
        // conversion is the caller's job below, and doing it here would skip
        // the prompt path entirely.
        engine.evaluate(tool_name, &input, true)
    }

    /// Run the permission gate.
    ///
    /// `Ok(Some(reason))` means the call was approved WITHOUT asking, and by
    /// which layer; `Ok(None)` means the user was asked and said yes. The
    /// caller carries the reason on the `tool_call` event — the decision is
    /// made before that event is emitted, so an auto-approval marker can ride
    /// along with the card rather than arriving after it and popping in.
    pub(super) async fn handle_permission_request(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call_id: &str,
        args: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        // Honor explicit --permissions override before running any hooks or prompt.
        // `Allow` auto-approves; `Deny` auto-rejects with an error tool_result;
        // `Ask` and `None` fall through to the standard hook/prompt flow.
        match stream_ctx.permission_override {
            Some(PermissionMode::Allow) => {
                tracing::debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "permission override Allow: auto-approving tool call"
                );
                return Ok(Some("permission override".to_string()));
            }
            Some(PermissionMode::Deny) => {
                let error_msg = "Tool call denied by permission override".to_string();
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for tool_result (permission override Deny)"
                    );
                }
                return Err(error_msg);
            }
            Some(PermissionMode::Ask) | None => {}
        }

        // Global `[permissions]` config — previously enforced for ACP agents
        // only. Config deny is absolute; config allow (incl. `default =
        // "allow"`) short-circuits the gate. Ask (or no matching rule) falls
        // through to PatternStore → Lua hooks → prompt. The engine is asked
        // with `is_interactive: true` deliberately: its own ask→deny
        // conversion would skip the fall-through layers, and the prompt
        // branch below already handles non-interactive turns.
        if let Some(engine) = &stream_ctx.permission_engine {
            use crucible_core::config::components::permissions::PermissionDecision;
            let rule_input = engine_input(&tool_call.name, args);
            match engine.evaluate(&tool_call.name, &rule_input, true) {
                PermissionDecision::Allow => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "Permissions config allows tool, skipping prompt"
                    );
                    return Ok(Some("permissions config".to_string()));
                }
                PermissionDecision::Deny { reason } => {
                    let error_msg = format!(
                        "Tool '{}' denied by permissions config: {reason}",
                        tool_call.name
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            call_id,
                            &tool_call.name,
                            serde_json::json!({ "error": &error_msg }),
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            "No subscribers for config-denied tool_result event"
                        );
                    }
                    return Err(error_msg);
                }
                PermissionDecision::Ask { .. } => {}
            }
        }

        let project_path = stream_ctx.workspace_path.to_string_lossy();
        // A grant at either persisted scope skips the prompt. Both stores
        // live under the injected directory, so a test never reads a real one.
        let pattern_store = match stream_ctx.whitelists_dir.as_deref() {
            Some(dir) => PatternStore::load_sync_in(dir, &project_path)
                .unwrap_or_default()
                .merge(&PatternStore::load_user_sync_in(dir).unwrap_or_default()),
            None => PatternStore::default(),
        };
        let pattern_matched = Self::check_pattern_match(&tool_call.name, args, &pattern_store);

        if pattern_matched {
            debug!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "Tool call matches whitelisted pattern, skipping permission prompt"
            );
            return Ok(Some("saved pattern".to_string()));
        }

        // The mode's declared stance. Consulted AFTER the hooks below, because
        // a stance is static and a hook is a decision — `cru.modes.auto` says
        // "allow by default", and a user hook that denies bash must still win.
        let mode_permissions = stream_ctx
            .agent_stream_config
            .modes
            .get(&stream_ctx.session_mode)
            .map(|m| m.permissions);

        let hook_result = Self::run_permission_hooks(
            stream_ctx.agent_stream_config.daemon_permissions.as_ref(),
            &tool_call.name,
            args,
            &stream_ctx.session_id,
            &stream_ctx.session_mode,
            &stream_ctx.agent_stream_config.mcp_read_only_tools,
        );

        match hook_result {
            PermissionHookResult::Allow => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook allowed tool, skipping permission prompt"
                );
                Ok(Some(if stream_ctx.session_mode == "auto" {
                    "auto mode".to_string()
                } else {
                    "Lua permission hook".to_string()
                }))
            }
            PermissionHookResult::Deny => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook denied tool"
                );
                let resource_desc = Self::brief_resource_description(&tool_call.name, args);
                let error_msg = format!(
                    "Lua hook denied permission to {} {}",
                    tool_call.name, resource_desc
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for hook denied tool_result event"
                    );
                }
                Err(error_msg)
            }
            PermissionHookResult::Prompt => {
                // No hook had an opinion: fall back to the mode's own rules,
                // then its default stance.
                //
                // The rules use the `[permissions]` grammar and the SAME
                // engine, so `bash:rg *` inherits its chained-command handling
                // — a mode that permits `rg` does not thereby permit
                // `rg foo && rm -rf /`. Writing a second matcher here would
                // have been the easy way to lose that.
                let mode_stance = match &mode_permissions {
                    Some(p) if p.has_rules() => {
                        match Self::evaluate_mode_rules(p, &tool_call.name, args) {
                            PermissionDecision::Allow => Some(crucible_lua::ModeStance::Allow),
                            PermissionDecision::Deny { .. } => Some(crucible_lua::ModeStance::Deny),
                            PermissionDecision::Ask { .. } => Some(crucible_lua::ModeStance::Ask),
                        }
                    }
                    Some(p) => Some(p.default),
                    None => None,
                };

                match mode_stance {
                    Some(crucible_lua::ModeStance::Allow) => {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            mode = %stream_ctx.session_mode,
                            "mode stance allows tool, skipping permission prompt"
                        );
                        return Ok(Some(format!("{} mode", stream_ctx.session_mode)));
                    }
                    Some(crucible_lua::ModeStance::Deny) => {
                        let error_msg = format!(
                            "Tool '{}' is not permitted in {} mode",
                            tool_call.name, stream_ctx.session_mode
                        );
                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_result(
                                &stream_ctx.session_id,
                                call_id,
                                &tool_call.name,
                                serde_json::json!({ "error": &error_msg }),
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %tool_call.name,
                                "No subscribers for mode-denied tool_result event"
                            );
                        }
                        return Err(error_msg);
                    }
                    Some(crucible_lua::ModeStance::Ask) | None => {}
                }

                // Non-interactive turns (delegated child sessions, headless
                // sends) have nobody to answer a prompt — deny immediately
                // with an actionable message instead of hanging.
                if !stream_ctx.is_interactive {
                    let error_msg = format!(
                        "Permission required for '{}' but this session runs non-interactively. \
                         Allow it via a permission pattern, Lua permission hook, or permissions \
                         config.",
                        tool_call.name
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            call_id,
                            &tool_call.name,
                            serde_json::json!({ "error": &error_msg }),
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            "No subscribers for non-interactive deny tool_result event"
                        );
                    }
                    return Err(error_msg);
                }

                let diffs = crate::tools::diff_synth::synthesize_diffs(&tool_call.name, args);
                let perm_request =
                    PermRequest::tool(&tool_call.name, args.clone()).with_diffs(diffs);
                let interaction_request = InteractionRequest::Permission(perm_request.clone());
                let (permission_id, response_rx) =
                    stream_ctx.slot.register_permission(perm_request);

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    permission_id = %permission_id,
                    "Emitting permission request for destructive tool"
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::interaction_requested(
                        &stream_ctx.session_id,
                        &permission_id,
                        &interaction_request,
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for permission request event"
                    );
                }

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    permission_id = %permission_id,
                    "Waiting for permission response"
                );

                // Bounded wait (parity with the ACP gate's 300 s): an
                // unanswered prompt must not wedge the turn forever.
                let response_result =
                    tokio::time::timeout(std::time::Duration::from_secs(300), response_rx).await;
                let (permission_granted, deny_reason) = match response_result {
                    Err(_elapsed) => {
                        stream_ctx.slot.take_permission(&permission_id);
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            permission_id = %permission_id,
                            "Permission prompt timed out, treating as deny"
                        );
                        (false, Some("permission prompt timed out".to_string()))
                    }
                    Ok(response_rx_result) => match response_rx_result {
                        Ok(response) => {
                            debug!(
                                session_id = %stream_ctx.session_id,
                                tool = %tool_call.name,
                                permission_id = %permission_id,
                                allowed = response.allowed,
                                pattern = ?response.pattern,
                                "Permission response received"
                            );

                            if response.allowed {
                                if let Some(ref pattern) = response.pattern {
                                    if let Some(file) =
                                        stream_ctx.whitelists_dir.as_deref().and_then(|dir| {
                                            PatternStore::store_file_in(
                                                dir,
                                                response.scope,
                                                &project_path,
                                            )
                                        })
                                    {
                                        if let Err(e) = Self::store_pattern_to(
                                            &file,
                                            &tool_call.name,
                                            args,
                                            pattern,
                                        ) {
                                            warn!(
                                                session_id = %stream_ctx.session_id,
                                                tool = %tool_call.name,
                                                pattern = %pattern,
                                                error = %e,
                                                "Failed to store pattern"
                                            );
                                        } else {
                                            info!(
                                                session_id = %stream_ctx.session_id,
                                                tool = %tool_call.name,
                                                pattern = %pattern,
                                                "Pattern stored for future use"
                                            );
                                        }
                                    }
                                }
                                (true, None)
                            } else {
                                (false, response.reason)
                            }
                        }
                        Err(_) => {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %tool_call.name,
                                permission_id = %permission_id,
                                "Permission channel dropped, treating as deny"
                            );
                            (false, None)
                        }
                    },
                };

                if permission_granted {
                    // The user was asked and said yes: nothing was granted on
                    // their behalf, so there is nothing to mark.
                    return Ok(None);
                }

                let resource_desc = Self::brief_resource_description(&tool_call.name, args);
                let error_msg = if let Some(reason) = &deny_reason {
                    format!(
                        "User denied permission to {} {}. Feedback: {}",
                        tool_call.name, resource_desc, reason
                    )
                } else {
                    format!(
                        "User denied permission to {} {}",
                        tool_call.name, resource_desc
                    )
                };

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    error = %error_msg,
                    "Permission denied, emitting error result"
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for permission denied tool_result event"
                    );
                }
                Err(error_msg)
            }
        }
    }

    /// A short description of what a tool call is acting on, for deny
    /// messages. Delegates to the shared projection so the phrase in an error
    /// matches what the UIs show for the same call.
    pub(in crate::agent_manager) fn brief_resource_description(
        tool_name: &str,
        args: &serde_json::Value,
    ) -> String {
        crucible_core::types::CanonicalToolCall::crucible_tool(tool_name, args)
            .summary(50)
            .unwrap_or_default()
    }

    /// Does a stored grant already answer for this call?
    ///
    /// Routes through [`pattern_kind`], the same classifier
    /// [`Self::store_pattern_to`] writes with.
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants
    )]
    pub(in crate::agent_manager) fn check_pattern_match(
        tool_name: &str,
        args: &serde_json::Value,
        pattern_store: &PatternStore,
    ) -> bool {
        match pattern_kind(tool_name, args) {
            PatternKind::Bash(command) => pattern_store.matches_bash(&command),
            PatternKind::File => {
                let path = args
                    .get("path")
                    .or_else(|| args.get("file"))
                    .or_else(|| args.get("name"))
                    .and_then(|v| v.as_str());
                if let Some(path) = path {
                    pattern_store.matches_file(path)
                } else {
                    false
                }
            }
            PatternKind::Tool => pattern_store.matches_tool(tool_name),
        }
    }

    /// Add `pattern` to the store at `file`, which a `Project` or `User`
    /// grant resolves through [`PatternStore::store_file_in`].
    ///
    /// `args` are the arguments of the call the user answered about. They are
    /// what [`pattern_kind`] reads, so the table this writes into is the table
    /// [`Self::check_pattern_match`] will read on the next identical call.
    ///
    /// The daemon is the only writer of a store file, so one process-wide
    /// lock serializes the load, the update and the save. Without it two
    /// sessions that grant at the same time overwrite each other.
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants
    )]
    pub(in crate::agent_manager) fn store_pattern_to(
        file: &std::path::Path,
        tool_name: &str,
        args: &serde_json::Value,
        pattern: &str,
    ) -> Result<(), crucible_core::config::PatternError> {
        static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = STORE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut store = PatternStore::load_file(file).unwrap_or_default();

        match pattern_kind(tool_name, args) {
            PatternKind::Bash(_) => store.add_bash_pattern(pattern)?,
            PatternKind::File => store.add_file_pattern(pattern)?,
            PatternKind::Tool => store.add_tool_pattern(pattern)?,
        }

        store.save_file(file)?;
        Ok(())
    }

    /// Ask this session's `cru.permissions.on_request` hooks, in priority
    /// order.
    ///
    /// One registry, on the one VM that runs Lua files. `None` is a manager
    /// with no daemon VM bound — every hook-free test — and means Prompt.
    ///
    /// `crucible_lua::handler_budget` gives the hooks 1 s. The VM stops a hook
    /// that runs longer, and this then returns `Prompt`.
    pub(super) fn run_permission_hooks(
        registry: Option<&super::super::DaemonPermissions>,
        tool_name: &str,
        args: &serde_json::Value,
        session_id: &str,
        session_mode: &str,
        mcp_read_only: &std::collections::HashSet<String>,
    ) -> PermissionHookResult {
        let Some((hooks, lua)) = registry else {
            return PermissionHookResult::Prompt;
        };

        let file_path = args
            .get("path")
            .or_else(|| args.get("file"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let request = PermissionRequest {
            tool_name: tool_name.to_string(),
            args: args.clone(),
            file_path,
            mode: Some(session_mode.to_string()),
            is_safe: crate::agent_manager::believed_read_only(tool_name, mcp_read_only),
        };

        match execute_permission_hooks(
            lua,
            hooks,
            &request,
            crucible_lua::Firing::InSession(session_id),
        ) {
            Ok(hook_result) => hook_result,
            Err(e) => {
                warn!(session_id = %session_id, tool = %tool_name, error = %e, "Permission hook failed");
                PermissionHookResult::Prompt
            }
        }
    }
}

/// Resolve the permission config to apply for a turn given the CLI override,
/// any agent-specific permissions, and the daemon's global permission config.
///
/// Priority: CLI override > agent-specific > global config.
///
/// For `Allow` and `Deny` overrides the user's intent is unconditional — the
/// returned config has the requested default and *empty* allow/deny/ask rule
/// lists, so base-config rules cannot re-introduce prompts or blocks. For
/// `Ask` the existing allow/deny/ask rules are preserved (interactive default).
pub(in crate::agent_manager) fn resolve_effective_permission_config(
    permission_override: Option<PermissionMode>,
    agent_permissions: Option<PermissionConfig>,
    global_permission_config: Option<PermissionConfig>,
) -> Option<PermissionConfig> {
    match permission_override {
        Some(mode @ (PermissionMode::Allow | PermissionMode::Deny)) => Some(PermissionConfig {
            default: mode,
            allow: Vec::new(),
            deny: Vec::new(),
            ask: Vec::new(),
        }),
        Some(PermissionMode::Ask) => {
            let mut config = agent_permissions
                .or(global_permission_config)
                .unwrap_or_default();
            config.default = PermissionMode::Ask;
            Some(config)
        }
        None => agent_permissions.or(global_permission_config),
    }
}

#[cfg(test)]
mod permission_serializer_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn serializer_lets_single_caller_through() {
        let s = PermissionSerializer::new();
        let result = s.run(async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn serializer_runs_concurrent_calls_one_at_a_time() {
        // Three concurrent callers must execute strictly serially.
        // Track in-flight count: it must never exceed 1.
        let s = PermissionSerializer::new();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let s = s.clone();
            let in_flight = in_flight.clone();
            let max_seen = max_seen.clone();
            handles.push(tokio::spawn(async move {
                s.run(async {
                    let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    let mut prev = max_seen.load(Ordering::SeqCst);
                    while n > prev {
                        match max_seen.compare_exchange(prev, n, Ordering::SeqCst, Ordering::SeqCst)
                        {
                            Ok(_) => break,
                            Err(actual) => prev = actual,
                        }
                    }
                    // Hold the section briefly so concurrent callers stack up.
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "concurrent callers must execute one at a time, but the in-flight high-water mark was higher"
        );
    }

    #[tokio::test]
    async fn serializer_releases_lock_after_completion() {
        // Once one call completes, the next one must be able to proceed.
        let s = PermissionSerializer::new();
        s.run(async {}).await;
        // Must complete without deadlock.
        let started = std::time::Instant::now();
        s.run(async {}).await;
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn separate_serializers_do_not_block_each_other() {
        // Per-session serialization: two distinct serializers should NOT
        // queue behind each other.
        let a = PermissionSerializer::new();
        let b = PermissionSerializer::new();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let task = |s: PermissionSerializer| {
            let in_flight = in_flight.clone();
            let max_seen = max_seen.clone();
            tokio::spawn(async move {
                s.run(async {
                    let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    let mut prev = max_seen.load(Ordering::SeqCst);
                    while n > prev {
                        match max_seen.compare_exchange(prev, n, Ordering::SeqCst, Ordering::SeqCst)
                        {
                            Ok(_) => break,
                            Err(actual) => prev = actual,
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            })
        };

        let h1 = task(a);
        let h2 = task(b);
        h1.await.unwrap();
        h2.await.unwrap();

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            2,
            "different serializers must not block each other; both should run concurrently"
        );
    }
}

#[cfg(test)]
mod acp_tool_name_tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome, ToolCallUpdateFields,
        ToolKind,
    };
    use crucible_core::interaction::PermRequest;

    fn fields(kind: Option<ToolKind>, title: &str) -> ToolCallUpdateFields {
        // `ToolCallUpdateFields` is `#[non_exhaustive]`, so it is built by
        // mutation rather than a struct literal.
        let mut f = ToolCallUpdateFields::default();
        f.kind = kind;
        f.title = Some(title.to_string());
        f
    }

    /// A tool call the agent named, and nothing else.
    fn named(name: &str) -> ToolCallUpdateFields {
        let mut f = ToolCallUpdateFields::default();
        f.name = Some(name.to_string());
        f
    }

    /// A Crucible tool loses the MCP prefix its server name made, so ONE key
    /// answers for the ACP agent and for the daemon's own tool path.
    ///
    /// Both wire forms are pinned: `mcp__<server>__<tool>` is what
    /// claude-agent-acp sends, `mcp.<server>.<tool>` what codex-acp does.
    #[test]
    fn a_crucible_tool_is_keyed_by_its_internal_name() {
        assert_eq!(
            acp_permission_tool_name(&named("mcp__crucible__read_note")),
            "read_note"
        );
        assert_eq!(
            acp_permission_tool_name(&named("mcp.crucible.read_note")),
            "read_note"
        );
    }

    /// Another server's tool keeps its whole name. Crucible does not own that
    /// name, and stripping the prefix would let `mcp__evil__read_note` take a
    /// rule written about Crucible's own `read_note`.
    #[test]
    fn another_mcp_server_keeps_its_whole_name() {
        assert_eq!(
            acp_permission_tool_name(&named("mcp__github__create_pr")),
            "mcp__github__create_pr"
        );
        assert_eq!(
            acp_permission_tool_name(&named("mcp__evil__read_note")),
            "mcp__evil__read_note"
        );
    }

    /// The agent's own tool is keyed by the name it sends, untouched.
    #[test]
    fn an_agent_tool_is_keyed_by_the_name_the_agent_sends() {
        assert_eq!(acp_permission_tool_name(&named("Bash")), "Bash");
        assert_eq!(acp_permission_tool_name(&named("Edit")), "Edit");
    }

    /// The name outranks the kind. A named MCP call whose `kind` is
    /// `execute` must not be keyed as `bash`.
    #[test]
    fn the_name_outranks_the_kind() {
        let mut f = named("mcp.crucible.read_note");
        f.kind = Some(ToolKind::Execute);
        assert_eq!(acp_permission_tool_name(&f), "read_note");
    }

    /// An agent that names no tool falls back to the coarse kind.
    #[test]
    fn an_unnamed_call_falls_back_to_the_kind() {
        assert_eq!(
            acp_permission_tool_name(&fields(Some(ToolKind::Execute), "Run cargo test")),
            "bash"
        );
        assert_eq!(
            acp_permission_tool_name(&fields(None, "Something")),
            "acp_tool"
        );
    }

    /// The gate must key on something an operator can write a rule against.
    ///
    /// It keyed on `title` — `"Read src/main.rs"` — which is prose. No
    /// `[permissions]` entry naming a tool could ever match it, so the entire
    /// permission config was inert for ACP-hosted agents.
    #[test]
    fn the_tool_name_comes_from_the_kind_not_the_prose_title() {
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Edit), "Edit src/main.rs")),
            "edit"
        );
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Execute), "Run cargo test")),
            "bash"
        );
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Read), "Read README")),
            "read"
        );
    }

    /// An unclassified call gets a name no file rule matches, rather than
    /// being guessed into one.
    #[test]
    fn an_unidentified_call_does_not_inherit_a_file_rule() {
        assert_eq!(acp_tool_name(&fields(None, "Something")), "acp_tool");
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Other), "Something")),
            "acp_tool"
        );
    }

    /// The point of all of it: an operator's rule now blocks an ACP call.
    ///
    /// Asserted through the gate rather than on the mapping alone, because a
    /// name that is correct and that no rule is evaluated against is the
    /// failure this fixes.
    #[tokio::test]
    async fn an_operator_rule_blocks_an_acp_tool_call() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["edit:*".to_string()],
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), true);

        let name = acp_tool_name(&fields(Some(ToolKind::Edit), "Edit src/main.rs"));
        let response = gate
            .request_permission(
                PermRequest::tool(name, serde_json::json!({"path": "src/main.rs"})),
                None,
            )
            .await;

        assert!(
            !response.allowed,
            "deny = [\"edit:*\"] must reach an ACP edit"
        );
    }

    /// …and an ACP `kind` never buys the read-only exemption, because the
    /// agent supplies it. Same reasoning `is_safe` gives for `readOnlyHint`.
    #[tokio::test]
    async fn a_read_kind_does_not_skip_the_prompt() {
        let gate = DaemonPermissionGate::new(None, true);
        let name = acp_tool_name(&fields(Some(ToolKind::Read), "Read /etc/passwd"));

        let response = gate
            .request_permission(PermRequest::tool(name, serde_json::json!({})), None)
            .await;

        assert!(
            !response.allowed,
            "an agent-supplied kind must not widen the gate"
        );
    }

    fn option(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), id.to_string(), kind)
    }

    fn selected(outcome: RequestPermissionOutcome) -> Option<String> {
        match outcome {
            RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.to_string()),
            _ => None,
        }
    }

    /// The exact kind wins when the agent offers it.
    #[test]
    fn the_option_of_the_exact_kind_is_selected() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("always", PermissionOptionKind::AllowAlways),
            option("no", PermissionOptionKind::RejectOnce),
        ];
        assert_eq!(
            selected(select_option(&options, PermissionOptionKind::AllowAlways)).as_deref(),
            Some("always")
        );
        assert_eq!(
            selected(select_option(&options, PermissionOptionKind::AllowOnce)).as_deref(),
            Some("once")
        );
    }

    /// The user allowed the call for the session, and the agent offers only
    /// `allow_once`. The call must run. `Cancelled` stopped the turn.
    #[test]
    fn an_allow_always_decision_takes_allow_once_when_the_agent_offers_only_that() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("no", PermissionOptionKind::RejectOnce),
        ];
        assert_eq!(
            selected(select_option(&options, PermissionOptionKind::AllowAlways)).as_deref(),
            Some("once")
        );
    }

    /// A one-time allow never takes `allow_always`. That option gives the
    /// agent a wider grant than the user chose.
    #[test]
    fn an_allow_once_decision_never_takes_allow_always() {
        let options = [
            option("always", PermissionOptionKind::AllowAlways),
            option("no", PermissionOptionKind::RejectOnce),
        ];
        assert!(matches!(
            select_option(&options, PermissionOptionKind::AllowOnce),
            RequestPermissionOutcome::Cancelled
        ));
    }

    /// A denial takes `reject_always` when the agent offers no
    /// `reject_once`. `Cancelled` stopped the whole turn instead of one call.
    #[test]
    fn a_reject_once_decision_takes_reject_always_when_the_agent_offers_only_that() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("never", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(
            selected(select_option(&options, PermissionOptionKind::RejectOnce)).as_deref(),
            Some("never")
        );
    }

    /// With no option of a usable kind, the outcome is `Cancelled`.
    #[test]
    fn no_usable_option_is_cancelled() {
        assert!(matches!(
            select_option(&[], PermissionOptionKind::RejectOnce),
            RequestPermissionOutcome::Cancelled
        ));
    }
}

/// The two agents' own permission frames, decided by the one tool policy.
///
/// The frames below are the shapes `claude-agent-acp` and `codex-acp` put on
/// the wire. They are the input the policy actually gets, so the tests build
/// them from JSON rather than from a builder that cannot be wrong.
#[cfg(test)]
mod acp_tool_policy_tests {
    use super::*;
    use agent_client_protocol::schema::v1::{RequestPermissionOutcome, RequestPermissionRequest};
    use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// One decided permission request, and whether the user was asked.
    struct Asked {
        outcome: RequestPermissionOutcome,
        prompts: usize,
    }

    impl Asked {
        fn allowed(&self) -> bool {
            matches!(self.outcome, RequestPermissionOutcome::Selected(ref selected)
                if selected.option_id.to_string().starts_with("allow"))
        }
    }

    /// The options every agent in these tests offers.
    fn options() -> serde_json::Value {
        serde_json::json!([
            { "optionId": "allow-once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "allow-always", "name": "Always", "kind": "allow_always" },
            { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" },
        ])
    }

    /// What claude-agent-acp sends: the tool name is on the request.
    fn claude_request(name: &str) -> RequestPermissionRequest {
        serde_json::from_value(serde_json::json!({
            "sessionId": "sess-1",
            "toolCall": {
                "toolCallId": "toolu_01",
                "name": name,
                "title": "Read the note",
                "status": "pending",
                "rawInput": { "title": "Some Note" },
            },
            "options": options(),
        }))
        .expect("the claude-agent-acp frame parses")
    }

    /// What codex-acp sends for an MCP approval: `kind: "execute"` and NO
    /// name. The ACP client fills `name` in from the `tool_call` frame that
    /// announced the same id, so `joined_name` is what that join produced.
    fn codex_request(joined_name: Option<&str>) -> RequestPermissionRequest {
        let mut tool_call = serde_json::json!({
            "toolCallId": "call-1",
            "kind": "execute",
            "status": "pending",
        });
        if let Some(name) = joined_name {
            tool_call["name"] = serde_json::json!(name);
        }
        serde_json::from_value(serde_json::json!({
            "sessionId": "sess-1",
            "toolCall": tool_call,
            "_meta": { "is_mcp_tool_approval": true },
            "options": options(),
        }))
        .expect("the codex-acp frame parses")
    }

    /// Decide one request against a card and an operator config, and count
    /// the prompts. A prompt that never fires is the whole point of a
    /// declared policy, so it is measured, not assumed.
    async fn decide(
        request: RequestPermissionRequest,
        card: &[(&str, ToolPolicy)],
        config: Option<PermissionConfig>,
    ) -> Asked {
        let prompts = Arc::new(AtomicUsize::new(0));
        let counter = prompts.clone();
        let callback: PermissionPromptCallback = Arc::new(move |_request| {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { PermResponse::allow() })
        });
        let gate = DaemonPermissionGate::new(config, true).with_prompt_callback(callback);

        let policy: ToolPolicyMap = card
            .iter()
            .map(|(name, policy)| ((*name).to_string(), *policy))
            .collect();
        let outcome = decide_acp_permission(&gate, Some(&policy), request).await;
        Asked {
            outcome,
            prompts: prompts.load(Ordering::SeqCst),
        }
    }

    /// A card `deny` refuses a Crucible MCP tool, and asks nobody.
    ///
    /// The card names the tool the daemon's own path names — `read_note` —
    /// while the agent sends `mcp__crucible__read_note`. One key, or the card
    /// silently does nothing for an ACP agent.
    #[tokio::test]
    async fn a_card_deny_refuses_a_crucible_mcp_tool() {
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Deny)],
            None,
        )
        .await;
        assert!(!asked.allowed(), "a card deny must refuse the call");
        assert_eq!(asked.prompts, 0, "a card deny asks nobody");
    }

    /// A card `allow` runs it, and asks nobody.
    #[tokio::test]
    async fn a_card_allow_runs_a_crucible_mcp_tool_without_a_prompt() {
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Allow)],
            None,
        )
        .await;
        assert!(asked.allowed(), "a card allow must run the call");
        assert_eq!(asked.prompts, 0, "a card allow asks nobody");
    }

    /// An operator rule reaches an ACP agent's tool.
    #[tokio::test]
    async fn an_operator_rule_refuses_a_crucible_mcp_tool() {
        let config = PermissionConfig {
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[],
            Some(config),
        )
        .await;
        assert!(!asked.allowed(), "`deny = [\"read_note:*\"]` must refuse");
        assert_eq!(asked.prompts, 0, "an operator deny asks nobody");
    }

    /// An operator `deny` outranks a card `allow` for an ACP agent too.
    ///
    /// The card answered ahead of the gate before, so an untrusted kiln could
    /// ship a card that walked past a configured deny.
    #[tokio::test]
    async fn an_operator_deny_outranks_a_card_allow() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Allow)],
            Some(config),
        )
        .await;
        assert!(
            !asked.allowed(),
            "a card allow must not beat an operator deny"
        );
    }

    /// A codex MCP approval carries `kind: "execute"` and no name of its own.
    /// It must not be keyed as `bash`.
    ///
    /// The coarse mapping turns every `execute` kind into `bash`, so a rule
    /// about the shell decided a note read, and a rule about the note read
    /// decided nothing.
    #[tokio::test]
    async fn a_codex_mcp_approval_is_not_a_bash_call() {
        let joined = || codex_request(Some("mcp.crucible.read_note"));

        let as_bash = decide(joined(), &[("bash", ToolPolicy::Deny)], None).await;
        assert!(
            as_bash.allowed(),
            "a rule about the shell must not decide an MCP note read"
        );

        let as_itself = decide(joined(), &[("read_note", ToolPolicy::Deny)], None).await;
        assert!(
            !as_itself.allowed(),
            "a rule about the note read must decide it"
        );
        assert_eq!(as_itself.prompts, 0, "a card deny asks nobody");
    }

    /// With no name at all — no `tool_call` frame ever announced this id —
    /// the coarse kind is the last resort, and the call is asked about.
    #[tokio::test]
    async fn an_unnamed_call_still_reaches_the_gate() {
        let asked = decide(codex_request(None), &[], None).await;
        assert_eq!(
            asked.prompts, 1,
            "a call the daemon cannot identify is put to the user"
        );
    }

    /// A tool the agent names and Crucible does not know still reaches the
    /// gate. It is neither allowed for being unknown nor refused for it.
    #[tokio::test]
    async fn a_tool_crucible_does_not_know_still_reaches_the_gate() {
        let asked = decide(claude_request("mcp__github__create_pr"), &[], None).await;
        assert_eq!(
            asked.prompts, 1,
            "an unknown tool is asked about, not guessed at"
        );
        assert!(asked.allowed(), "and the user's answer is honoured");
    }
}

/// Tests that call the real handler that `build_acp_permission_handler`
/// returns. The unit tests above cover its parts. These tests cover the
/// order of the parts: the declared policy, then the gate, then the prompt.
#[cfg(test)]
mod acp_permission_handler_tests {
    use super::*;
    use crate::agent_manager::tests::create_test_agent_manager;
    use crate::test_support::temp_session_manager;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
    use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
    use crucible_core::interaction::PermResponse;
    use std::time::Duration;

    const SESSION: &str = "acp-perm-session";

    /// An ACP `bash` call that offers exactly one allow and one reject.
    fn execute_request() -> RequestPermissionRequest {
        let mut fields = ToolCallUpdateFields::default();
        fields.kind = Some(ToolKind::Execute);
        fields.title = Some("Run cargo test".to_string());
        fields.raw_input = Some(serde_json::json!({ "command": "cargo test" }));
        RequestPermissionRequest::new(
            "acp-wire-session",
            ToolCallUpdate::new("call-1", fields),
            vec![
                PermissionOption::new("allow_once", "Allow once", PermissionOptionKind::AllowOnce),
                PermissionOption::new(
                    "reject_once",
                    "Reject once",
                    PermissionOptionKind::RejectOnce,
                ),
            ],
        )
    }

    fn selected(outcome: &RequestPermissionOutcome) -> Option<String> {
        match outcome {
            RequestPermissionOutcome::Selected(s) => Some(s.option_id.to_string()),
            _ => None,
        }
    }

    fn policy(stance: ToolPolicy) -> ToolPolicyMap {
        ToolPolicyMap::from([("bash".to_string(), stance)])
    }

    /// Build the handler for an interactive session with no override.
    fn handler(
        am: &AgentManager,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        tool_policy: Option<ToolPolicyMap>,
    ) -> crate::acp::client::PermissionRequestHandler {
        am.build_acp_permission_handler(SESSION, event_tx, true, None, None, tool_policy)
    }

    /// Read events until the prompt arrives. Return its request id.
    async fn prompt_id(rx: &mut broadcast::Receiver<SessionEventMessage>) -> String {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("the handler must emit interaction_requested")
                .expect("event channel open");
            if msg.event == "interaction_requested" {
                return msg.data["request_id"]
                    .as_str()
                    .expect("request_id")
                    .to_string();
            }
        }
    }

    /// A card that allows `bash` answers the call. No prompt appears.
    #[tokio::test]
    async fn a_declared_allow_selects_allow_once_without_a_prompt() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, Some(policy(ToolPolicy::Allow)));

        let outcome = handle(execute_request()).await;

        assert_eq!(selected(&outcome).as_deref(), Some("allow_once"));
        assert!(
            event_rx.try_recv().is_err(),
            "a declared Allow must not prompt the user"
        );
        assert!(am.list_all_pending_permissions().is_empty());
    }

    /// A card that denies `bash` refuses the call. No prompt appears.
    #[tokio::test]
    async fn a_declared_deny_selects_reject_once_without_a_prompt() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, Some(policy(ToolPolicy::Deny)));

        let outcome = handle(execute_request()).await;

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(
            event_rx.try_recv().is_err(),
            "a declared Deny must not prompt the user"
        );
    }

    /// With no policy, the handler asks the user. The answer goes through
    /// the manager reply API, as a client answer does.
    #[tokio::test]
    async fn with_no_policy_the_user_answer_selects_the_option() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        for (answer, expected) in [
            (PermResponse::allow(), "allow_once"),
            (PermResponse::deny(), "reject_once"),
        ] {
            let pending = tokio::spawn(handle(execute_request()));
            let id = prompt_id(&mut event_rx).await;
            am.respond_to_permission(SESSION, &id, answer)
                .expect("the prompt must be in the session registry");

            let outcome = tokio::time::timeout(Duration::from_secs(5), pending)
                .await
                .expect("the handler must return after the answer")
                .expect("join");
            assert_eq!(selected(&outcome).as_deref(), Some(expected));
        }
    }

    /// Nobody answers. After 300 s the handler rejects the call. It does not
    /// wait forever. It also removes the prompt from the registry.
    #[tokio::test(start_paused = true)]
    async fn an_unanswered_prompt_rejects_after_the_timeout() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let pending = tokio::spawn(handle(execute_request()));
        let _id = prompt_id(&mut event_rx).await;
        assert_eq!(am.list_all_pending_permissions().len(), 1);

        tokio::time::advance(Duration::from_secs(301)).await;
        let outcome = tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .expect("the handler must return after the 300 s timeout")
            .expect("join");

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(
            am.list_all_pending_permissions().is_empty(),
            "a timed-out prompt must leave the registry"
        );
    }
}
