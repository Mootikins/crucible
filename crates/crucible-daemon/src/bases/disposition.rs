//! The daemon's existing session write mode and review ledger govern plugin edits.
use super::*;
use crate::{
    file_write::{write_locked, LockedChange},
    rpc::RpcContext,
};
use crucible_core::{
    file_write::ExpectedBase,
    session::{PhysicalRoot, Session},
    types::WriteMode,
};
use serde_json::{json, Value as Json};

#[derive(Default)]
pub(super) struct Writer<'a> {
    pub ctx: Option<&'a RpcContext>,
    pub session: Option<Session>,
}
impl Writer<'_> {
    fn writes(&self) -> Option<crate::tools::notes::NoteWrites> {
        let (Some(ctx), Some(s)) = (self.ctx, &self.session) else {
            return None;
        };
        let mode = if ctx.agents.turn_running(s.id.as_str()) {
            ctx.agents.slot(s.id.as_str()).write_mode().clone()
        } else {
            let mode = crate::tools::notes::TurnWriteMode::default();
            mode.set(
                ctx.agents.mode_writes(
                    s.agent
                        .as_ref()
                        .and_then(|a| a.mode.as_deref())
                        .unwrap_or("normal"),
                ),
            );
            mode
        };
        Some(crate::tools::notes::NoteWrites::new(
            mode,
            ctx.agents.proposals().clone(),
            s,
        ))
    }
    pub fn proposed_text(&self, root: &Path, path: &Path) -> Result<Option<String>> {
        let Some(writes) = self.writes().filter(|w| w.mode() == WriteMode::Propose) else {
            return Ok(None);
        };
        writes
            .proposed_text(
                &PhysicalRoot::from_top_level(root.to_path_buf()),
                &path.strip_prefix(root)?.to_string_lossy(),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    pub fn read_path(&self, root: &Path, path: &Path) -> Result<()> {
        if let (Some(ctx), Some(s)) = (self.ctx, &self.session) {
            super::plugin_api::scope(ctx, s, root)
                .resolve(&path.strip_prefix(root)?.to_string_lossy())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn before(
        &self,
        root: &Path,
        path: &Path,
        previous: &Path,
        content: Option<&str>,
    ) -> Result<Json> {
        let relative = path.strip_prefix(root)?.to_string_lossy().to_string();
        let session = self.session.as_ref().map(|s| s.id.as_str());
        let mut payload = json!({
            "path": relative,
            "previous_path": previous.strip_prefix(root)?.to_string_lossy(),
            "content": content,
        });
        if let Some(ctx) = self.ctx {
            payload["kiln"] = json!(ctx.kiln_registry.name_for(root));
            if let Some(s) = &self.session {
                ctx.agents
                    .bases_write_permission(s, &relative, content)
                    .await
                    .map_err(anyhow::Error::msg)?;
                let scope = super::plugin_api::scope(ctx, s, root);
                scope
                    .resolve_for_write(&relative)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
            if crucible_core::kiln::is_note_file(path) {
                if let Some(content) = content {
                    let old = match self.proposed_text(root, previous)? {
                        Some(text) => text,
                        None => match tokio::fs::read_to_string(previous).await {
                            Ok(text) => text,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                            Err(e) => return Err(e.into()),
                        },
                    };
                    let parser = crucible_core::parser::CrucibleParser::new();
                    for (key, text) in [("properties", content), ("old_properties", old.as_str())] {
                        let parsed = parser.parse_content(text, path).await?;
                        payload[key] = json!(parsed
                            .frontmatter
                            .map(|f| f.properties().clone())
                            .unwrap_or_default());
                    }
                }
            }
            super::policy::before(ctx.agents.plugin_handlers(), session, payload.clone()).await?;
        }
        Ok(payload)
    }
    pub async fn put(
        &self,
        root: &Path,
        path: &Path,
        content: String,
        base: ExpectedBase,
    ) -> Result<Json> {
        let _order = crate::file_write::lock(&root.join(".crucible-bases-writes")).await;
        let relative = path.strip_prefix(root)?.to_string_lossy().to_string();
        let payload = self.before(root, path, path, Some(&content)).await?;
        if let (Some(ctx), Some(s)) = (self.ctx, &self.session) {
            let writes = self.writes().expect("session write target");
            if writes.mode() == WriteMode::Propose {
                let proposal = writes
                    .propose(
                        PhysicalRoot::from_top_level(root.to_path_buf()),
                        &relative,
                        base,
                        content,
                    )
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                return Ok(
                    json!({"ok":true,"status":"proposed","proposal":proposal.id,"path":relative}),
                );
            }
            // The enclosing tool call owns attribution when its capture is active
            // in this future. Other concurrent calls must still open their own.
            if crate::agent_manager::messaging::review_capture::captures_session(s.id.as_str()) {
                let answer = write_locked(path, LockedChange::Put(content), base).await?;
                if answer["ok"] == true {
                    self.changed(payload);
                }
                return Ok(answer);
            }
            let roots = ctx
                .kiln_registry
                .paths_for(&s.kilns)
                .into_iter()
                .chain(s.workspace.iter().cloned())
                .collect::<Vec<_>>();
            ctx.agents
                .review
                .open_or_restore(
                    s.id.as_str(),
                    &s.storage_path(ctx.sessions.sessions_root()),
                    &roots,
                )
                .await?;
            let bracket = ctx.agents.review.open_bracket(s.id.as_str()).await?;
            let answer = write_locked(path, LockedChange::Put(content), base).await;
            ctx.agents
                .review
                .close(
                    s.id.as_str(),
                    bracket,
                    &format!("bases-{}", uuid::Uuid::new_v4()),
                    0,
                )
                .await?;
            crate::server::session::review::emit_review_changed(
                &ctx.event_tx,
                s.id.as_str(),
                "bases_write",
            );
            let answer = answer?;
            if answer["ok"] == true {
                self.changed(payload);
            }
            return Ok(answer);
        }
        let answer = write_locked(path, LockedChange::Put(content), base).await?;
        if answer["ok"] == true {
            self.changed(payload);
        }
        Ok(answer)
    }
    pub fn changed(&self, mut payload: Json) {
        if let Some(ctx) = self.ctx {
            payload.as_object_mut().map(|o| o.remove("content"));
            let path = payload["path"].as_str().unwrap_or_default().to_string();
            ctx.event_tx
                .emit(crucible_core::protocol::SessionEventMessage::typed(
                    crate::event_map::SYSTEM_SESSION,
                    crucible_core::protocol::session_events::SystemPayload::BaseChanged {
                        path,
                        change: payload,
                    },
                ));
        }
    }
}
