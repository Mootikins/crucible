use super::super::*;
use super::DEFAULT_MAX_TOOL_DEPTH;
use crucible_core::config::components::permissions::PermissionMode;

/// Map the stream driver's outcome to the completion-channel status pair.
fn outcome_to_status(outcome: StreamOutcome) -> (TurnStatus, Option<String>) {
    match outcome {
        StreamOutcome::Completed => (TurnStatus::Completed, None),
        StreamOutcome::Failed(reason) => (TurnStatus::Failed, Some(reason)),
    }
}

impl AgentManager {
    pub async fn send_message(
        &self,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<String, AgentError> {
        self.send_message_inner(
            session_id,
            content,
            event_tx,
            is_interactive,
            permission_override,
            None,
        )
        .await
    }

    /// Like [`send_message`], but also returns a completion channel resolved
    /// when the turn reaches ANY terminal state (completed, cancelled, timed
    /// out, failed). This is the only reliable way to await a turn: the event
    /// bus emits no terminal event on successful completion.
    pub async fn send_message_notified(
        &self,
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
                event_tx,
                is_interactive,
                permission_override,
                Some(completion_tx),
            )
            .await?;
        Ok((message_id, completion_rx))
    }

    async fn send_message_inner(
        &self,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
        completion_tx: Option<oneshot::Sender<TurnOutcome>>,
    ) -> Result<String, AgentError> {
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
                    started_at: Instant::now(),
                });
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
            .get_or_create_agent(
                session_id,
                &agent_config,
                &tool_root,
                &event_tx_clone,
                is_interactive,
                permission_override,
            )
            .await
        {
            Ok(agent) => agent,
            Err(e) => {
                self.request_state.remove(session_id);
                return Err(e);
            }
        };
        info!(target: "ttft", session_id = %session_id, stage = "agent_ready", elapsed_ms = ttft_start.elapsed().as_millis() as u64, "ttft");

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
        let conversation_tree = self
            .get_or_rebuild_session_tree(
                session_id,
                &session.jsonl_path(self.session_manager.sessions_root()),
            )
            .await;

        if !emit_event(
            event_tx,
            SessionEventMessage::user_message(session_id, &message_id, &original_content),
        ) {
            warn!(session_id = %session_id, "No subscribers for user_message event");
        }

        let snapshot_key_node = {
            let t = conversation_tree.lock().await;
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
            // benign — a workspace outside git has nothing to track, while an
            // unreadable journal is lost evidence. They are logged the same
            // way because the *distinction* is not carried by this error: an
            // unreadable journal is recorded on the session's `Integrity`
            // inside `open_or_restore`, which is what holds its writes rather
            // than letting them through unattributed.
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
            let parent = t.current();
            let _user_node = t.add_child_and_advance(
                parent,
                crucible_core::turn::NodeContent::User {
                    text: original_content.clone(),
                },
            );
            // After append+advance, undo_depth() is the count of User
            // nodes on the current path — 1 means this turn is the first.
            t.undo_depth() == 1
        };

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
        let attachment_message = crate::agent_manager::attachments::build_attachment_message(
            &tool_root,
            &original_content,
        );
        if attachment_message.is_some() {
            debug!(session_id = %session_id, "Attached @-mentioned file contents to the turn");
        }

        // Pass the user's content through to the stream loop unchanged;
        // the Precognition system block (if any) is staged on
        // StreamContext and prepended by apply_transform_context_handlers
        // when the seam fires. This is the migration from string-level
        // mutation (old enrich_with_precognition) to the message-array
        // seam — Lua transform_context handlers can now further mutate
        // the precognition message via the same seam.
        let content = original_content.clone();

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
            session_state: self.get_or_create_session_state(session_id),
            workspace_path: tool_root.clone(),
            session_dir: session.storage_path(self.session_manager.sessions_root()),
            agent_stream_config: {
                let (lua_validators, plugin_lua) = match self.lua_validators() {
                    Some((r, l)) => (Some(r), Some(l)),
                    None => (None, None),
                };
                AgentStreamConfig::from_session_agent(
                    &agent_config,
                    TurnEnvironment {
                        lua_validators,
                        plugin_lua,
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
            },
            tool_dispatcher: self.get_or_create_session_dispatcher(&session).await,
            permission_override,
            conversation_tree,
            slot: self.slot(session_id),
            session_manager: self.session_manager.clone(),
            precognition_message,
            attachment_message,
            session_mode,
            is_interactive,
            // Compile the global [permissions] config once per turn. Unlike
            // the ACP gate there is no per-agent profile config for internal
            // agents (cards carry tool_policy instead), so this is global-only.
            permission_engine: self.permission_config.as_ref().map(|config| {
                Arc::new(
                    crucible_core::config::components::permissions::PermissionEngine::new(Some(
                        config,
                    )),
                )
            }),
            context_attach: self.context_attach(),
        };

        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();
            let stream_config = stream_ctx.agent_stream_config.clone();
            // Use session-configured max_iterations, falling back to the default
            let max_tool_depth = stream_config
                .max_iterations
                .map(|n| n as usize)
                .unwrap_or(DEFAULT_MAX_TOOL_DEPTH);

            let stream_future = Self::execute_agent_stream(
                agent,
                content,
                stream_ctx.clone(),
                stream_config,
                &mut accumulated_response,
                false,
                0,
                max_tool_depth,
                0,
            );

            // Wrap in execution timeout if configured
            let timed_future = async {
                if let Some(timeout_secs) = stream_ctx.agent_stream_config.execution_timeout_secs {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(timeout_secs),
                        stream_future,
                    )
                    .await
                    {
                        Ok(outcome) => outcome_to_status(outcome),
                        Err(_) => {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                timeout_secs = timeout_secs,
                                "Execution timeout reached"
                            );
                            if !emit_event(
                                &stream_ctx.event_tx,
                                SessionEventMessage::ended(
                                    &stream_ctx.session_id,
                                    "error: execution timeout reached",
                                ),
                            ) {
                                warn!(
                                    session_id = %stream_ctx.session_id,
                                    "No subscribers for execution timeout ended event"
                                );
                            }
                            (
                                TurnStatus::TimedOut,
                                Some("execution timeout reached".to_string()),
                            )
                        }
                    }
                } else {
                    outcome_to_status(stream_future.await)
                }
            };

            let (status, error) = tokio::select! {
                _ = cancel_rx => {
                    debug!(session_id = %session_id_owned, "Request cancelled");
                    if !emit_event(
                        &event_tx_clone,
                        SessionEventMessage::ended(&session_id_owned, "cancelled"),
                    ) {
                        warn!(session_id = %session_id_owned, "No subscribers for cancelled event");
                    }
                    (TurnStatus::Cancelled, None)
                }
                outcome = timed_future => outcome,
            };

            // Single convergence point for ALL exit paths — this send must
            // happen before the request_state slot is released so an awaiter
            // observes the outcome strictly after the turn is over.
            if let Some(tx) = completion_tx {
                let _ = tx.send(TurnOutcome {
                    status,
                    final_text: std::mem::take(&mut accumulated_response),
                    error,
                });
            }

            request_state.remove(&session_id_owned);
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
    /// `refuse_untrusted_for_kilns` gives: only the user can weigh dropping a
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
    /// is no longer resident in memory (ended or evicted). This is what makes
    /// ended sessions transparently resumable on send. Storage is one flat root,
    /// so reviving needs nothing but the id — no session→kiln index, no probing
    /// open kilns for the one that happens to hold it.
    async fn get_or_revive_session(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::session::Session, AgentError> {
        // Reviving reads `{sessions_root}/{id}` off disk, so the id has to be
        // one the daemon could have filed. An unusable one names no session.
        let validated = crucible_core::session::SessionId::parse(session_id)
            .map_err(|e| AgentError::SessionNotFound(e.to_string()))?;

        if let Some(session) = self.session_manager.get_session(session_id) {
            // Resident but ended: reached because `end_session` keeps the session
            // in memory (see its comment — evicting there lost in-flight events).
            // Route it through the same always-resumable path as a non-resident
            // one, so sending to an ended session flips it back to Active instead
            // of streaming a turn into something `session.list` calls finished.
            if session.state != crucible_core::session::SessionState::Ended {
                return Ok(session);
            }
            let revived = self
                .session_manager
                .resume_session_from_storage(&validated)
                .await?;
            self.refuse_untrusted_on_revive(&revived)?;
            info!(session_id = %session_id, "Reactivated an ended session on send");
            return Ok(revived);
        }

        match self
            .session_manager
            .resume_session_from_storage(&validated)
            .await
        {
            Ok(session) => {
                self.refuse_untrusted_on_revive(&session)?;
                info!(session_id = %session_id, "Revived idle session from storage on send");
                Ok(session)
            }
            Err(SessionError::NotFound(_)) => {
                Err(AgentError::SessionNotFound(session_id.to_string()))
            }
            Err(e) => Err(e.into()),
        }
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
    /// 3. `switch_model` finishes its `update_session().await` — a storage
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
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
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
                self.build_agent_from_config(
                    session_id,
                    agent_config,
                    workspace,
                    event_tx,
                    is_interactive,
                    permission_override,
                )
                .await?
                .0
            }
        };

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

        // Cache and return. Install only if nothing invalidated us while we
        // were building — step 4 of the interleaving on this function. Losing
        // the race is not an error: this turn keeps the handle it just built
        // (valid for the config it read) and simply goes uncached, so the next
        // turn rebuilds from the new config.
        let agent = Arc::new(Mutex::new(agent));
        if !slot.install_agent(generation, &agent) {
            debug!(
                session_id = %session_id,
                "session config changed during agent build; serving this turn uncached"
            );
        }

        Ok(agent)
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
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
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

        let agent_permissions = resolved_config.agent_name.as_deref().and_then(|name| {
            self.acp_config.as_ref().and_then(|acp| {
                let available = crate::acp::discovery::default_agent_profiles();
                resolve_agent_profile(name, &acp.agents, &available).and_then(|p| p.permissions)
            })
        });

        let acp_permission_handler = if resolved_config.agent_type == "acp" {
            Some(self.build_acp_permission_handler(
                session_id,
                event_tx,
                is_interactive,
                permission_override,
                agent_permissions,
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
            .and_then(|name| {
                self.session_manager
                    .kiln_registry()
                    .resolve(name)
                    .path()
                    .map(std::path::Path::to_path_buf)
            });
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

            if let Some(config) = self.kiln_manager.enrichment_config().cloned() {
                embedding_provider = Some(
                    crate::embedding::get_or_create_embedding_provider(&config)
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
            mcp_gateway: self.mcp_gateway.clone(),
            acp_permission_handler,
            acp_config: self.acp_config.as_ref(),
            context_config: self.context_config.as_ref(),
            knowledge_repo,
            embedding_provider,
            plugin_tools,
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
        })
        .await?;

        Ok((agent, resolved_config))
    }
}
