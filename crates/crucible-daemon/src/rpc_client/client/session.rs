//! Session RPC methods
//!
//! Methods for managing chat sessions, sending messages, and configuring agents.

use anyhow::{Context, Result};
use crucible_core::config::KilnName;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use crucible_core::session::{SessionDetail, SessionSummary};
use crucible_core::types::SendOutcome;
use std::path::Path;

use super::DaemonClient;

// --- Session RPC Response Types ---

impl DaemonClient {
    // =========================================================================
    // Session RPC Methods
    // =========================================================================

    /// Create a session.
    ///
    /// With `configure_agent`, the daemon also resolves and configures the
    /// agent of the session in the same call, and the reply carries the
    /// resolved `agent_model`. An unknown ACP profile then fails with
    /// `INVALID_PARAMS`, and no session is made.
    pub async fn session_create(&self, request: SessionCreateRequest) -> Result<SessionSummary> {
        self.typed_call(RpcMethod::SessionCreate, request).await
    }

    pub async fn session_list(
        &self,
        kiln: Option<&KilnName>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
    ) -> Result<SessionListReply> {
        self.session_list_with_children(
            kiln,
            workspace,
            session_type,
            state,
            include_archived,
            None,
        )
        .await
    }

    /// `session.list` with explicit control over delegated-child visibility
    /// (children are hidden unless `include_children` is `Some(true)`).
    pub async fn session_list_with_children(
        &self,
        kiln: Option<&KilnName>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
        include_children: Option<bool>,
    ) -> Result<SessionListReply> {
        self.typed_call(
            RpcMethod::SessionList,
            SessionListRequest {
                session_type: session_type.map(|t| t.to_string()),
                kilns: kiln.map(KilnName::to_string).into_iter().collect(),
                workspace: workspace.map(|ws| ws.to_string_lossy().to_string()),
                state: state.map(|s| s.to_string()),
                include_archived,
                include_children,
            },
        )
        .await
    }

    pub async fn session_get(&self, session_id: &str) -> Result<SessionDetail> {
        let reply = self
            .session_id_call(RpcMethod::SessionGet, session_id)
            .await?;
        Ok(serde_json::from_value(reply)?)
    }

    /// `session.status` — the status list of a session.
    ///
    /// Returned as raw JSON (`{"status": [StatusDisplayItem, …]}`) for the
    /// web route, which forwards it verbatim.
    pub async fn session_status(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call(RpcMethod::SessionStatus, session_id)
            .await
    }

    /// `session.status`, decoded into the items that the TUI draws.
    pub async fn session_status_items(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::StatusDisplayItem>> {
        decode_status_items(self.session_status(session_id).await?)
    }

    /// `session.list_notifications`: every notification the daemon delivers
    /// to one session.
    pub async fn session_list_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::Notification>> {
        let reply = self
            .session_id_call(RpcMethod::SessionListNotifications, session_id)
            .await?;
        Ok(serde_json::from_value(reply["notifications"].clone())?)
    }

    /// `session.dismiss_notification`: close one notification for one
    /// session. True when the daemon dropped it, or hid a shared one for
    /// this session; false when it does not reach the session.
    pub async fn session_dismiss_notification(
        &self,
        session_id: &str,
        notification_id: &str,
    ) -> Result<bool> {
        let reply: serde_json::Value = self
            .typed_call(
                RpcMethod::SessionDismissNotification,
                Scoped::new(
                    session_id.to_string(),
                    NotificationKey {
                        notification_id: notification_id.to_string(),
                    },
                ),
            )
            .await?;
        reply["success"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("session.dismiss_notification: no success in {reply}"))
    }

    pub async fn session_pause(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call(RpcMethod::SessionPause, session_id)
            .await
    }

    pub async fn session_resume(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call(RpcMethod::SessionResume, session_id)
            .await
    }

    pub async fn session_end(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call(RpcMethod::SessionEnd, session_id)
            .await
    }

    pub async fn session_delete(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionDelete,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    pub async fn session_archive(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionArchive,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    pub async fn session_unarchive(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionUnarchive,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    pub async fn session_replay(
        &self,
        recording_path: &Path,
        speed: f64,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionReplay,
            SessionReplayRequest {
                recording_path: recording_path.to_string_lossy().to_string(),
                speed,
            },
        )
        .await
    }

    pub async fn session_resume_from_storage(
        &self,
        session_id: &str,
        page: Page,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionResumeFromStorage,
            Scoped::new(session_id, page),
        )
        .await
    }

    /// One page of a session's stored events. Unlike
    /// [`Self::session_resume_from_storage`], the session stays as it is: an
    /// ended session stays ended, and no start hook runs.
    pub async fn session_history(&self, session_id: &str, page: Page) -> Result<serde_json::Value> {
        self.typed_call(RpcMethod::SessionHistory, Scoped::new(session_id, page))
            .await
    }

    pub async fn session_send_message(
        &self,
        session_id: &str,
        content: &str,
        is_interactive: bool,
    ) -> Result<SendOutcome> {
        self.session_send_message_with_permissions(session_id, content, is_interactive, None)
            .await
    }

    pub async fn session_send_message_with_permissions(
        &self,
        session_id: &str,
        content: &str,
        is_interactive: bool,
        permission_mode: Option<String>,
    ) -> Result<SendOutcome> {
        self.typed_call(
            RpcMethod::SessionSendMessage,
            Scoped::new(
                session_id.to_string(),
                MessageInput {
                    content: content.to_string(),
                    is_interactive,
                    permission_mode,
                    comments: Vec::new(),
                },
            ),
        )
        .await
    }

    /// Send a message with the stored review comments that it attaches.
    ///
    /// The daemon refuses the message when a comment is unknown or resolved.
    pub async fn session_send_message_with_comments(
        &self,
        session_id: &str,
        content: &str,
        comments: &[crucible_core::diff::CommentRef],
        is_interactive: bool,
    ) -> Result<SendOutcome> {
        self.typed_call(
            RpcMethod::SessionSendMessage,
            Scoped::new(
                session_id.to_string(),
                MessageInput {
                    content: content.to_string(),
                    is_interactive,
                    permission_mode: None,
                    comments: comments.to_vec(),
                },
            ),
        )
        .await
    }

    /// All pending interactions across sessions (`{pending: [{session_id,
    /// request_id, request}]}`) — polled by the web Inbox.
    pub async fn session_pending_interactions(&self) -> Result<serde_json::Value> {
        self.call(RpcMethod::SessionPendingInteractions, serde_json::json!({}))
            .await
    }

    pub async fn session_interaction_respond(
        &self,
        session_id: &str,
        request_id: &str,
        response: crucible_core::interaction::InteractionResponse,
    ) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionInteractionRespond,
            Scoped::new(
                session_id.to_string(),
                InteractionAnswer {
                    request_id: request_id.to_string(),
                    response: serde_json::to_value(response)?,
                },
            ),
        )
        .await
    }

    /// Clear the model context of the session, as the user. The transcript
    /// stays; the daemon sends `context_cleared`.
    pub async fn session_clear(&self, session_id: &str) -> Result<()> {
        self.call(
            RpcMethod::SessionClear,
            serde_json::json!({ "session_id": session_id }),
        )
        .await
        .map(drop)
    }

    pub async fn session_cancel(&self, session_id: &str) -> Result<bool> {
        let resp: SessionCancelResponse = self
            .typed_call(
                RpcMethod::SessionCancel,
                Scoped::session(session_id.to_string()),
            )
            .await?;

        Ok(resp.cancelled)
    }

    pub async fn session_set_title(&self, session_id: &str, title: &str) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetTitle,
            Scoped::new(
                session_id.to_string(),
                Title {
                    title: title.to_string(),
                },
            ),
        )
        .await
    }

    /// Generate a topic-based title for a session (idempotent — returns the
    /// existing title if one is already set).
    pub async fn session_generate_title(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionGenerateTitle,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// Search session transcripts within `kilns` — the caller's whole kiln set,
    /// not one member of it. Scope is kiln-set *overlap*, so a caller that
    /// sends a subset silently hides the sessions sharing the rest.
    pub async fn session_search(
        &self,
        query: &str,
        kilns: &[KilnName],
        limit: Option<usize>,
    ) -> Result<crucible_core::session::SessionSearchResponse> {
        self.typed_call(
            RpcMethod::SessionSearch,
            SessionSearchRequest {
                query: query.to_string(),
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                limit,
            },
        )
        .await
    }

    // =========================================================================
    // Session Observe RPC Methods
    // =========================================================================

    /// The persisted wire envelopes past a seq cursor (`session.events_after`),
    /// in order — the tail a reconnecting chat stream replays before its live
    /// forwarding begins. Envelopes carry the `seq` they were stamped with.
    pub async fn session_events_after(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<Vec<crucible_core::protocol::SessionEventMessage>> {
        self.typed_call(
            RpcMethod::SessionEventsAfter,
            Scoped::new(session_id.to_string(), EventCursor { after }),
        )
        .await
    }

    /// List persisted sessions within `kilns` — the caller's whole kiln set,
    /// not one member of it. Scope is kiln-set *overlap*, so a caller that
    /// sends a subset silently hides the sessions sharing the rest.
    pub async fn session_list_persisted(
        &self,
        kilns: &[KilnName],
        session_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionListPersisted,
            SessionListPersistedRequest {
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                session_type: session_type.map(|t| t.to_string()),
                limit,
            },
        )
        .await
    }

    /// Render a persisted session's events to markdown.
    pub async fn session_render_markdown(
        &self,
        session_id: &str,
        include_timestamps: Option<bool>,
        include_tokens: Option<bool>,
        include_tools: Option<bool>,
        max_content_length: Option<usize>,
    ) -> Result<String> {
        let resp: SessionRenderMarkdownResponse = self
            .typed_call(
                RpcMethod::SessionRenderMarkdown,
                Scoped::new(
                    session_id.to_string(),
                    MarkdownOptions {
                        include_timestamps,
                        include_tokens,
                        include_tools,
                        max_content_length,
                    },
                ),
            )
            .await?;
        Ok(resp.markdown)
    }

    /// Export a session to a markdown file.
    pub async fn session_export_to_file(
        &self,
        session_id: &str,
        output_path: Option<&Path>,
        include_timestamps: Option<bool>,
    ) -> Result<String> {
        let resp: SessionExportToFileResponse = self
            .typed_call(
                RpcMethod::SessionExportToFile,
                Scoped::new(
                    session_id.to_string(),
                    ExportOptions {
                        output_path: output_path.map(|p| p.to_string_lossy().to_string()),
                        include_timestamps,
                    },
                ),
            )
            .await?;
        Ok(resp.output_path)
    }

    /// Clean up old persisted sessions.
    ///
    /// `kilns` is the caller's whole kiln set; `all_kilns` sweeps every session
    /// on the machine and is refused unless set. One of the two has to say
    /// something — an empty `kilns` with `all_kilns: false` is an error, not a
    /// silent no-op, because this verb deletes.
    pub async fn session_cleanup(
        &self,
        kilns: &[KilnName],
        older_than_days: u64,
        dry_run: bool,
        all_kilns: bool,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionCleanup,
            SessionCleanupRequest {
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                older_than_days,
                dry_run,
                all_kilns,
            },
        )
        .await
    }
}

/// Decode a `session.status` reply.
///
/// An item that does not decode fails the whole read. A client then shows
/// the failure, where a list without that item would hide it: the item that
/// fails can be the one that says `stop`.
pub fn decode_status_items(
    reply: serde_json::Value,
) -> Result<Vec<crucible_core::types::StatusDisplayItem>> {
    let Some(list) = reply.get("status") else {
        anyhow::bail!("the session.status reply has no status list");
    };
    serde_json::from_value(list.clone())
        .context("the session.status reply has an item that this client cannot read")
}
