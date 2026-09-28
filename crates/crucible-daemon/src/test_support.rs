//! Canonical test mock implementations for daemon tests
//!
//! This module provides shared mock implementations for common traits used across
//! daemon tests. These mocks are simple stubs that return default/empty values,
//! suitable for testing code that depends on these traits without needing a full
//! implementation.

use crate::agent_manager::AgentHandle;
use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::chat::ChatResult;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A literal session id for tests, through the same validation production uses.
///
/// Tests name sessions with fixed strings; this is how they get a
/// [`SessionId`](crucible_core::session::SessionId) without a second, laxer
/// constructor existing for their benefit.
pub fn sid(id: &str) -> crucible_core::session::SessionId {
    crucible_core::session::SessionId::parse(id).expect("a valid test session id")
}

/// The runtime roots of this repository, for a test boot.
///
/// A test binary runs from `target/debug/deps`, so the exe-relative roots do
/// not exist, and the machine roots fall through to an installed tree of
/// another build. A test gives these roots to the boot instead.
pub fn repo_runtime_roots() -> Vec<crucible_core::runtime_path::RuntimeEntry> {
    let root = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime"));
    assert!(
        root.join("defaults").join("init.luau").is_file(),
        "the repository runtime tree must hold the shipped defaults: {}",
        root.display()
    );
    crate::runtime_defaults::shipped_roots(&[root])
}

/// Canonical mock implementation of `KnowledgeRepository` for tests.
///
/// [`MockKnowledgeRepository::new`] returns empty values from every method.
/// [`MockKnowledgeRepository::with_results`] scripts what `search_vectors`
/// returns. [`MockKnowledgeRepository::failing`] makes `search_vectors` fail,
/// so a test can check how a caller treats one broken kiln.
#[derive(Default)]
pub struct MockKnowledgeRepository {
    results: Vec<crucible_core::types::SearchResult>,
    /// What `search_blocks` answers. Empty means "this kiln has no block
    /// rows", which is what an un-reindexed kiln looks like.
    block_results: Vec<crucible_core::types::SearchResult>,
    fail_search: bool,
    /// Every `limit` a `search_blocks` call asked for, in order.
    block_limits: std::sync::Mutex<Vec<usize>>,
    /// The stored rows `blocks_for_note` answers, filtered by `note_path`.
    note_blocks: Vec<crucible_core::storage::BlockRecord>,
}

impl MockKnowledgeRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script the block-granularity answer.
    pub fn with_block_results(mut self, results: Vec<crucible_core::types::SearchResult>) -> Self {
        self.block_results = results;
        self
    }

    /// Script the stored rows of every note `blocks_for_note` may be asked for.
    pub fn with_note_blocks(mut self, rows: Vec<crucible_core::storage::BlockRecord>) -> Self {
        self.note_blocks = rows;
        self
    }

    pub fn with_results(results: Vec<crucible_core::types::SearchResult>) -> Self {
        Self {
            results,
            ..Self::default()
        }
    }

    pub fn failing() -> Self {
        Self {
            fail_search: true,
            ..Self::default()
        }
    }

    /// The `limit` of every `search_blocks` call so far, in order.
    pub fn block_limits(&self) -> Vec<usize> {
        self.block_limits.lock().unwrap().clone()
    }
}

#[async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn search_blocks(
        &self,
        _vector: Vec<f32>,
        limit: usize,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        self.block_limits.lock().unwrap().push(limit);
        Ok(self.block_results.iter().take(limit).cloned().collect())
    }

    async fn blocks_for_note(
        &self,
        path: &str,
    ) -> crucible_core::Result<Vec<crucible_core::storage::BlockRecord>> {
        Ok(self
            .note_blocks
            .iter()
            .filter(|row| row.note_path == path)
            .cloned()
            .collect())
    }

    async fn list_note_records(
        &self,
    ) -> crucible_core::Result<Vec<crucible_core::storage::note_store::NoteRecord>> {
        Ok(Vec::new())
    }

    async fn links_for_note(
        &self,
        _path: &str,
    ) -> crucible_core::Result<crucible_core::traits::NoteLinks> {
        Ok(crucible_core::traits::NoteLinks::default())
    }

    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn get_note_by_path(
        &self,
        _path: &str,
    ) -> crucible_core::Result<Option<crucible_core::storage::note_store::NoteRecord>> {
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
        if self.fail_search {
            return Err(crucible_core::CrucibleError::DatabaseError(
                "mock failure".into(),
            ));
        }
        Ok(self.results.clone())
    }
}

/// Canonical mock implementation of `EmbeddingProvider` for tests.
///
/// Every embedding is a vector of `0.1` with [`dimensions`](Self::dimensions)
/// entries (384 by default). The mock counts `embed_batch` calls, so a test can
/// check how a caller splits its batches. [`with_failure_on_batch_call`]
/// (Self::with_failure_on_batch_call) makes one numbered call fail, so a test
/// can check that a caller reports a failure in the middle of a run.
pub struct MockEmbeddingProvider {
    dimensions: usize,
    embed_batch_calls: AtomicUsize,
    fail_on_batch_call: Option<usize>,
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEmbeddingProvider {
    pub fn new() -> Self {
        Self {
            dimensions: 384,
            embed_batch_calls: AtomicUsize::new(0),
            fail_on_batch_call: None,
        }
    }

    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            dimensions,
            ..Self::new()
        }
    }

    /// Make the `fail_on_batch_call`-th call to `embed_batch` fail. Calls are
    /// counted from one.
    pub fn with_failure_on_batch_call(fail_on_batch_call: usize) -> Self {
        Self {
            fail_on_batch_call: Some(fail_on_batch_call),
            ..Self::new()
        }
    }

    /// The number of `embed_batch` calls so far.
    pub fn batch_calls(&self) -> usize {
        self.embed_batch_calls.load(Ordering::SeqCst)
    }

    fn vector(&self) -> Vec<f32> {
        vec![0.1; self.dimensions]
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.vector())
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let call_idx = self.embed_batch_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_batch_call == Some(call_idx) {
            anyhow::bail!("forced embed_batch failure on call {call_idx}");
        }
        Ok(vec![self.vector(); texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn provider_kind(&self) -> &'static str {
        "mock"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
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
    /// never terminates on its own; the caller cancels it.
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
                        call: None,
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

crate::impl_unsupported_session_knobs!(MockSubagentHandle);

#[async_trait]
impl AgentHandle for MockSubagentHandle {
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Run one `git` command in `dir`, asserting it succeeded, and return its
/// stdout.
///
/// Shared because test modules each grew their own copy with slightly
/// different failure messages and one of them silently discarded stderr.
pub async fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = tokio::process::Command::from(crucible_core::git::command())
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
    String::from_utf8_lossy(&out.stdout).into_owned()
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

impl TempSessionStorage {
    /// The registry this storage resolves persisted kilns against, so a
    /// `SessionManager` built over it resolves names the same way.
    pub fn kiln_registry(&self) -> &std::sync::Arc<crate::kiln_registry::KilnRegistry> {
        self.inner.kiln_registry()
    }
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
        session_id: &crucible_core::session::SessionId,
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
        session_id: &crucible_core::session::SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, crate::session_manager::SessionError> {
        self.inner.load_events(session_id, limit, offset).await
    }

    async fn count_events(
        &self,
        session_id: &crucible_core::session::SessionId,
    ) -> Result<usize, crate::session_manager::SessionError> {
        self.inner.count_events(session_id).await
    }
}

/// Storage over a private, self-owned temp root. See [`TempSessionStorage`].
pub fn temp_session_storage() -> std::sync::Arc<TempSessionStorage> {
    temp_session_storage_with_kilns(&[])
}

/// [`temp_session_storage`], plus `extra` kiln entries pointing at directories
/// the test owns.
///
/// For the tests that need a kiln name to resolve to a *specific* directory —
/// one holding a `kiln.toml` classification, or the notes a search is expected
/// to find. Those directories must lie outside the storage's own data root, or
/// the registration floor refuses them for enclosing every transcript, which is
/// exactly what it is there for.
pub fn temp_session_storage_with_kilns(
    extra: &[(&str, &std::path::Path)],
) -> std::sync::Arc<TempSessionStorage> {
    use crate::session_storage::FileSessionStorage;
    let root = tempfile::TempDir::new().expect("temp dir for session storage");
    let owned: Vec<(&str, std::path::PathBuf)> = TEMP_KILNS
        .iter()
        .map(|name| (*name, root.path().join("kilns").join(name)))
        .collect();
    let mut kilns: Vec<(&str, &std::path::Path)> = extra.to_vec();
    // The caller's entries first, so a name they bound wins over the stock one.
    kilns.extend(
        owned
            .iter()
            .filter(|(name, _)| !extra.iter().any(|(n, _)| n == name))
            .map(|(name, path)| (*name, path.as_path())),
    );
    std::sync::Arc::new(TempSessionStorage {
        inner: FileSessionStorage::new(FileSessionStorage::root_for(root.path()))
            .with_registry(kiln_registry(root.path(), &kilns)),
        _root: root,
    })
}

/// The kiln names [`temp_session_storage`] registers.
///
/// A storage whose registry is empty resolves no persisted kiln, so a session
/// saved and reloaded through one comes back kiln-less. That is the correct
/// behaviour — an unresolvable path is not a kiln — and it is useless as a
/// fixture, because almost every test here wants "a kiln that exists" and cares
/// about something else entirely. These are ordinary directories under the same
/// temp root, registered through the real builder, so `kiln_name("kiln")`
/// survives a round trip without any test having to build a registry of its
/// own.
///
/// Anything NOT in this list is unregistered on purpose: that is how a test
/// spells "a name no entry claims".
pub const TEMP_KILNS: &[&str] = &[
    "kiln",
    "kiln1",
    "kiln2",
    "extra-kiln",
    "other-kiln",
    "notes",
    "reference",
    "vault",
    "mine",
    "theirs",
    "a",
    "b",
    "elsewhere",
    "workspace",
];

/// A [`SessionManager`](crate::session_manager::SessionManager) over
/// [`temp_session_storage`], for the many tests that need a manager but never
/// assert on where its sessions landed.
pub fn temp_session_manager() -> std::sync::Arc<crate::session_manager::SessionManager> {
    temp_session_manager_with_kilns(&[])
}

/// [`temp_session_manager`] over [`temp_session_storage_with_kilns`].
pub fn temp_session_manager_with_kilns(
    extra: &[(&str, &std::path::Path)],
) -> std::sync::Arc<crate::session_manager::SessionManager> {
    let storage = temp_session_storage_with_kilns(extra);
    let registry = storage.kiln_registry().clone();
    std::sync::Arc::new(
        crate::session_manager::SessionManager::with_storage(storage).with_kiln_registry(registry),
    )
}

/// A kiln name, straight through the validating parser production uses.
///
/// Tests name kilns with fixed strings; this is how they get a
/// [`KilnName`](crucible_core::config::KilnName) without a second, laxer
/// constructor existing for their benefit.
pub fn kiln_name(name: &str) -> crucible_core::config::KilnName {
    crucible_core::config::KilnName::parse(name).expect("a valid test kiln name")
}

/// A kiln registry over `kilns`, built the way the daemon builds its own.
///
/// Goes through [`KilnRegistry::from_app_config`] rather than reaching into the
/// map, so a test's registry has been through the same floor, the same `~`
/// expansion and the same collision rules as the real one — a fixture that
/// skipped them would prove nothing about the code under test.
///
/// `data_home` is both the anchor for relative paths and the data root the
/// floor refuses, so a test that names a kiln inside its own temp data root
/// gets the production refusal rather than a fixture-only permit.
pub fn kiln_registry(
    data_home: &std::path::Path,
    kilns: &[(&str, &std::path::Path)],
) -> std::sync::Arc<crate::kiln_registry::KilnRegistry> {
    let lazy: Vec<(&str, &std::path::Path, bool)> = kilns
        .iter()
        .map(|(name, path)| (*name, *path, false))
        .collect();
    kiln_registry_with_lazy(data_home, &lazy)
}

/// As [`kiln_registry`], with each entry's `lazy` flag.
///
/// The table form is what carries `lazy`, so a test that needs one has to
/// write the entry as a table rather than the string shorthand — same as a
/// user would.
pub fn kiln_registry_with_lazy(
    data_home: &std::path::Path,
    kilns: &[(&str, &std::path::Path, bool)],
) -> std::sync::Arc<crate::kiln_registry::KilnRegistry> {
    let entries: serde_json::Map<String, serde_json::Value> = kilns
        .iter()
        .map(|(name, path, lazy)| {
            let value = if *lazy {
                serde_json::json!({ "path": path.to_string_lossy(), "lazy": true })
            } else {
                serde_json::Value::String(path.to_string_lossy().into_owned())
            };
            ((*name).to_string(), value)
        })
        .collect();
    std::sync::Arc::new(
        crate::kiln_registry::KilnRegistry::from_app_config(
            crate::kiln_registry::KilnRegistryContext::new(
                data_home.to_path_buf(),
                None,
                data_home.to_path_buf(),
            ),
            Some(&serde_json::Value::Object(
                [("kilns".to_string(), serde_json::Value::Object(entries))]
                    .into_iter()
                    .collect(),
            )),
        )
        .expect("a test kiln registry with no name collisions"),
    )
}

/// A [`SessionManager`](crate::session_manager::SessionManager) rooted under
/// `data_home` whose registry — and whose storage layer's registry — know
/// `kilns`.
///
/// Both halves come from one registry for the reason the daemon's own
/// composition root gives: two would be two answers to "which directory is
/// `notes`", and the load path and the scope path would disagree.
pub fn session_manager_with_kilns(
    data_home: &std::path::Path,
    kilns: &[(&str, &std::path::Path)],
) -> crate::session_manager::SessionManager {
    let registry = kiln_registry(data_home, kilns);
    let storage = crate::session_storage::FileSessionStorage::new(
        crate::session_storage::FileSessionStorage::root_for(data_home),
    )
    .with_registry(registry.clone());
    crate::session_manager::SessionManager::with_storage(std::sync::Arc::new(storage))
        .with_kiln_registry(registry)
}

/// A unique directory path for a test manager's plain review snapshots.
///
/// [`crate::agent_manager::AgentManagerParams::review_snapshot_root`] is
/// required rather than defaulted, so that no test writes review snapshots into
/// the developer's real data home. Most manager fixtures never capture a root
/// outside git, so all they need is a path that is unique and theirs: the
/// directory is created and removed again here, and nothing recreates it unless
/// a capture actually runs.
///
/// A test that *does* capture a plain root passes the path of a `TempDir` it
/// holds open instead.
pub fn scratch_snapshot_root() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("a temporary directory for review snapshots")
        .path()
        .join("review-snapshots")
}

/// The recordings and stored logs that the transcript readers are pinned
/// against, with golden files in `assets/fixtures/golden/`.
pub const READER_FIXTURES: [&str; 7] = [
    "session_log_wire.jsonl",
    "old_wire_session.jsonl",
    "mixed_view_session.jsonl",
    "acp_parity_internal.jsonl",
    "acp_parity_delegated.jsonl",
    "reproduce.jsonl",
    "delegation-demo.jsonl",
];

/// The path of `name` in `assets/fixtures/`.
pub fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures")
        .join(name)
}

/// The `session.jsonl` that the daemon keeps for the fixture `name`.
///
/// A stored log stays as it is. A recording (a header line, then
/// `{ts, seq, event, data}` lines) becomes the wire lines of the events that
/// the daemon persists, with the time of each event.
pub fn stored_log(name: &str) -> String {
    use crucible_core::protocol::SessionEventMessage;
    use serde_json::Value;

    let path = fixture_path(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let recording = text.lines().next().is_some_and(|first| {
        serde_json::from_str::<Value>(first).is_ok_and(|v| v.get("version").is_some())
    });
    if !recording {
        return text;
    }
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| {
            let name = line.get("event")?.as_str()?.to_string();
            let mut message =
                SessionEventMessage::new("s1", name, line.get("data").cloned().unwrap_or_default());
            message.seq = line.get("seq").and_then(Value::as_u64);
            message.timestamp = line
                .get("ts")
                .and_then(|ts| serde_json::from_value(ts.clone()).ok());
            message
                .payload()
                .is_ok_and(|p| p.is_persisted())
                .then(|| serde_json::to_string(&message).expect("a wire line"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compare `got` with the golden file at `path`. `CRUCIBLE_WRITE_GOLDEN=1`
/// writes the file instead.
pub fn assert_golden(path: &std::path::Path, got: &str) {
    if std::env::var_os("CRUCIBLE_WRITE_GOLDEN").is_some() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(path, format!("{}\n", got.trim_end())).unwrap();
        return;
    }
    let want = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        got.trim_end(),
        want.trim_end(),
        "{} differs. The reader gives:\n{got}",
        path.display()
    );
}

/// A real [`crate::Server`] bound in process, run on a spawned task, over an
/// isolated data root.
///
/// Every daemon integration test used to carry its own copy of this: a temp
/// dir, a socket path, one of the `bind_with_data_home*` constructors, a
/// spawned `server.run()`, a readiness poll and a shutdown. The copies drifted
/// (a 5s deadline here, a 60s one there; `assert!` in one, `bail!` in
/// another) without any of the difference being a deliberate test decision.
/// [`InProcessDaemonBuilder`] is the one place that setup lives now; a test
/// asks for the kilns and the socket kind it needs and gets the daemon back.
pub struct InProcessDaemon {
    // `None` when the caller supplied its own `data_home` (and therefore owns
    // whatever temp directory backs it) rather than asking the builder for a
    // fresh one.
    _temp_dir: Option<tempfile::TempDir>,
    // Held only to keep `XDG_RUNTIME_DIR` set for the daemon's lifetime; nothing
    // reads it back.
    _env_guard: Option<crucible_core::test_support::EnvVarGuard>,
    data_home: std::path::PathBuf,
    socket_path: std::path::PathBuf,
    kilns: Vec<(String, std::path::PathBuf)>,
    server_handle: tokio::task::JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl InProcessDaemon {
    /// Start with no kilns and a fresh temp data home. Most callers want
    /// [`InProcessDaemonBuilder`] instead, to register a kiln or two first.
    pub async fn start() -> anyhow::Result<Self> {
        InProcessDaemonBuilder::new()?.start().await
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn data_home(&self) -> &std::path::Path {
        &self.data_home
    }

    /// Where a session's own files land under this daemon's data home —
    /// never inside a kiln, which is the thing more than one fixture asserts.
    pub fn sessions_root(&self) -> std::path::PathBuf {
        self.data_home.join("sessions")
    }

    /// The directory registered under `name`. Panics if the builder never
    /// registered that name — a fixture asking for an unregistered kiln has a
    /// bug, not a missing-value case to handle.
    pub fn kiln_dir(&self, name: &str) -> std::path::PathBuf {
        self.kilns
            .iter()
            .find(|(registered, _)| registered == name)
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| panic!("no kiln named {name:?} was registered on this daemon"))
    }

    /// Connect a [`crate::DaemonClient`] to this daemon over its socket.
    pub async fn connect(&self) -> crate::DaemonClient {
        crate::DaemonClient::connect_to(&self.socket_path)
            .await
            .expect("connect to the in-process test daemon")
    }

    /// Send the shutdown signal and wait for `server.run()` to actually
    /// return, rather than a fixed sleep guessing how long that takes.
    pub async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), self.server_handle).await;
    }
}

/// Builds an [`InProcessDaemon`]. See that type for what it replaces.
pub struct InProcessDaemonBuilder {
    temp_dir: Option<tempfile::TempDir>,
    data_home: std::path::PathBuf,
    // name, path, lazy
    kilns: Vec<(String, std::path::PathBuf, bool)>,
    ready_timeout: Duration,
    xdg_runtime_socket: bool,
}

impl InProcessDaemonBuilder {
    /// A fresh temp directory as the data home.
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let data_home = temp_dir.path().to_path_buf();
        Ok(Self {
            temp_dir: Some(temp_dir),
            data_home,
            kilns: Vec::new(),
            ready_timeout: Duration::from_secs(5),
            xdg_runtime_socket: false,
        })
    }

    /// Bind over a `data_home` the caller already owns, instead of a fresh
    /// temp directory — for a fixture that seeds files (an `llm.json`, a
    /// settings file) into the data home before the daemon reads it, and
    /// therefore has to hold the `TempDir` itself.
    pub fn at_data_home(data_home: std::path::PathBuf) -> Self {
        Self {
            temp_dir: None,
            data_home,
            kilns: Vec::new(),
            ready_timeout: Duration::from_secs(5),
            xdg_runtime_socket: false,
        }
    }

    pub fn data_home(&self) -> &std::path::Path {
        &self.data_home
    }

    /// Register an eager kiln named `name`, in a directory this builder
    /// creates under the data home.
    pub fn with_kiln(self, name: &str) -> Self {
        let path = self.data_home.join("kilns").join(name);
        self.with_kiln_at(name, path)
    }

    /// Register an eager kiln named `name` at a caller-chosen path, creating
    /// the directory if it does not already exist. The path may lie outside
    /// the data home, and usually should: the registration floor refuses a
    /// kiln inside the data root it is protecting.
    pub fn with_kiln_at(mut self, name: &str, path: impl Into<std::path::PathBuf>) -> Self {
        let path = path.into();
        std::fs::create_dir_all(&path).expect("create the kiln directory for a test fixture");
        self.kilns.push((name.to_string(), path, false));
        self
    }

    /// As [`Self::with_kiln_at`], registered LAZY: boot does not open it, so
    /// a fixture proving "nothing opened this kiln" has one to point at that
    /// isn't opened by the daemon's own startup.
    pub fn with_lazy_kiln_at(mut self, name: &str, path: impl Into<std::path::PathBuf>) -> Self {
        let path = path.into();
        std::fs::create_dir_all(&path).expect("create the kiln directory for a test fixture");
        self.kilns.push((name.to_string(), path, true));
        self
    }

    /// How long to poll for the socket to accept connections before giving
    /// up. Most fixtures keep the 5s default; a suite that runs under heavy
    /// contention (many daemons at once, cold binaries) may need longer.
    pub fn with_ready_timeout(mut self, timeout: Duration) -> Self {
        self.ready_timeout = timeout;
        self
    }

    /// Resolve the socket path through `XDG_RUNTIME_DIR` and
    /// [`crate::rpc_client::lifecycle::default_socket_path`], the way a real
    /// `cru` client resolves it, instead of a path this builder picks
    /// itself. For a fixture proving that a CLI-side helper finds the daemon
    /// the same way the daemon bound it.
    pub fn using_xdg_runtime_socket(mut self) -> Self {
        self.xdg_runtime_socket = true;
        self
    }

    pub async fn start(self) -> anyhow::Result<InProcessDaemon> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let env_guard = if self.xdg_runtime_socket {
            Some(crucible_core::test_support::EnvVarGuard::set(
                "XDG_RUNTIME_DIR",
                self.data_home
                    .to_str()
                    .expect("a UTF-8 data home path")
                    .to_string(),
            ))
        } else {
            None
        };

        let socket_path = if self.xdg_runtime_socket {
            crate::rpc_client::lifecycle::default_socket_path()
        } else {
            self.data_home.join("daemon.sock")
        };

        let entries: Vec<(&str, &std::path::Path, bool)> = self
            .kilns
            .iter()
            .map(|(name, path, lazy)| (name.as_str(), path.as_path(), *lazy))
            .collect();

        let server = crate::Server::bind_with_data_home_and_kiln_entries(
            &socket_path,
            self.data_home.clone(),
            &entries,
        )
        .await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Poll for readiness rather than sleeping a fixed interval. Under a
        // loaded box the socket may not be accepting when a fixed timer
        // elapses, which is the standard intermittent-failure source these
        // fixtures used to hit independently.
        let deadline = tokio::time::Instant::now() + self.ready_timeout;
        loop {
            if crate::DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "daemon did not start accepting connections within {:?}",
                    self.ready_timeout
                );
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(InProcessDaemon {
            _temp_dir: self.temp_dir,
            _env_guard: env_guard,
            data_home: self.data_home,
            socket_path,
            kilns: self
                .kilns
                .into_iter()
                .map(|(name, path, _)| (name, path))
                .collect(),
            server_handle,
            shutdown_handle,
        })
    }
}
