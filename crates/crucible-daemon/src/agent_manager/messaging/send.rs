use super::super::*;
use crucible_core::config::components::permissions::PermissionMode;
use crucible_core::turn::{StopReason, TurnOrigin};

/// Map the stream driver's outcome to the status, the last stop reason and
/// the error text of the turn.
fn outcome_to_status(outcome: StreamOutcome) -> (TurnStatus, Option<StopReason>, Option<String>) {
    match outcome {
        StreamOutcome::Completed(stop_reason) => (TurnStatus::Completed, stop_reason, None),
        StreamOutcome::HandlerCancelled(reason) => {
            (TurnStatus::HandlerCancelled, None, Some(reason))
        }
        StreamOutcome::Failed(reason) => (TurnStatus::Failed, None, Some(reason)),
    }
}

/// What one turn needs beside the session and the message.
///
/// The fields travel together through the entry points of a turn. A struct
/// keeps each call readable, and it lets a new field reach the turn without a
/// new argument on every caller.
pub(crate) struct TurnRequest<'a> {
    /// Who asked for the turn. A plugin turn has no caller to notify.
    pub origin: TurnOrigin,
    /// Reset conversation context after claiming the turn slot, before the
    /// prompt is committed. This makes clear-with-prompt one admitted turn.
    pub clear_before: bool,
    /// The review comments that the message attaches, as the daemon rendered
    /// them. `server::diff_context::review_context` builds this text.
    pub review_context: Option<crate::diff::context::ReviewContext>,
    pub event_tx: &'a broadcast::Sender<SessionEventMessage>,
    pub is_interactive: bool,
    pub permission_override: Option<PermissionMode>,
    /// Resolved when the turn reaches a terminal state. `None` when nobody
    /// awaits the turn.
    pub completion_tx: Option<oneshot::Sender<TurnOutcome>>,
}

impl AgentManager {
    /// Clear context in this session. With a prompt, the marker and the new
    /// turn share one request claim, so another sender cannot slip between.
    ///
    /// A plugin clear outside a turn has no turn to copy. Its turn takes the
    /// session's stored mode and asks the user, as a plugin's own send does.
    pub fn clear_session<'a>(
        self: &'a Arc<Self>,
        session_id: &'a str,
        prompt: Option<String>,
        plugin: Option<String>,
        event_tx: &'a broadcast::Sender<SessionEventMessage>,
    ) -> futures::future::BoxFuture<'a, Result<Option<String>, AgentError>> {
        let gate = crate::agent_manager::slot::TurnGate {
            is_interactive: true,
            permission_override: None,
            origin: plugin.map_or(TurnOrigin::User, TurnOrigin::Plugin),
        };
        self.clear_with_gate(session_id, prompt, gate, event_tx)
    }

    /// Clear context. The turn of `prompt` takes the interactivity and the
    /// override of `gate`. A plugin clear during a turn waits for the end
    /// of that turn, and then takes the gate of that turn.
    fn clear_with_gate<'a>(
        self: &'a Arc<Self>,
        session_id: &'a str,
        prompt: Option<String>,
        gate: crate::agent_manager::slot::TurnGate,
        event_tx: &'a broadcast::Sender<SessionEventMessage>,
    ) -> futures::future::BoxFuture<'a, Result<Option<String>, AgentError>> {
        Box::pin(async move {
            if gate.origin.plugin().is_some() && self.request_state.contains_key(session_id) {
                let slot = self.slot(session_id);
                let turn = slot.turn_gate().unwrap_or_default();
                slot.set_clear_after_turn(crate::agent_manager::slot::ClearAfterTurn {
                    prompt,
                    gate: crate::agent_manager::slot::TurnGate {
                        origin: gate.origin,
                        ..turn
                    },
                });
                return Ok(None);
            }
            let plugin = gate.origin.plugin().map(str::to_owned);
            if let Some(prompt) = prompt {
                return self
                    .send_message_inner(
                        session_id,
                        prompt,
                        TurnRequest {
                            origin: gate.origin,
                            clear_before: true,
                            review_context: None,
                            event_tx,
                            is_interactive: gate.is_interactive,
                            permission_override: gate.permission_override,
                            completion_tx: None,
                        },
                    )
                    .await
                    .map(Some);
            }

            let session = self.get_or_revive_session(session_id).await?;
            let (cancel_tx, _cancel_rx) = oneshot::channel();
            match self.request_state.entry(session_id.to_string()) {
                dashmap::mapref::entry::Entry::Occupied(_) => {
                    return Err(AgentError::ConcurrentRequest(session_id.to_string()));
                }
                dashmap::mapref::entry::Entry::Vacant(entry) => {
                    entry.insert(RequestState {
                        cancel_tx: Some(cancel_tx),
                        task_handle: None,
                        _work: Some(self.activity().start(crate::activity::WorkKind::Turn)),
                    });
                }
            }
            let result = self
                .clear_context_inner(&session, plugin.as_deref(), event_tx)
                .await;
            let result = match result {
                Ok(()) => self.ensure_agent_handle(session_id, Some(event_tx)).await,
                Err(error) => Err(error),
            };
            self.request_state.remove(session_id);
            result.map(|()| None)
        })
    }

    async fn clear_context_inner(
        &self,
        session: &crucible_core::session::Session,
        plugin: Option<&str>,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<(), AgentError> {
        let session_id = session.id.to_string();
        let slot = self.slot(&session_id);
        let mut input = slot.input.lock().await;
        let tree = self
            .get_or_rebuild_session_tree(
                session_id.as_str(),
                &session.jsonl_path(self.session_manager.sessions_root()),
            )
            .await;
        let marker = crate::observe::LogEvent::Clear {
            ts: chrono::Utc::now(),
            plugin: plugin.map(str::to_owned),
        };
        self.session_manager
            .storage()
            .append_event(
                session,
                &marker
                    .to_jsonl()
                    .map_err(crate::session_manager::SessionError::from)?,
            )
            .await?;
        *tree.lock().await = crucible_core::turn::ConversationTree::new();
        input.pending.clear();
        input.after_turn = None;
        drop(input);

        if session
            .agent
            .as_ref()
            .is_some_and(|agent| agent.agent_type == "acp")
        {
            self.session_manager
                .modify_session(&session_id, |live| {
                    live.acp_session_id = None;
                    true
                })
                .await?;
            slot.invalidate_agent();
        }
        emit_event(
            event_tx,
            SessionEventMessage::typed(
                session_id.as_str(),
                crucible_core::protocol::session_events::TurnPayload::ContextCleared {
                    plugin: plugin.map(str::to_owned),
                },
            ),
        );
        Ok(())
    }

    pub async fn send_message(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<String, AgentError> {
        self.send_message_inner(
            session_id,
            content,
            TurnRequest {
                origin: TurnOrigin::User,
                clear_before: false,
                review_context: None,
                event_tx,
                is_interactive,
                permission_override,
                completion_tx: None,
            },
        )
        .await
    }

    /// Send `content` as a turn of the plugin `plugin`, when no turn runs.
    /// It takes the session's stored mode and the plugin's approval, and a
    /// prompt waits for the user (decision 12).
    pub async fn send_plugin_message(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        plugin: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<String, AgentError> {
        self.send_message_inner(
            session_id,
            content,
            TurnRequest {
                origin: TurnOrigin::Plugin(plugin),
                clear_before: false,
                review_context: None,
                event_tx,
                is_interactive: true,
                permission_override: None,
                completion_tx: None,
            },
        )
        .await
    }

    /// Send the words that a person wrote in the channel of the plugin
    /// `relay`, such as Discord. The turn is a user turn that names its relay.
    pub async fn send_relayed_message(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        relay: Option<String>,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
    ) -> Result<String, AgentError> {
        self.send_message_inner(
            session_id,
            content,
            TurnRequest {
                origin: relay.map_or(TurnOrigin::User, TurnOrigin::Relay),
                clear_before: false,
                review_context: None,
                event_tx,
                is_interactive,
                permission_override: None,
                completion_tx: None,
            },
        )
        .await
    }

    /// Like [`send_message`], with the review comments that the message
    /// attaches.
    pub async fn send_message_with_context(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        review_context: Option<crate::diff::context::ReviewContext>,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<String, AgentError> {
        self.send_message_inner(
            session_id,
            content,
            TurnRequest {
                origin: TurnOrigin::User,
                clear_before: false,
                review_context,
                event_tx,
                is_interactive,
                permission_override,
                completion_tx: None,
            },
        )
        .await
    }

    /// Like [`send_message`], but also returns a completion channel resolved
    /// when the turn reaches ANY terminal state (completed, cancelled, timed
    /// out, failed). This is the only reliable way to await a turn: the event
    /// bus emits no terminal event on successful completion.
    pub async fn send_message_notified(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<(String, oneshot::Receiver<TurnOutcome>), AgentError> {
        let (completion_tx, completion_rx) = oneshot::channel();
        let message_id = self
            .send_message_inner(
                session_id,
                content,
                TurnRequest {
                    origin: TurnOrigin::User,
                    clear_before: false,
                    review_context: None,
                    event_tx,
                    is_interactive,
                    permission_override,
                    completion_tx: Some(completion_tx),
                },
            )
            .await?;
        Ok((message_id, completion_rx))
    }

    /// Start the turn a `turn:complete` handler asked for.
    ///
    /// Boxed, because it re-enters `send_message_inner`, which spawns the
    /// task that calls this: an unboxed future would hold its own type.
    ///
    /// The turn takes the request slot like any other. A client that sends
    /// its own message in the same instant can win that slot, and the
    /// handler's turn is then lost — said out loud here rather than hidden,
    /// because one slot per session is what stops two turns at once.
    fn start_follow_up_turn(
        self: &Arc<Self>,
        session_id: String,
        follow_up: crate::agent_manager::slot::FollowUpTurn,
        event_tx: broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> futures::future::BoxFuture<'static, ()> {
        let manager = self.clone();
        Box::pin(async move {
            info!(
                session_id = %session_id,
                "Starting the turn a turn:complete handler asked for"
            );
            if let Err(e) = manager
                .send_message_inner(
                    &session_id,
                    follow_up.content,
                    TurnRequest {
                        origin: TurnOrigin::Plugin(follow_up.plugin),
                        clear_before: false,
                        review_context: None,
                        event_tx: &event_tx,
                        is_interactive,
                        permission_override,
                        completion_tx: None,
                    },
                )
                .await
            {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "The turn a turn:complete handler asked for did not start"
                );
            }
        })
    }

    async fn send_message_inner(
        self: &Arc<Self>,
        session_id: &str,
        content: String,
        request: TurnRequest<'_>,
    ) -> Result<String, AgentError> {
        let TurnRequest {
            origin,
            clear_before,
            review_context,
            event_tx,
            is_interactive,
            permission_override,
            completion_tx,
        } = request;
        let ttft_start = Instant::now();
        info!(target: "ttft", session_id = %session_id, stage = "send_message_entry", elapsed_ms = 0, "ttft");
        // Sessions are always resumable: if the target is no longer resident in
        // memory (ended or evicted), transparently revive it from storage before
        // processing the turn so the caller never has to gate on lifecycle state.
        let session = self.get_or_revive_session(session_id).await?;

        let agent_config = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        use dashmap::mapref::entry::Entry;
        let (cancel_tx, cancel_rx) = oneshot::channel();

        match self.request_state.entry(session_id.to_string()) {
            Entry::Occupied(_) => {
                return Err(AgentError::ConcurrentRequest(session_id.to_string()));
            }
            Entry::Vacant(e) => {
                e.insert(RequestState {
                    cancel_tx: Some(cancel_tx),
                    task_handle: None,
                    // Held until the stream task releases the slot at the end
                    // of the turn. This is what keeps the daemon from exiting
                    // mid-turn once the client has detached.
                    _work: Some(self.activity().start(crate::activity::WorkKind::Turn)),
                });
            }
        }

        if clear_before {
            if let Err(error) = self
                .clear_context_inner(&session, origin.plugin(), event_tx)
                .await
            {
                self.request_state.remove(session_id);
                return Err(error);
            }
        }

        // Where the agent's tools act. A session with no workspace still
        // anchors somewhere concrete; see `scope::session_tool_root`.
        let tool_root = crate::agent_manager::scope::session_tool_root(
            &session,
            self.session_manager.sessions_root(),
        );

        let event_tx_clone = event_tx.clone();
        let agent = match self
            .get_or_create_agent(session_id, &agent_config, &tool_root, &event_tx_clone)
            .await
        {
            Ok(agent) => agent,
            Err(e) => {
                self.request_state.remove(session_id);
                return Err(e);
            }
        };
        info!(target: "ttft", session_id = %session_id, stage = "agent_ready", elapsed_ms = ttft_start.elapsed().as_millis() as u64, "ttft");

        let input_slot = self.slot(session_id);
        match &origin {
            TurnOrigin::User | TurnOrigin::Relay(_) => input_slot
                .plugin_turn_count
                .store(0, std::sync::atomic::Ordering::Relaxed),
            TurnOrigin::Plugin(plugin) => {
                let count = input_slot
                    .plugin_turn_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    .saturating_add(1);
                if count >= session.plugin_turn_limit {
                    let approval = match self.get_plugin_approval(session_id, plugin).await {
                        Ok(approval) => approval,
                        Err(error) => {
                            self.request_state.remove(session_id);
                            return Err(error);
                        }
                    };
                    if approval == crucible_core::session::PluginApproval::Inherit {
                        if let Err(error) = self
                            .set_plugin_approval(
                                session_id,
                                plugin,
                                crucible_core::session::PluginApproval::Ask,
                                Some(event_tx),
                            )
                            .await
                        {
                            self.request_state.remove(session_id);
                            return Err(error);
                        }
                        let notice = crucible_core::types::Notification::warning(format!(
                            "Plugin {plugin} reached the {} consecutive turn limit; approval is now Ask",
                            session.plugin_turn_limit
                        ));
                        if let Err(error) = self
                            .add_notification(session_id, notice, Some(event_tx))
                            .await
                        {
                            self.request_state.remove(session_id);
                            return Err(error);
                        }
                    }
                }
            }
        }

        let message_id = format!("msg-{}", uuid::Uuid::new_v4());
        let original_content = content;

        // Scheduler-owned conversation tree: commit the user message
        // node before the agent turn starts. The tree is the
        // authoritative source of conversation state; the agent handle
        // receives the flattened path via `TurnContext.messages` and
        // does not hold history between turns.
        //
        // Workspace snapshot capture happens *before* we add the User
        // node, keyed by the cursor's pre-turn node id. After
        // `undo_turns(n)` the cursor lands on that exact node — the
        // parent of the rewound User — so on undo the daemon looks up
        // the snapshot under the new cursor and restores it.
        // Use the rebuilding variant so a daemon restart that resumes a
        // persisted session sees its prior history. Without this the
        // first-user-message gate (Precognition / digest) treats every
        // post-restart message as first.
        //
        // Fetched BEFORE this turn's `user_message` is emitted, and that
        // ordering is load-bearing. The rebuild reads `session.jsonl`, which a
        // separate writer task appends the emitted event to; issued after the
        // emit, the two race. When the append won, the rebuilt tree already
        // held this turn's User node, the append below made it the second, and
        // `undo_depth() == 1` — the first-user-message gate — read false. The
        // turn then ran with Precognition silently skipped: no warning, no
        // `precognition_complete`, nothing in the transcript to say the answer
        // was ungrounded. Reproduced by widening the window to 200ms, which
        // turns `oneshot_precognition_query_e2e` red every run.
        let mut input = input_slot.input.lock().await;
        let conversation_tree = self
            .get_or_rebuild_session_tree(
                session_id,
                &session.jsonl_path(self.session_manager.sessions_root()),
            )
            .await;

        // The review comments that the message attaches. An internal agent
        // gets them as accepted context before the user turn, so replay, undo
        // and fork keep them with their role. An ACP agent owns its history,
        // so the block goes with this turn only, as the `@file` attachments do.
        //
        // Both routes tag the block with its kind. A `transform_context`
        // handler then finds the block by its tag on either route, instead of
        // matching a substring of the text the daemon rendered.
        let mut acp_review_context = None;
        if let Some(review) = review_context {
            if agent_config.agent_type == "acp" {
                acp_review_context = Some(
                    crucible_core::traits::ContextMessage::injection(
                        crate::diff::context::KIND,
                        review.source,
                        &review.body,
                    )
                    .with_tag(crate::diff::context::KIND),
                );
            } else if let Err(e) = input
                .accept(
                    self.session_manager.storage().as_ref(),
                    &session,
                    crate::observe::LogEvent::System {
                        ts: chrono::Utc::now(),
                        content: review.body,
                        tags: vec![crate::diff::context::KIND.to_string()],
                        injection: Some((
                            crate::diff::context::KIND.to_string(),
                            review.source.to_string(),
                        )),
                    },
                )
                .await
            {
                self.request_state.remove(session_id);
                return Err(e.into());
            }
        }

        // One exhaustive table, so a new origin fails to compile here.
        let opening_event = match &origin {
            TurnOrigin::User => {
                SessionEventMessage::user_message(session_id, &message_id, &original_content)
            }
            TurnOrigin::Plugin(_) | TurnOrigin::Relay(_) => SessionEventMessage::typed(
                session_id,
                crucible_core::protocol::session_events::TurnPayload::UserMessage {
                    message_id: message_id.clone(),
                    content: original_content.clone(),
                    origin: Some(origin.clone()),
                },
            ),
        };
        if !emit_event(event_tx, opening_event) {
            warn!(session_id = %session_id, "No subscribers for user_message event");
        }

        let snapshot_key_node = {
            let mut t = conversation_tree.lock().await;
            // Undo lands on the user node's parent, so accepted context must
            // already be in the tree when the snapshot key is chosen.
            for message in input.pending.drain(..) {
                crate::observe::rebuild::apply_injection_to_tree(&mut t, &message);
            }
            t.current()
        };
        let snapshot = crate::workspace_snapshot::WorkspaceSnapshot::create(
            &tool_root,
            session_id,
            snapshot_key_node.index(),
        )
        .await;
        self.snapshots
            .insert(session_id.to_string(), snapshot_key_node.index(), snapshot);

        // Open the review ledger over every root this session may write to,
        // not just the workspace: a note written into a kiln that is not the
        // workspace is a real change the composed diff has to show, and only
        // `session.workspace` is snapshotted above.
        //
        // The session's storage dir is deliberately excluded — it is
        // daemon-owned spill under `~/.crucible`, never review material.
        //
        // Re-opening is a no-op, so `session_base` is captured on the first
        // turn and never recomputed. Recomputing it on a later turn would
        // report that the agent had changed nothing, which reads as success
        // rather than as data loss — and the same is true across a daemon
        // restart, which is why this restores from `review.jsonl` rather than
        // opening blind. A journal that exists and will not read is an error
        // here, never a fresh base.
        // The workspace only when there IS one — never the tool anchor, which
        // for a workspace-less session is its own storage directory, and the
        // comment above says exactly why that must not be review material.
        let mut review_roots: Vec<std::path::PathBuf> = session.workspace.iter().cloned().collect();
        review_roots.extend(self.session_manager.kiln_paths(&session.kilns));
        if let Err(e) = self
            .review
            .open_or_restore(
                session_id,
                &session.storage_path(self.session_manager.sessions_root()),
                &review_roots,
            )
            .await
        {
            // Two different failures land here and they are not equally
            // benign — a root that is not there has nothing to track, while an
            // unreadable journal is lost evidence. They are logged the same
            // way because the *distinction* is not carried by this error: an
            // unreadable journal is recorded on the session's `Integrity`
            // inside `open_or_restore`, which is what holds its writes rather
            // than letting them through unattributed. A root outside git is no
            // longer one of these: it is tracked through the plain backend.
            debug!(
                session_id = %session_id,
                error = %e,
                "review ledger not opened; this session's changes go unattributed"
            );
        }

        // A delegated child records its own intervals; the parent has none for
        // the delegation, by design. Registering the link here — off the field
        // the session already carries — is what lets the child's attribution be
        // harvested up when it ends, and needs no tool call id threaded through
        // `DelegationRequest`. Idempotent, so re-registering on every turn is
        // the same as registering on the first.
        if let Some(parent) = session.parent_session_id.as_deref() {
            self.review.set_parent(session_id, parent);
        }

        // Watch the ledger's roots, not `review_roots`: `open` normalises each
        // to its repository top level and dedupes, and the tracker matches
        // observed paths by prefix, so a root spelled any other way than the
        // way git prints it matches nothing the watcher ever sees. Re-watching
        // a root another session already holds is refcounted, not duplicated.
        if let Some(watch) = self.external_watch() {
            let roots: Vec<std::path::PathBuf> = self
                .review
                .ledger(session_id)
                .map(|l| l.roots().map(std::path::Path::to_path_buf).collect())
                .unwrap_or_default();
            if !roots.is_empty() {
                if let Err(e) = watch.watch_session(session_id, &roots).await {
                    debug!(
                        session_id = %session_id,
                        error = %e,
                        "review watch not established; external edits will not push"
                    );
                }
            }
        }

        let is_first_user_message = {
            let mut t = conversation_tree.lock().await;
            input.after_turn = Some(message_id.clone());
            let parent = t.current();
            let turn_node = match &origin {
                TurnOrigin::User | TurnOrigin::Relay(_) => crucible_core::turn::NodeContent::User {
                    text: original_content.clone(),
                },
                TurnOrigin::Plugin(plugin) => crucible_core::turn::NodeContent::Plugin {
                    text: original_content.clone(),
                    name: plugin.clone(),
                },
            };
            t.add_child_and_advance(parent, turn_node);
            // After append+advance, undo_depth() is the count of User
            // nodes on the current path — 1 means this turn is the first.
            t.undo_depth() == 1
        };

        drop(input);
        info!(target: "ttft", session_id = %session_id, stage = "precognition_start", elapsed_ms = ttft_start.elapsed().as_millis() as u64, "ttft");
        let precognition_message =
            if crate::agent_manager::precognition_gate::should_run_precognition(
                agent_config.precognition_enabled,
                &original_content,
                &session.kilns,
                is_first_user_message,
            ) {
                self.compute_precognition_message(
                    session_id,
                    &original_content,
                    &session,
                    &agent_config,
                    event_tx,
                )
                .await
            } else {
                None
            };
        info!(target: "ttft", session_id = %session_id, stage = "precognition_done", elapsed_ms = ttft_start.elapsed().as_millis() as u64, "ttft");

        // `@file` mentions resolve here, on every turn — not just the first,
        // and regardless of Precognition: attaching a file is something the
        // user did on purpose, so it is not subject to the auto-RAG gate.
        // The containment of the session decides which files a mention can
        // read, so a mention cannot reach more than a `read_file` call.
        let mention_roots = crate::agent_manager::scope::session_containment(
            &session,
            self.session_manager.sessions_root(),
            self.session_manager.kiln_registry(),
        );
        let attachment_message = crate::agent_manager::attachments::build_attachment_message(
            &tool_root,
            &mention_roots,
            &original_content,
        );
        if attachment_message.is_some() {
            debug!(session_id = %session_id, "Attached @-mentioned file contents to the turn");
        }
        // One injection is one element, so the review and the files go as
        // two messages.
        let attachment_messages: Vec<_> = acp_review_context
            .into_iter()
            .chain(attachment_message)
            .collect();

        // Pass the user's content through to the stream loop unchanged;
        // the Precognition system block (if any) is staged on
        // StreamContext and prepended by apply_transform_context_handlers
        // when the seam fires. This is the migration from string-level
        // mutation (old enrich_with_precognition) to the message-array
        // seam — Lua transform_context handlers can now further mutate
        // the precognition message via the same seam.
        // ACP owns its history and receives this turn through a user-role
        // prompt. Tag plugin text here; the internal agent gets the tagged
        // system node from the scheduler-owned conversation tree instead.
        let content = match origin.plugin() {
            Some(plugin) if agent_config.agent_type == "acp" => {
                crucible_core::traits::ContextMessage::injection(
                    "plugin",
                    plugin,
                    &original_content,
                )
                .content
            }
            _ => original_content.clone(),
        };

        let session_id_owned = session_id.to_string();
        let request_state = self.request_state.clone();
        // Snapshot the agent's mode for this request so the tool-dispatch
        // path can enforce plan-mode restrictions on unwrapped invoke_tool
        // calls without reaching back into the agent handle.
        //
        // Drain any pending mode change first: a `set_mode` RPC that arrived
        // while the cached handle was busy serving the PREVIOUS turn stored
        // its new mode in the slot's `pending_mode` rather than block (or evict —
        // which
        // would lose ACP conversation history). Applying it here, on the
        // handle we've just locked for THIS turn, is the earliest safe point:
        // the previous turn has released the lock, and we haven't yet snapshotted
        // the mode for tool-dispatch enforcement.
        let session_mode = {
            let mut guard = agent.lock().await;
            if let Some(pending) = self.slot(session_id).take_pending_mode() {
                match guard.apply_mode(&pending).await {
                    Ok(()) => tracing::info!(
                        session_id = %session_id,
                        mode = %pending,
                        "applied deferred mode change at turn start"
                    ),
                    Err(e) => tracing::warn!(
                        session_id = %session_id,
                        mode = %pending,
                        error = %e,
                        "deferred mode change rejected by handle; using existing mode"
                    ),
                }
            }
            guard.get_mode_id().to_string()
        };
        // The note tools read the write mode of this turn from the slot. It
        // follows the mode snapshot above, so a mode change during the turn
        // changes the next turn only. An ACP agent always applies its writes.
        self.slot(session_id).write_mode().set(
            self.mode_writes(&session_mode)
                .effective_for(&agent_config.agent_type),
        );
        // The ACP permission handler of a cached handle reads this for each
        // call, so the handle keeps nothing of the turn that built it.
        self.slot(session_id)
            .set_turn_gate(crate::agent_manager::slot::TurnGate {
                is_interactive,
                permission_override,
                origin: origin.clone(),
            });
        // The turn proposal ends with the turn, on every exit path below.
        let proposals = self.proposals.clone();
        let proposal_session = session.id.clone();
        // Snapshot the plugin tool names for the plan-mode dispatch guard —
        // resolved per turn, so tools from a plugin loaded mid-session are
        // still covered.
        let plugin_tool_names = match self.plugin_registry().await {
            Some(registry) => registry.tool_names(),
            None => Default::default(),
        };
        // Same snapshot rule for what external MCP servers declared read-only:
        // resolved per turn, so a server connected mid-session is covered.
        let mcp_read_only_tools = match &self.mcp_gateway {
            Some(gw) => gw.read().await.read_only_tool_names(),
            None => Default::default(),
        };
        let stream_ctx = StreamContext {
            session_id: session_id_owned.clone(),
            message_id: message_id.clone(),
            event_tx: event_tx_clone.clone(),
            slot: self.slot(session_id),
            workspace_path: tool_root.clone(),
            session_dir: session.storage_path(self.session_manager.sessions_root()),
            whitelists_dir: self.whitelists_dir(),
            agent_stream_config: {
                AgentStreamConfig::from_session_agent(
                    &agent_config,
                    TurnEnvironment {
                        plugin_handlers: self.plugin_handlers(),
                        isolation: self.isolation(),
                        plugin_tool_names,
                        modes: self.modes.clone(),
                        mcp_read_only_tools,
                    },
                )
                // Capture and the gate read the same ledgers, so the turn
                // carries one handle to them rather than two.
                .with_review(self.review.clone())
                // The dispatch half of `cru.tools.set_active`: filtering only
                // the advertised set would leave every excluded tool callable
                // by a model that names one anyway.
                .with_active_tools(self.active_tools())
            },
            tool_dispatcher: self.get_or_create_session_dispatcher(&session).await,
            permission_override,
            conversation_tree,
            session_manager: self.session_manager.clone(),
            precognition_message,
            attachment_messages,
            session_mode,
            origin,
            is_interactive,
            // The same rules the ACP gate reads: the agent profile's, else
            // the global config.
            permission_engine: Arc::new(self.session_permission_engine(session_id)),
            context_attach: self.context_attach(),
        };

        let manager = self.clone();
        let slot = self.slot(session_id);
        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();
            let stream_config = stream_ctx.agent_stream_config.clone();

            let stream_future = Self::execute_agent_stream(
                agent,
                content,
                stream_ctx.clone(),
                stream_config,
                &mut accumulated_response,
            );

            let (status, stop_reason, error) = tokio::select! {
                _ = cancel_rx => {
                    debug!(session_id = %session_id_owned, "Request cancelled");
                    // The user stopped the work, so the turn a turn:complete
                    // handler asked for stops with it. Dropping the stored
                    // content is the whole clear.
                    drop(slot.take_follow_up());
                    drop(slot.take_clear_after_turn());
                    (TurnStatus::Cancelled, None, None)
                }
                outcome = stream_future => outcome_to_status(outcome),
            };

            // The next turn starts a new proposal. The store forgets the turn
            // proposal before an awaiter sees the outcome and sends again.
            proposals.end_turn(&proposal_session);
            // The same for the permission state of the turn.
            slot.clear_turn_gate();

            // A caller that awaits the turn (a workflow step, a delegation)
            // owns the next turn of the session, so a `turn:complete` handler
            // gets no follow-up turn: it would take the slot from the caller.
            let awaited = completion_tx.is_some();
            // Single convergence point for ALL exit paths — this send must
            // happen before the request_state slot is released so an awaiter
            // observes the outcome strictly after the turn is over.
            if let Some(tx) = completion_tx {
                let _ = tx.send(TurnOutcome {
                    status,
                    final_text: std::mem::take(&mut accumulated_response),
                    error: error.clone(),
                });
            }

            request_state.remove(&session_id_owned);

            // The one event that ends the whole turn for the clients. It
            // comes after the slot is free, so a client that sends its next
            // message when it sees this event does not get
            // `ConcurrentRequest`.
            if !emit_event(
                &event_tx_clone,
                SessionEventMessage::turn_finished(&session_id_owned, status, stop_reason, error),
            ) {
                warn!(session_id = %session_id_owned, "No subscribers for turn_finished event");
            }

            // A turn ENDS. A `turn:complete` handler that wants more work gets
            // a NEW turn, here, with the slot free. It is a normal turn, so it
            // takes admission, Precognition, persistence and undo like any
            // other; only its origin says who asked for it.
            if let Some(clear) = slot.take_clear_after_turn() {
                drop(slot.take_follow_up());
                let clear_request: futures::future::BoxFuture<'_, _> =
                    Box::pin(manager.clear_with_gate(
                        &session_id_owned,
                        clear.prompt,
                        clear.gate,
                        &event_tx_clone,
                    ));
                if let Err(error) = clear_request.await {
                    warn!(session_id = %session_id_owned, error = %error, "Deferred context clear failed");
                }
            } else if let Some(follow_up) = slot.take_follow_up().filter(|_| !awaited) {
                manager
                    .start_follow_up_turn(
                        session_id_owned,
                        follow_up,
                        event_tx_clone,
                        is_interactive,
                        permission_override,
                    )
                    .await;
            }
        });

        if let Some(mut state) = self.request_state.get_mut(session_id) {
            state.task_handle = Some(task);
        }

        Ok(message_id)
    }

    /// Re-run the attach-time trust gate over a session that just came off
    /// disk.
    ///
    /// The create-time gate runs once, against the kilns the session was
    /// created with. Names make that insufficient: a session can hold a name
    /// that resolved to nothing when it was created — so it was never
    /// classified, because it was not a kiln — and that resolves for the first
    /// time now, because the user registered the entry in between. Nothing else
    /// would ever gate it: `session.create` is long past, and the attach
    /// handlers never ran for a name that was already in the file.
    ///
    /// Resolution and this gate therefore travel together, both on load. A
    /// refusal fails the turn rather than detaching the kiln, for the reason
    /// `refuse_untrusted_for_attached_kilns` gives: only the user can weigh dropping a
    /// corpus they are mid-conversation with.
    fn refuse_untrusted_on_revive(
        &self,
        session: &crucible_core::session::Session,
    ) -> Result<(), AgentError> {
        let Some(agent) = session.agent.as_ref() else {
            return Ok(());
        };
        self.refuse_untrusted_for_attached_kilns(session, agent)
    }

    /// Return the live session for `session_id`, reviving it from storage if it
    /// is no longer resident in memory (ended or evicted), and resuming it if
    /// it is paused. This is what makes ended and paused sessions
    /// transparently resumable on send. Every revive runs the start checks. Storage is one flat root,
    /// so reviving needs nothing but the id — no session→kiln index, no probing
    /// open kilns for the one that happens to hold it.
    async fn get_or_revive_session(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::session::Session, AgentError> {
        // Reviving reads `{sessions_root}/{id}` off disk, so the id has to be
        // one the daemon could have filed. An id that will not parse is the
        // caller's bug, not a missing session — reported as such so the answer
        // names the parameter instead of sending them looking for a session.
        let validated = crucible_core::session::SessionId::parse(session_id)
            .map_err(|e| AgentError::InvalidSessionId(e.to_string()))?;

        let revived = if let Some(session) = self.session_manager.get_session(session_id) {
            // Resident but ended: reached because `end_session` keeps the session
            // in memory (see its comment — evicting there lost in-flight events).
            // Route it through the same always-resumable path as a non-resident
            // one, so sending to an ended session flips it back to Active instead
            // of streaming a turn into something `session.list` calls finished.
            match session.state {
                crucible_core::session::SessionState::Ended => {
                    self.session_manager
                        .resume_session_from_storage(&validated)
                        .await?
                }
                // Paused: the pause ran the end hooks, which released the
                // isolation claim. Taken as it is, the turn would run with no
                // claim, and its tools on the host. So a send resumes it, and
                // the start checks below run, as `session.resume` runs them.
                crucible_core::session::SessionState::Paused => {
                    self.session_manager.resume_session(session_id).await?;
                    self.session_manager
                        .get_session(session_id)
                        .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?
                }
                crucible_core::session::SessionState::Active
                | crucible_core::session::SessionState::Compacting => return Ok(session),
            }
        } else {
            match self
                .session_manager
                .resume_session_from_storage(&validated)
                .await
            {
                Ok(session) => session,
                Err(SessionError::NotFound(_)) => {
                    return Err(AgentError::SessionNotFound(session_id.to_string()))
                }
                Err(e) => return Err(e.into()),
            }
        };
        self.refuse_untrusted_on_revive(&revived)?;

        // A revived session owes the start checks a created one passed. The
        // isolation registry is memory: after a restart, or after an ended
        // session released its claim, only the start hooks claim it again.
        // Without them an ACP agent would start on the host, not in its sandbox.
        match self.delegation_service.session_lifecycle() {
            Some(lifecycle) => lifecycle
                .enforce_session_start(session_id)
                .await
                .map_err(|e| AgentError::SessionRefused(e.to_string()))?,
            // No lifecycle means no plugin runtime, so no plugin can claim
            // isolation. A session that asked for it cannot be live.
            None => {
                if let Some(requirement) = crate::session_lifecycle::required_isolation(&revived) {
                    return Err(AgentError::SessionRefused(
                        crate::session_lifecycle::unclaimed_isolation_reason(&requirement),
                    ));
                }
            }
        }
        info!(session_id = %session_id, "Revived a session on send");

        // Start hooks can change the session, so answer with what they left.
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    /// The session's cached agent handle, building one if there is none.
    ///
    /// # Why the install is generation-checked
    ///
    /// The request slot does **not** make this mutually exclusive with
    /// `switch_model`, and reading `models.rs`'s `ConcurrentRequest` check as if
    /// it does is the mistake this comment exists to prevent. That check
    /// (`models.rs`, `request_state.contains_key`) only *reads* the slot — it
    /// never claims it — and a non-claiming check cannot protect a
    /// check-then-act sequence whose two halves straddle an `await`.
    ///
    /// The reachable interleaving, in order:
    ///
    /// 1. `switch_model` reads the slot, finds it free (no turn yet), proceeds.
    /// 2. A turn claims the slot (`send_message_inner`, before this call) and
    ///    this function misses the cache and starts the slow build.
    /// 3. `switch_model` finishes its `modify_session().await` — a storage
    ///    write, and the whole window — persists model B, then invalidates the
    ///    agent cache, which is still empty, so it removes **nothing**. It
    ///    reports success.
    /// 4. The build completes and installs the handle it built for model **A**.
    ///
    /// Storage now says B and the live handle answers as A, for that turn and
    /// every turn after it, because nothing invalidates it again. That is
    /// exactly the `CLAUDE.md` pitfall about `get_*` not returning what `set_*`
    /// stored, and `session.get_model` reads storage, so the two disagree
    /// silently.
    ///
    /// Note the ordering: the *plan's* interleaving (a switch landing inside a
    /// build that already holds the slot) is unreachable — that switch gets
    /// `ConcurrentRequest` at step 1. The generation counter closes the version
    /// above, and step 4 is where it acts.
    ///
    /// Making `switch_model` claim the slot instead would turn a deferrable
    /// mid-turn switch into an error; `pending_mode` is the in-repo precedent
    /// for preferring deferral over refusal.
    async fn get_or_create_agent(
        &self,
        session_id: &str,
        agent_config: &SessionAgent,
        workspace: &std::path::Path,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<Arc<Mutex<BoxedAgentHandle>>, AgentError> {
        // Check the cache, and note the generation the build below has to
        // install against. Anything that invalidates while we are awaiting
        // moves it, and this build's result is then stale by definition.
        let slot = self.slot(session_id);
        let generation = match slot.agent_or_generation() {
            crate::agent_manager::slot::CachedAgent::Hit(cached) => {
                debug!(session_id = %session_id, "Using cached agent");
                return Ok(cached);
            }
            crate::agent_manager::slot::CachedAgent::Miss { generation } => generation,
        };

        // Build the agent handle from configuration (or the test-support
        // factory override, which lets tests script delegation children
        // whose session ids don't exist before spawn time).
        let mut agent = match self.agent_factory_override() {
            Some(factory) => factory(agent_config, workspace)
                .await
                .map_err(AgentFactoryError::AgentBuild)?,
            None => {
                self.build_agent_from_config(session_id, agent_config, workspace, event_tx)
                    .await?
                    .0
            }
        };

        // Persist the ACP agent's own session id. The next handle build —
        // after an eviction or a daemon restart — reads it back and sends
        // `session/resume`, so the agent keeps its history. Internal agents
        // answer `None` and skip this. A changed id (a resume that fell
        // back to `session/new`) overwrites the stale one.
        if let Some(acp_id) = agent.acp_session_id() {
            if let Err(e) = self.persist_acp_session_id(session_id, acp_id).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "The ACP session id was not persisted; resume will start fresh"
                );
            }
        }

        // Re-apply the persisted session mode: a mode set before the first
        // message (or after a handle eviction) must still shape this handle's
        // behavior (plan mode filters write tools). Best-effort — an agent
        // that rejects the mode falls back to its default rather than
        // failing the whole send.
        if let Some(mode_id) = agent_config.mode.as_deref() {
            if let Err(e) = agent.set_mode_str(mode_id).await {
                tracing::warn!(
                    session_id = %session_id,
                    mode = %mode_id,
                    error = %e,
                    "Persisted session mode not applied to new agent handle"
                );
            }
        }

        // Re-apply the persisted model of an ACP session. A resume that fell
        // back to `session/new` starts on the default model of the agent,
        // while the session still shows the stored one. When the agent does
        // not offer the stored model, the stored value takes the model of the
        // agent, so that the session does not show a model that nothing runs.
        if agent_config.agent_type == "acp" {
            if let Some(current) = agent.current_model().map(str::to_string) {
                if current != agent_config.model {
                    let offered = agent.fetch_available_models().await;
                    let applied = offered.contains(&agent_config.model)
                        && agent.switch_model(&agent_config.model).await.is_ok();
                    if !applied {
                        let stored = self.session_manager.modify_session(session_id, |live| {
                            match live.agent.as_mut() {
                                Some(stored_agent) if stored_agent.model != current => {
                                    stored_agent.model = current;
                                    true
                                }
                                _ => false,
                            }
                        });
                        if let Err(e) = stored.await {
                            tracing::warn!(session_id = %session_id, error = %e,
                                "The model of the ACP agent was not persisted");
                        }
                    }
                }
            }
        }

        // What the agent declared about itself, read once here while nothing
        // holds the handle. An ACP agent sends this in the `session/new`
        // reply, so this is the first moment it exists; an internal agent
        // declares nothing and the session keeps Crucible's own settings.
        let surface = crate::agent_manager::slot::AgentSurface {
            modes: agent.get_modes().cloned(),
            config_options: agent.agent_config_options().to_vec(),
        };
        let agent_modes = surface.modes.clone();

        // Tell the clients when the agent's own mode set replaces the one
        // they are showing. Knob reads bring the handle up themselves, so a
        // front end's dropdown usually answers from this surface directly —
        // but a list fetched before the handle existed (or from an older
        // daemon) is corrected the same way: `mode_changed` is the signal
        // both front ends already act on, and an id they do not recognise
        // makes them re-fetch the list.
        if let Some(current) = agent_modes.as_ref().map(|m| m.current_mode_id.0.as_ref()) {
            if agent_config.mode.as_deref() != Some(current) {
                emit_event(
                    event_tx,
                    SessionEventMessage::mode_changed(session_id, current),
                );
            }
        }

        // Cache and return. Install only if nothing invalidated us while we
        // were building — step 4 of the interleaving on this function. Losing
        // the race is not an error: this turn keeps the handle it just built
        // (valid for the config it read) and simply goes uncached, so the next
        // turn rebuilds from the new config.
        let agent = Arc::new(Mutex::new(agent));
        if !slot.install_agent(generation, &agent, surface) {
            debug!(
                session_id = %session_id,
                "session config changed during agent build; serving this turn uncached"
            );
        }

        Ok(agent)
    }

    /// Store the id the ACP agent gave this session, so the next handle build
    /// can send `session/resume`. A no-op when the stored id is the same.
    pub(crate) async fn persist_acp_session_id(
        &self,
        session_id: &str,
        acp_id: String,
    ) -> Result<(), SessionError> {
        self.session_manager
            .modify_session(session_id, |live| {
                if live.acp_session_id.as_deref() == Some(acp_id.as_str()) {
                    return false;
                }
                live.acp_session_id = Some(acp_id);
                true
            })
            .await?;
        Ok(())
    }

    /// Bring the session's agent handle up without sending a message.
    ///
    /// Everything a front end draws about an ACP session — its modes, its
    /// model selector, its knob support — is answered from the surface the
    /// handshake fills, and the handshake is also the resume: the connect
    /// flow sends `session/resume` with the id a previous handle persisted.
    /// The handle used to come up only with the first message, so a resumed
    /// session's dropdowns answered from Crucible's fallbacks (the Lua mode
    /// set, the configured providers) until the user sent something. Knob
    /// reads and writes ensure instead; a cached handle makes it a no-op.
    ///
    /// A session with no agent configured answers `Ok` — there is nothing to
    /// bring up, and the caller's own read produces the accurate error. Only
    /// a failed BUILD is an error.
    pub(crate) async fn ensure_agent_handle(
        &self,
        session_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.slot(session_id).cached_agent().is_some() {
            return Ok(());
        }
        let Ok((session, agent_config)) = self.get_session_with_agent(session_id) else {
            return Ok(());
        };
        if agent_config.agent_type != "acp" {
            // Internal surfaces (the Lua mode registry, the provider model
            // catalogue) are live without a handle.
            return Ok(());
        }

        // Announcements ride the caller's channel when it has one; a caller
        // without one drops them rather than inventing a bus.
        let (detached_tx, _rx) = broadcast::channel(16);
        let event_tx = event_tx.unwrap_or(&detached_tx);

        let tool_root = crate::agent_manager::scope::session_tool_root(
            &session,
            self.session_manager.sessions_root(),
        );
        self.get_or_create_agent(session_id, &agent_config, &tool_root, event_tx)
            .await
            .map(|_| ())
    }

    /// Create the appropriate agent handle from session configuration.
    ///
    /// Resolves provider endpoint, acquires knowledge repository and embedding
    /// provider from the session's kiln, builds ACP permission handler if needed,
    /// and creates the agent handle via the agent factory.
    async fn build_agent_from_config(
        &self,
        session_id: &str,
        agent_config: &SessionAgent,
        workspace: &std::path::Path,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<(BoxedAgentHandle, SessionAgent), AgentError> {
        let mut resolved_config = if agent_config.endpoint.is_none() {
            let provider_key = agent_config
                .provider_key
                .as_deref()
                .unwrap_or_else(|| agent_config.provider.as_str());
            if let Some(provider) = self.resolve_provider_config(provider_key) {
                let mut config = agent_config.clone();
                config.endpoint = provider.endpoint;
                debug!(
                    provider_key = %provider_key,
                    endpoint = ?config.endpoint,
                    "Resolved endpoint from llm config"
                );
                config
            } else {
                agent_config.clone()
            }
        } else {
            agent_config.clone()
        };

        // Inject tool spilling context into system prompt (once, at agent creation)
        if !resolved_config.system_prompt.is_empty() {
            resolved_config.system_prompt.push_str(
                "\n\nLarge tool outputs are saved to $CRU_SESSION_DIR/tools/. Use this path in shell commands to access full content.",
            );
        }

        info!(
            session_id = %session_id,
            provider = %resolved_config.provider,
            model = %resolved_config.model,
            endpoint = ?resolved_config.endpoint,
            "Creating new agent"
        );

        let acp_permission_handler = if resolved_config.agent_type == "acp" {
            Some(self.build_acp_permissions(
                session_id,
                event_tx,
                workspace,
                resolved_config.tool_policy.clone(),
            ))
        } else {
            None
        };

        let session_for_factory = self.session_manager.get_session(session_id);
        let session_kilns = session_for_factory
            .as_ref()
            .map(|s| s.kilns.clone())
            .unwrap_or_default();
        // The name the session carries becomes a directory here and nowhere
        // else. A name with no registry entry resolves to nothing, and the
        // session then builds with no knowledge repository at all rather than
        // with one rooted somewhere it was never granted.
        let kiln_path = session_for_factory
            .as_ref()
            .and_then(|s| s.default_kiln())
            .and_then(|name| self.session_manager.kiln_registry().resolve(name).path());
        let kiln_path = kiln_path.as_deref();
        let mut knowledge_repo = None;
        let mut embedding_provider = None;

        if let Some(kiln_path) = kiln_path {
            let storage = self
                .kiln_manager
                .get_or_open(kiln_path)
                .await
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            knowledge_repo = Some(storage.as_knowledge_repository());

            if self.kiln_manager.enrichment_config().is_some() {
                embedding_provider = Some(
                    self.kiln_manager
                        .embedding_provider()
                        .await
                        .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?,
                );
            }
        }

        // Both off `OnceLock`s bound at daemon startup, NOT off the loader
        // mutex: `fire_session_start` holds that across plugin hook execution
        // (container builds included), so taking it here made a cold-start turn
        // queue behind an unrelated session's slow start. Each accessor falls
        // back to the loader only for managers the server never wired.
        let lua_handle: Option<mlua::Lua> = self.plugin_lua().await;
        let plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>> =
            self.plugin_registry().await;

        let agent = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            modes: Some(self.modes.clone()),
            agent_config: &resolved_config,
            lua: lua_handle.as_ref(),
            workspace,
            kiln_path,
            session_kilns: &session_kilns,
            parent_session_id: Some(session_id),
            background_spawner: Some(self.background_manager.clone()),
            delegation_spawner: Some(self.delegation_service.clone()),
            card_roots: &self.card_roots,
            mcp_gateway: self.mcp_gateway.clone(),
            acp_permission_handler,
            acp_config: self.acp_config.as_ref(),
            context_config: self.context_config.as_ref(),
            knowledge_repo,
            embedding_provider,
            plugin_tools,
            // The handle re-reads this on every request, so a plugin's
            // `cru.tools.set_active` narrows the next request rather than
            // waiting for the agent cache to rebuild the handle.
            active_tools: Some(self.active_tools()),
            // Read at agent-construction time rather than session start: the
            // claim is made by a `required` start hook, which has already run
            // by now, and reading it here means the agent is relocated by
            // whatever plugin actually claimed the session.
            sandbox_exec: self
                .isolation()
                .and_then(|reg| reg.sandbox_exec(session_id)),
            // For an ACP agent, this is the containment of the kiln tools it
            // reaches over MCP. A session that has gone missing gets the empty
            // allowlist, which reaches nothing — the fail-closed reading, and
            // the same one `session_containment` gives a kiln-less session.
            containment: session_for_factory
                .as_ref()
                .map(|session| {
                    crate::agent_manager::scope::session_containment(
                        session,
                        self.session_manager.sessions_root(),
                        self.session_manager.kiln_registry(),
                    )
                })
                .unwrap_or_else(|| crate::tools::containment::RootSet::scoped(vec![], vec![])),
            // The id a previous handle persisted (see get_or_create_agent).
            // With it, the ACP connect flow resumes the agent session, so
            // the agent keeps its history across a daemon restart.
            resume_acp_session_id: session_for_factory
                .as_ref()
                .and_then(|session| session.acp_session_id.clone()),
            event_tx: Some(event_tx),
        })
        .await?;

        Ok((agent, resolved_config))
    }
}
