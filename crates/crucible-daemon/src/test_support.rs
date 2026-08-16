//! Canonical test mock implementations for daemon tests
//!
//! This module provides shared mock implementations for common traits used across
//! daemon tests. These mocks are simple stubs that return default/empty values,
//! suitable for testing code that depends on these traits without needing a full
//! implementation.

use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::chat::{AgentHandle, ChatResult};
use crucible_core::traits::KnowledgeRepository;
use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use std::time::Duration;

/// Canonical mock implementation of KnowledgeRepository for testing
///
/// Returns empty/default values for all methods. Use this in tests that need
/// a KnowledgeRepository but don't care about the actual data.
pub struct MockKnowledgeRepository;

#[async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
        _limit: usize,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

/// Canonical mock implementation of EmbeddingProvider for testing
///
/// Returns mock embeddings (384-dimensional vectors of 0.1) for all inputs.
/// Use this in tests that need an EmbeddingProvider but don't care about
/// actual embedding quality.
pub struct MockEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; _texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }
}

/// Scripted behaviors for [`MockSubagentHandle`]. Covers the success, delay,
/// failure, pending, and turn-cap scenarios exercised by delegation and
/// background-job tests.
#[derive(Clone)]
pub enum MockSubagentBehavior {
    ImmediateSuccess(String),
    DelayedSuccess {
        output: String,
        delay: Duration,
    },
    DelayedFailure {
        error: String,
        delay: Duration,
    },
    Pending,
    StreamFailure(String),
    /// Emits `marker` text plus a tool call every turn, so the execution loop
    /// only terminates when it hits `max_turns`. Used to verify turn caps.
    RepeatingToolCall(String),
}

/// Canonical mock agent handle driven by a [`MockSubagentBehavior`] script.
pub struct MockSubagentHandle {
    behavior: MockSubagentBehavior,
}

impl MockSubagentHandle {
    pub fn new(behavior: MockSubagentBehavior) -> Self {
        Self { behavior }
    }
}

#[async_trait]
impl crucible_core::turn::Agent for MockSubagentHandle {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        _ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        let behavior = self.behavior.clone();
        let body = async_stream::stream! {
            match behavior {
                MockSubagentBehavior::ImmediateSuccess(output) => {
                    yield TurnEvent::TextDelta(output);
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                }
                MockSubagentBehavior::DelayedSuccess { output, delay } => {
                    tokio::time::sleep(delay).await;
                    yield TurnEvent::TextDelta(output);
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                }
                MockSubagentBehavior::DelayedFailure { error, delay } => {
                    tokio::time::sleep(delay).await;
                    yield TurnEvent::Error(TurnError::Internal(error));
                }
                MockSubagentBehavior::Pending => {
                    futures::future::pending::<()>().await;
                }
                MockSubagentBehavior::StreamFailure(message) => {
                    yield TurnEvent::Error(TurnError::Internal(message));
                }
                MockSubagentBehavior::RepeatingToolCall(marker) => {
                    yield TurnEvent::TextDelta(marker);
                    yield TurnEvent::ToolCall {
                        id: "call-1".to_string(),
                        name: "noop".to_string(),
                        args: serde_json::Value::Null,
                        diffs: Vec::new(),
                    };
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                }
            }
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

#[async_trait]
impl AgentHandle for MockSubagentHandle {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "normal"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Run one `git` command in `dir`, asserting it succeeded.
///
/// Shared because three test modules each grew their own copy with slightly
/// different failure messages and one of them silently discarded stderr.
pub async fn git(dir: &std::path::Path, args: &[&str]) {
    let out = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git repository at `dir` containing `files`, all committed.
///
/// The identity is set explicitly so a machine with no configured `user.email`
/// still runs the suite.
pub async fn init_repo(dir: &std::path::Path, files: &[(&str, &str)]) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]).await;
    git(dir, &["config", "user.email", "t@t"]).await;
    git(dir, &["config", "user.name", "t"]).await;
    for (name, contents) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }
    if !files.is_empty() {
        git(dir, &["add", "."]).await;
        git(dir, &["commit", "-q", "-m", "init"]).await;
    }
}

/// Session storage under a temp directory the storage itself owns.
///
/// Sessions are no longer filed inside a kiln, so a `SessionManager` has to be
/// told where to write; without an injected root a test falls back to the
/// developer's real `~/.crucible` — green on CI, destructive locally. The
/// `TempDir` is held here rather than handed back because most of the fixtures
/// that need one build their manager inline, inside a struct literal, and have
/// nowhere to park it.
pub struct TempSessionStorage {
    inner: crate::session_storage::FileSessionStorage,
    _root: tempfile::TempDir,
}

#[async_trait]
impl crate::session_storage::SessionStorage for TempSessionStorage {
    fn sessions_root(&self) -> &std::path::Path {
        self.inner.sessions_root()
    }

    async fn save(
        &self,
        session: &crucible_core::session::Session,
    ) -> Result<(), crate::session_manager::SessionError> {
        self.inner.save(session).await
    }

    async fn load(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::session::Session, crate::session_manager::SessionError> {
        self.inner.load(session_id).await
    }

    async fn list(
        &self,
    ) -> Result<Vec<crucible_core::session::SessionSummary>, crate::session_manager::SessionError>
    {
        self.inner.list().await
    }

    async fn append_event(
        &self,
        session: &crucible_core::session::Session,
        event: &str,
    ) -> Result<(), crate::session_manager::SessionError> {
        self.inner.append_event(session, event).await
    }

    async fn append_markdown(
        &self,
        session: &crucible_core::session::Session,
        role: &str,
        content: &str,
    ) -> Result<(), crate::session_manager::SessionError> {
        self.inner.append_markdown(session, role, content).await
    }

    async fn load_events(
        &self,
        session_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, crate::session_manager::SessionError> {
        self.inner.load_events(session_id, limit, offset).await
    }

    async fn count_events(
        &self,
        session_id: &str,
    ) -> Result<usize, crate::session_manager::SessionError> {
        self.inner.count_events(session_id).await
    }
}

/// Storage over a private, self-owned temp root. See [`TempSessionStorage`].
pub fn temp_session_storage() -> std::sync::Arc<TempSessionStorage> {
    use crate::session_storage::FileSessionStorage;
    let root = tempfile::TempDir::new().expect("temp dir for session storage");
    std::sync::Arc::new(TempSessionStorage {
        inner: FileSessionStorage::new(FileSessionStorage::root_for(root.path())),
        _root: root,
    })
}

/// A [`SessionManager`](crate::session_manager::SessionManager) over
/// [`temp_session_storage`], for the many tests that need a manager but never
/// assert on where its sessions landed.
pub fn temp_session_manager() -> std::sync::Arc<crate::session_manager::SessionManager> {
    std::sync::Arc::new(crate::session_manager::SessionManager::with_storage(
        temp_session_storage(),
    ))
}
