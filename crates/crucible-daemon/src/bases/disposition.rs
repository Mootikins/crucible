//! The daemon's existing session write mode and review ledger govern plugin edits.
use super::operation::{internal, PendingWrite, WriteOutcome};
use super::*;
use crate::{
    file_write::{write_locked, LockedChange},
    rpc::RpcContext,
    tools::{fs_scope::FsScope, notes::Disposition},
};
use crucible_core::{
    file_write::ExpectedBase,
    note_edit::disk_hash,
    proposal::ProposedWrite,
    session::{PhysicalRoot, Session},
};
use serde_json::{json, Value as Json};

/// The lock that serialises the Bases writes of one kiln, so that a policy
/// such as a WIP limit sees every earlier write. It is taken before any
/// per-path lock.
pub(super) const ORDER_LOCK: &str = ".crucible-bases-writes";

tokio::task_local! {
    /// Present while a `base:before_write` policy runs. The policy runs under
    /// the order lock, and that lock is not reentrant.
    static IN_POLICY: ();
}

/// Who writes: the daemon context, and the session a plugin writes for.
/// No session is a person at a client; no context is a test.
#[derive(Default)]
pub(super) struct Writer<'a> {
    pub ctx: Option<&'a RpcContext>,
    pub session: Option<Session>,
}

/// Where a set of Bases changes landed.
pub(super) enum Landed<T> {
    Proposed(crucible_core::proposal::Proposal),
    /// The session proposes its writes, and a proposal cannot hold these.
    Unproposable(String),
    Applied(T),
}

/// What admission decided about one write.
pub(super) enum Admission {
    /// Write it. The payload is what the policy saw, for `base:changed`.
    Admitted(Json),
    Refused(String),
}

impl Writer<'_> {
    /// Serialise the Bases writes of the kiln at `root`.
    ///
    /// A policy that wrote through Bases would wait for the order lock that
    /// its own caller holds, so such a write is refused instead.
    pub async fn serialize(&self, root: &Path) -> Result<tokio::sync::OwnedMutexGuard<()>> {
        ensure!(
            IN_POLICY.try_with(|()| ()).is_err(),
            "A base:before_write policy cannot write through Bases; return cancel or nil instead"
        );
        Ok(crate::file_write::lock(&root.join(ORDER_LOCK)).await)
    }
    fn writes(&self) -> Option<crate::tools::notes::NoteWrites> {
        let (Some(ctx), Some(s)) = (self.ctx, &self.session) else {
            return None;
        };
        Some(crate::tools::notes::NoteWrites::new(
            ctx.agents.write_mode_for(s),
            ctx.agents.proposals().clone(),
            s,
        ))
    }
    /// The session's proposal target when it proposes its writes now, from
    /// the decision that the note tools share.
    fn proposing(&self) -> Option<crate::tools::notes::NoteWrites> {
        let writes = self.writes();
        match crate::tools::notes::NoteWrites::disposition(writes.as_ref()) {
            Disposition::Propose(_) => writes,
            Disposition::Apply => None,
        }
    }

    /// Land `changes`, one set of whole-file writes and deletions in the
    /// kiln at `root`, in the session's disposition: as one proposal, or by
    /// running `apply` inside the session's review attribution.
    /// `changes` is `None` when a proposal cannot hold them.
    pub async fn dispose<T>(
        &self,
        root: &Path,
        changes: Option<Vec<ProposedWrite>>,
        apply: impl std::future::Future<Output = Result<T>>,
    ) -> Result<Landed<T>> {
        let writes = self.writes();
        match crate::tools::notes::NoteWrites::disposition(writes.as_ref()) {
            Disposition::Propose(writes) => {
                let Some(changes) = changes else {
                    return Ok(Landed::Unproposable(
                        "A proposal holds text; move this file when the session applies its writes"
                            .into(),
                    ));
                };
                debug_assert!(changes.iter().all(|c| c.root.as_path() == root));
                Ok(Landed::Proposed(writes.propose_all(changes)?))
            }
            Disposition::Apply => Ok(Landed::Applied(self.attributed(apply).await?)),
        }
    }
    /// The session's read scope in the kiln at `root`.
    pub fn scope(&self, root: &Path) -> Option<FsScope> {
        let (ctx, s) = (self.ctx?, self.session.as_ref()?);
        Some(super::plugin_api::scope(ctx, s, root))
    }
    pub fn proposed_text(&self, root: &Path, path: &Path) -> Result<Option<String>> {
        let Some(writes) = self.proposing() else {
            return Ok(None);
        };
        Ok(writes.proposed_text(
            &PhysicalRoot::from_top_level(root.to_path_buf()),
            &path.strip_prefix(root)?.to_string_lossy(),
        )?)
    }
    pub fn read_path(&self, root: &Path, path: &Path) -> Result<()> {
        if let Some(scope) = self.scope(root) {
            scope.resolve(&path.strip_prefix(root)?.to_string_lossy())?;
        }
        Ok(())
    }
    /// The pending proposed note writes in the kiln at `root`, from every
    /// session, with the properties each would leave.
    pub fn pending_writes(&self, root: &Path) -> Result<Vec<PendingWrite>> {
        let Some(ctx) = self.ctx else {
            return Ok(vec![]);
        };
        let root = PhysicalRoot::from_top_level(root.to_path_buf());
        let proposals = ctx.agents.proposals().list(false).map_err(internal)?;
        Ok(proposals
            .into_iter()
            .filter(|p| p.state.is_pending())
            .flat_map(|p| {
                let id = p.id.to_string();
                p.writes
                    .into_iter()
                    .filter(|w| {
                        w.root == root && crucible_core::kiln::is_note_file(Path::new(&w.path))
                    })
                    .map(move |w| PendingWrite {
                        properties: properties(&w.new_text),
                        path: w.path,
                        proposal: id.clone(),
                    })
            })
            .filter(|w| {
                self.read_path(root.as_path(), &root.as_path().join(&w.path))
                    .is_ok()
            })
            .collect())
    }

    /// Check the session's permission and write scope for `relative`, a file
    /// that a write changes. `Some(reason)` when a permission rule refuses it.
    pub async fn permit(&self, root: &Path, relative: &str) -> Result<Option<String>> {
        self.permit_content(root, relative, None).await
    }
    async fn permit_content(
        &self,
        root: &Path,
        relative: &str,
        content: Option<&str>,
    ) -> Result<Option<String>> {
        let (Some(ctx), Some(s)) = (self.ctx, &self.session) else {
            return Ok(None);
        };
        if let Err(reason) = ctx
            .agents
            .bases_write_permission(s, relative, content)
            .await
        {
            return Ok(Some(reason));
        }
        super::plugin_api::scope(ctx, s, root).resolve_for_write(relative)?;
        Ok(None)
    }

    /// Check permission, scope and policy for writing `content` to `path`,
    /// whose text was at `previous` before the write.
    pub async fn admit(
        &self,
        root: &Path,
        path: &Path,
        previous: &Path,
        content: Option<&str>,
    ) -> Result<Admission> {
        let relative = path.strip_prefix(root)?.to_string_lossy().to_string();
        let mut payload = json!({
            "path": relative,
            "previous_path": previous.strip_prefix(root)?.to_string_lossy(),
            "content": content,
        });
        let Some(ctx) = self.ctx else {
            return Ok(Admission::Admitted(payload));
        };
        payload["kiln"] = json!(ctx.kiln_registry.name_for(root));
        if let Some(reason) = self.permit_content(root, &relative, content).await? {
            return Ok(Admission::Refused(reason));
        }
        if let (true, Some(content)) = (crucible_core::kiln::is_note_file(path), content) {
            let old = match self.proposed_text(root, previous)? {
                Some(text) => text,
                None => match tokio::fs::read_to_string(previous).await {
                    Ok(text) => text,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(e) => return Err(e.into()),
                },
            };
            payload["properties"] = properties(content);
            payload["old_properties"] = properties(&old);
        }
        let session = self.session.as_ref().map(|s| s.id.as_str());
        let verdict = IN_POLICY
            .scope(
                (),
                super::policy::before(ctx.agents.plugin_handlers(), session, payload.clone()),
            )
            .await?;
        Ok(match verdict {
            Some(reason) => Admission::Refused(reason),
            None => Admission::Admitted(payload),
        })
    }

    /// Write `content` to `path` in the session's disposition. The caller
    /// holds [`Self::serialize`] and the lock of `path`.
    pub async fn put(
        &self,
        root: &Path,
        path: &Path,
        content: String,
        base: ExpectedBase,
    ) -> Result<WriteOutcome> {
        // The resolved form: a symlink cannot take the write out of the kiln.
        let path = crate::file_write::contain(path, root).map_err(|refused| {
            anyhow::anyhow!(
                "{}",
                refused["message"]
                    .as_str()
                    .unwrap_or("Path escapes the kiln")
            )
        })?;
        let relative = path.strip_prefix(root)?.to_string_lossy().to_string();
        let payload = match self.admit(root, &path, &path, Some(&content)).await? {
            Admission::Admitted(payload) => payload,
            Admission::Refused(reason) => {
                return Ok(WriteOutcome::Refused {
                    path: relative,
                    reason,
                })
            }
        };
        let change = ProposedWrite {
            root: PhysicalRoot::from_top_level(root.to_path_buf()),
            path: relative.clone(),
            base: base.clone(),
            new_text: content.clone(),
            remove: false,
            moved_from: None,
        };
        let apply = async { Ok(write_locked(&path, LockedChange::Put(content), base).await?) };
        let answer = match self.dispose(root, Some(vec![change]), apply).await? {
            Landed::Proposed(proposal) => {
                return Ok(WriteOutcome::Proposed {
                    path: relative,
                    proposal: proposal.id.to_string(),
                })
            }
            Landed::Applied(answer) => answer,
            Landed::Unproposable(reason) => anyhow::bail!(reason),
        };
        let outcome = match (&answer["ok"], answer["current_hash"].as_str()) {
            (Json::Bool(true), _) => WriteOutcome::Applied {
                path: relative,
                ancestor_hash: answer["content_hash"].as_str().unwrap_or_default().into(),
            },
            (_, Some(current)) => WriteOutcome::Stale {
                path: relative,
                current_hash: current.into(),
            },
            _ => anyhow::bail!(
                "{}",
                answer["message"].as_str().unwrap_or("The write failed")
            ),
        };
        if matches!(outcome, WriteOutcome::Applied { .. }) {
            self.changed(payload);
        }
        Ok(outcome)
    }

    /// Run `write` inside the session's review attribution. A write for no
    /// session, or one nested in a bracketed tool call, needs none of its own.
    pub async fn attributed<T>(&self, write: impl std::future::Future<Output = T>) -> T {
        let (Some(ctx), Some(s)) = (self.ctx, &self.session) else {
            return write.await;
        };
        let roots = ctx
            .kiln_registry
            .paths_for(&s.kilns)
            .into_iter()
            .chain(s.workspace.iter().cloned())
            .collect::<Vec<_>>();
        crate::agent_manager::messaging::review_capture::attribute_write(
            &ctx.agents.review,
            s.id.as_str(),
            &s.storage_path(ctx.sessions.sessions_root()),
            &roots,
            &ctx.event_tx,
            "bases_write",
            write,
        )
        .await
    }

    /// Announce a write that landed on disk.
    pub fn changed(&self, mut payload: Json) {
        if let Some(ctx) = self.ctx {
            payload.as_object_mut().map(|o| o.remove("content"));
            let path = payload["path"].as_str().unwrap_or_default().to_string();
            ctx.event_tx
                .emit(crate::event_map::base_changed(path, payload));
        }
    }
    /// The hash of `path` on disk, for an answer that wrote nothing.
    pub async fn current_hash(path: &Path) -> Result<String> {
        Ok(disk_hash(&tokio::fs::read_to_string(path).await?))
    }
}

/// The frontmatter properties of `text` as JSON; none when it has none or
/// they do not parse.
fn properties(text: &str) -> Json {
    crucible_core::note_frontmatter::frontmatter_mapping(text)
        .ok()
        .and_then(|m| serde_json::to_value(m).ok())
        .unwrap_or_else(|| json!({}))
}
