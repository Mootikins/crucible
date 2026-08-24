//! Watcher backstop for the review ledger.
//!
//! A capture bracket (`crate::review`) can only attribute writes that land
//! between a tool call's before-tree and after-tree. Three kinds of write
//! escape that window, and none of them are hypothetical:
//!
//! - an async formatter or language server landing after a `Bash` call returned,
//! - a Lua plugin writing straight to disk instead of through a tool,
//! - the user editing in their own editor while the agent works.
//!
//! The damage is worse than a missing feature. The next bracket's before-tree
//! swallows the write, so it either vanishes from the composed diff or gets
//! credited to whichever tool call happened to run next. This module watches
//! the session's roots and records the writes no bracket claimed, so the
//! composed diff can call them `external` — the user's own work, displayed but
//! never rejectable — instead of lying about who made them.
//!
//! The tracker is purely additive: it never removes a hunk and never overrides
//! the ledger's attribution. A missed event therefore degrades to "the queue
//! refreshes a little later", never to a confident wrong answer.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crucible_core::EXCLUDED_DIRS;
use dashmap::DashMap;
// Tokio's clock rather than `std`'s, so the linger below is testable under
// `tokio::time::pause` instead of by sleeping.
use tokio::sync::{broadcast, Mutex};
use tokio::time::Instant;
use tracing::{debug, info, warn};

use crate::watch::{
    error::Result,
    handlers::ExternalChangeHandler,
    traits::{DebounceConfig, WatchConfig},
    WatchManager, WatchManagerConfig,
};

/// Directories never handed to the watch backend, whatever git says.
///
/// `.git` is the one that matters: git does not report its own directory as
/// ignored, and every command the ledger runs writes inside it, so watching it
/// is a feedback loop. The rest are shared with the kiln subsystem and cost
/// nothing to keep.
///
/// Everything else comes from [`crate::review::git::ignored_dirs`]. A
/// hand-written list of build directories was here before, and it was wrong in
/// both directions: it named `target` and `dist` (Rust and Node, nothing else)
/// and it matched only at depth one, so `crates/*/target` and
/// `docs-site/node_modules` went into the recursive watch anyway.
fn always_excluded(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    EXCLUDED_DIRS.iter().map(|dir| root.join(dir))
}

/// Coalescing window for worktree events.
///
/// A save-on-keystroke editor and a formatter both emit bursts; the review
/// queue only needs to know the worktree moved, so anything shorter than this
/// buys nothing and costs a recomputed composed diff per keystroke.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// How long a closed [`CaptureWindow`] keeps suppressing its roots.
///
/// Without this the suppression is decorative. A write is debounced for
/// [`DEBOUNCE`] before the handler ever sees it, so a window that closes the
/// instant the write completes has already reopened by the time its own event
/// arrives — and the writer, whether a tool call or the daemon reverting a
/// rejected hunk, is reported as an external edit anyway.
///
/// Over-suppressing costs a user edit made within this window of a tool call
/// going unannounced until their next keystroke, because the record is a
/// notification and not attribution — `is_external()` is
/// `tool_call_ids.is_empty()`, decided by the ledger, whatever the watcher
/// saw. Under-suppressing costs a feedback loop.
///
/// The delivery path holds a write for two debounce stages, not one: the
/// notify backend waits [`DEBOUNCE`], then the manager's own debouncer waits
/// [`DEBOUNCE`] again. Each stage adds up to a quarter of its window in tick
/// slack. The linger therefore covers two stages plus slack.
const SUPPRESSION_LINGER: Duration = Duration::from_millis(2 * DEBOUNCE.as_millis() as u64 + 200);

/// A worktree change no capture bracket owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalChange {
    /// Tracked root the change fell under (a repository top level).
    pub root: PathBuf,
    /// Absolute path that changed.
    pub path: PathBuf,
    /// Sessions tracking `root` when the change landed.
    pub sessions: Vec<String>,
}

/// Who owns an observed write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// A capture bracket was open over the root. The ledger's tree diff
    /// attributes this write exactly; the watcher stays out of it.
    Bracketed,
    /// Nothing was writing under the ledger's supervision, so no interval can
    /// own this change and the composed diff must show it as `external`.
    External,
    /// Not under any tracked root, or filtered as build/editor noise.
    Untracked,
}

#[derive(Debug, Default)]
struct RootState {
    /// Sessions tracking this root. A root is watched while any session holds
    /// it, so a parent and its delegated children share one watch.
    sessions: Vec<String>,
    /// Capture brackets currently open over this root, across all sessions.
    open_brackets: usize,
    /// When the last bracket over this root stopped suppressing. See
    /// [`SUPPRESSION_LINGER`].
    suppressed_until: Option<Instant>,
    /// Paths seen changing with no bracket open.
    external: HashSet<PathBuf>,
    /// Directories git ignores under this root, as of the last registration.
    ///
    /// The plan already keeps these out of the backend, so this catches the
    /// case the plan cannot: an ignored *file* inside a directory that must
    /// stay watched. Reporting one costs a redundant recompute rather than a
    /// wrong diff — the record is a notification, and the ledger decides
    /// attribution — so a snapshot taken at registration is enough.
    ignored: HashSet<PathBuf>,
}

impl RootState {
    fn is_suppressed(&self) -> bool {
        self.open_brackets > 0
            || self
                .suppressed_until
                .is_some_and(|until| Instant::now() < until)
    }
}

/// Records worktree writes that no capture bracket owns.
///
/// Shared between the watch handler that observes writes and whoever brackets
/// them. Cheap to clone behind an `Arc`; all state is interior-mutable and
/// lock-free at the shard level, because [`Self::observe`] runs on the watch
/// event path and must never be the thing that stalls it.
pub struct ExternalChangeTracker {
    roots: DashMap<PathBuf, RootState>,
    changes_tx: broadcast::Sender<ExternalChange>,
}

impl Default for ExternalChangeTracker {
    fn default() -> Self {
        Self {
            roots: DashMap::new(),
            // Bounded: a lagging subscriber drops notifications rather than
            // holding the watch path hostage. Losing one is safe — the record
            // in `external` survives and a later change re-notifies.
            changes_tx: broadcast::channel(256).0,
        }
    }
}

impl ExternalChangeTracker {
    /// Subscribe to unowned changes as they are observed.
    pub fn subscribe(&self) -> broadcast::Receiver<ExternalChange> {
        self.changes_tx.subscribe()
    }

    /// Track `roots` on behalf of a session, returning the roots that were not
    /// already tracked — the ones a watch still has to be added for.
    pub fn track(&self, session_id: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut fresh = Vec::new();
        for root in roots {
            let mut state = self.roots.entry(root.clone()).or_default();
            let is_new = state.sessions.is_empty();
            if !state.sessions.iter().any(|s| s == session_id) {
                state.sessions.push(session_id.to_string());
            }
            if is_new {
                fresh.push(root.clone());
            }
        }
        fresh
    }

    /// Drop a session's claim on its roots, returning the roots no session
    /// holds any more — the ones whose watch can be torn down.
    ///
    /// A root with a bracket still open is kept regardless. Forgetting it
    /// mid-bracket would leave the outstanding [`CaptureWindow`] decrementing
    /// a counter nobody reads, and the next session on that root would see
    /// its own writes reported as external.
    pub fn untrack_session(&self, session_id: &str) -> Vec<PathBuf> {
        let mut released = Vec::new();
        self.roots.retain(|root, state| {
            state.sessions.retain(|s| s != session_id);
            if state.sessions.is_empty() && state.open_brackets == 0 {
                released.push(root.clone());
                false
            } else {
                true
            }
        });
        released
    }

    /// Record the directories git ignores under `root`.
    pub fn set_ignored(&self, root: &Path, ignored: HashSet<PathBuf>) {
        if let Some(mut state) = self.roots.get_mut(root) {
            state.ignored = ignored;
        }
    }

    /// Forget `root`'s ignore set, once nothing watches it.
    pub fn clear_ignored(&self, root: &Path) {
        if let Some(mut state) = self.roots.get_mut(root) {
            state.ignored.clear();
        }
    }

    /// Roots currently watched. Only the tests observe the set.
    #[cfg(test)]
    pub(crate) fn tracked_roots(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|e| e.key().clone()).collect()
    }

    /// Unowned changes recorded under `root` so far.
    pub fn external_paths(&self, root: &Path) -> Vec<PathBuf> {
        self.roots
            .get(root)
            .map(|s| s.external.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Claim every root `session_id` tracks for as long as the returned guard
    /// lives.
    ///
    /// This is the self-write suppression the backstop needs in both
    /// directions. Writes an agent makes through a bracketed tool call are the
    /// ledger's to attribute, and reporting them as external would double-count
    /// them; writes the *daemon* makes — reverting a rejected hunk — would
    /// otherwise come straight back in as a user edit, which is a feedback loop
    /// that ends with the queue refilling itself.
    ///
    /// Nested and concurrent brackets are counted, so a delegated session
    /// bracketing the same root does not un-suppress its parent's window.
    pub fn capture(self: &Arc<Self>, session_id: &str) -> CaptureWindow {
        let mut roots = Vec::new();
        for mut entry in self.roots.iter_mut() {
            if entry.sessions.iter().any(|s| s == session_id) {
                entry.open_brackets += 1;
                roots.push(entry.key().clone());
            }
        }
        CaptureWindow {
            tracker: Arc::clone(self),
            roots,
        }
    }

    /// Classify one observed write, recording it when nobody owns it.
    pub fn observe(&self, path: &Path) -> Ownership {
        let Some(root) = self.root_for(path) else {
            return Ownership::Untracked;
        };
        if is_noise(&root, path) {
            return Ownership::Untracked;
        }

        let sessions = {
            let Some(mut state) = self.roots.get_mut(&root) else {
                return Ownership::Untracked;
            };
            if state.ignored.iter().any(|dir| path.starts_with(dir)) {
                return Ownership::Untracked;
            }
            if state.is_suppressed() {
                return Ownership::Bracketed;
            }
            state.external.insert(path.to_path_buf());
            state.sessions.clone()
        };

        debug!(
            root = %root.display(),
            path = %path.display(),
            "worktree changed with no capture bracket open; recording as external"
        );
        // Sent on every observation, not only the first for a path: the channel
        // is a "the composed diff moved" signal, and a client that only heard
        // about the first edit to a file would render every later one stale.
        let _ = self.changes_tx.send(ExternalChange {
            root,
            path: path.to_path_buf(),
            sessions,
        });
        Ownership::External
    }

    /// Undo a single [`Self::track`] claim. Only for the caller that failed to
    /// establish the watch it had just claimed a root for.
    fn untrack_root(&self, root: &Path, session_id: &str) {
        let empty = {
            let Some(mut state) = self.roots.get_mut(root) else {
                return;
            };
            state.sessions.retain(|s| s != session_id);
            state.sessions.is_empty() && state.open_brackets == 0
        };
        if empty {
            self.roots.remove(root);
        }
    }

    /// The tracked root containing `path`, longest match first so a kiln
    /// nested inside a workspace repo resolves to the kiln.
    fn root_for(&self, path: &Path) -> Option<PathBuf> {
        self.roots
            .iter()
            .filter(|e| path.starts_with(e.key()))
            .max_by_key(|e| e.key().as_os_str().len())
            .map(|e| e.key().clone())
    }
}

/// An open capture bracket, as far as the watcher is concerned.
///
/// Held for the duration of a bracketed tool call or a daemon-side write.
/// Dropping it re-arms external detection for the roots it claimed.
pub struct CaptureWindow {
    tracker: Arc<ExternalChangeTracker>,
    roots: Vec<PathBuf>,
}

impl std::fmt::Debug for CaptureWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureWindow")
            .field("roots", &self.roots)
            .finish()
    }
}

impl Drop for CaptureWindow {
    fn drop(&mut self) {
        for root in &self.roots {
            if let Some(mut state) = self.tracker.roots.get_mut(root) {
                state.open_brackets = state.open_brackets.saturating_sub(1);
                // The last bracket to leave arms the linger; a nested one
                // leaving must not, or it would shorten its parent's window
                // to nothing.
                if state.open_brackets == 0 {
                    state.suppressed_until = Some(Instant::now() + SUPPRESSION_LINGER);
                }
            }
        }
    }
}

/// Whether a path under `root` is churn rather than a change worth reviewing.
fn is_noise(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    // Only the directory components: a source file legitimately named
    // `build` or `target` is a change worth reviewing.
    let in_excluded_dir = relative
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .any(|c| {
            let Component::Normal(name) = c else {
                return false;
            };
            name.to_str()
                .is_some_and(|name| EXCLUDED_DIRS.contains(&name))
        });
    in_excluded_dir || is_editor_scratch(path)
}

/// Editor scratch and atomic-save temporaries.
///
/// A vim swapfile or an `emacs` lockfile is a write the user is about to
/// undo. Surfacing those makes the queue flicker on every keystroke burst,
/// and the file they shadow generates its own event anyway.
fn is_editor_scratch(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.ends_with(".tmp")
        || name.starts_with(".#")
        || (name.starts_with('#') && name.ends_with('#'))
}

/// Debounced watches over the roots a session may write to.
///
/// One [`WatchManager`] serves every session, with roots refcounted across
/// them. Delegated children share their parent's workspace verbatim, so a
/// watch per session per root would open the same inotify descriptors several
/// times over and deliver the same write to the tracker several times.
pub struct ExternalChangeWatch {
    manager: Mutex<WatchManager>,
    tracker: Arc<ExternalChangeTracker>,
    /// Backend group id per watched root. A root's watches are added and
    /// dropped as one group, because a plan is hundreds of paths sharing a
    /// single inotify instance.
    watches: Mutex<Vec<(PathBuf, String)>>,
}

impl ExternalChangeWatch {
    /// Start the backstop. Watches are added per session with
    /// [`Self::watch_session`].
    pub async fn start(tracker: Arc<ExternalChangeTracker>) -> Result<Self> {
        let config = WatchManagerConfig {
            // The default handler set is the kiln indexer. A workspace watch
            // exists to answer "who wrote this line", not to feed the note
            // pipeline, and registering the indexer here would re-index every
            // source file in the repo on every build.
            enable_default_handlers: false,
            queue_capacity: 1000,
            debounce: DebounceConfig::new(DEBOUNCE.as_millis() as u64),
        };

        let mut manager = WatchManager::new(config).await?;
        manager
            .register_handler(Arc::new(ExternalChangeHandler::new(Arc::clone(&tracker))))
            .await?;
        manager.start().await?;

        Ok(Self {
            manager: Mutex::new(manager),
            tracker,
            watches: Mutex::new(Vec::new()),
        })
    }

    /// The tracker this watch feeds. Bracket writes through
    /// [`ExternalChangeTracker::capture`] so they are not re-detected.
    pub fn tracker(&self) -> &Arc<ExternalChangeTracker> {
        &self.tracker
    }

    /// Watch `roots` on behalf of a session. Roots another session already
    /// holds are reused.
    pub async fn watch_session(&self, session_id: &str, roots: &[PathBuf]) -> Result<()> {
        let fresh = self.tracker.track(session_id, roots);

        // Refresh every tracked root's ignore set, not only the fresh ones.
        // The set is what keeps build output out of the queue, and it is a
        // snapshot: a `target/` created after the watch was established is
        // inside a recursive watch (that is how directories created later stay
        // covered) and matches nothing frozen, so its events would arrive
        // unfiltered. Re-asking git costs about five milliseconds per root per
        // turn, because git does not descend a wholly ignored tree.
        for root in roots {
            self.tracker.set_ignored(root, ignored_paths(root).await);
        }

        if fresh.is_empty() {
            return Ok(());
        }

        let mut manager = self.manager.lock().await;
        let mut watches = self.watches.lock().await;
        for root in fresh {
            // Only the DIRECTORIES prune the walk. An ignored file cannot be
            // pruned — it sits inside a directory that must stay watched — so
            // handing it to the plan would split that parent for nothing.
            //
            // A root git cannot answer for is watched whole rather than not at
            // all: a slow watch still reports external edits, and this module
            // exists to prevent silence.
            let mut ignored: HashSet<PathBuf> = always_excluded(&root).collect();
            match crate::review::git::ignored_entries(&root).await {
                Ok(entries) => ignored.extend(entries.dirs),
                Err(e) => {
                    warn!(
                        root = %root.display(),
                        error = %e,
                        "git could not list ignored directories; watching the root whole"
                    );
                }
            }

            // Off the runtime: the plan reads every directory it will watch,
            // and it runs inside the turn carrying the user's message. The
            // recursive watch it replaces cost 13 seconds of dead screen on a
            // repository with a 400 GB `target/`, and on a cold one it never
            // returned at all.
            let plan = {
                let planning_root = root.clone();
                let planning_ignored = ignored.clone();
                match tokio::task::spawn_blocking(move || {
                    plan_watches(&planning_root, &planning_ignored)
                })
                .await
                {
                    Ok(plan) => plan,
                    Err(e) => {
                        warn!(root = %root.display(), error = %e, "watch plan failed; root unwatched");
                        self.tracker.untrack_root(&root, session_id);
                        continue;
                    }
                }
            };
            let template = WatchConfig::new(String::new())
                .with_debounce(DebounceConfig::new(DEBOUNCE.as_millis() as u64));
            let group = watch_group_id(&root);

            match manager.add_watch_group(&group, &plan, &template).await {
                Ok(count) => {
                    info!(
                        root = %root.display(),
                        watches = count,
                        "watching workspace root for unowned changes"
                    );
                    watches.push((root, group));
                }
                Err(e) => {
                    // A root we cannot watch is a root whose external changes
                    // go unrecorded. That is survivable — attribution still
                    // works for bracketed writes — but it is exactly the
                    // silence this module exists to prevent, so say so.
                    warn!(
                        root = %root.display(),
                        error = %e,
                        "failed to watch root; unowned changes there will not be detected"
                    );
                    self.tracker.untrack_root(&root, session_id);
                }
            }
        }
        Ok(())
    }

    /// Release a session's roots, tearing down the watches nothing holds.
    pub async fn unwatch_session(&self, session_id: &str) -> Result<()> {
        let released = self.tracker.untrack_session(session_id);
        if released.is_empty() {
            return Ok(());
        }

        let mut manager = self.manager.lock().await;
        let mut watches = self.watches.lock().await;
        let mut remaining = Vec::with_capacity(watches.len());
        for (root, group) in watches.drain(..) {
            if released.contains(&root) {
                if let Err(e) = manager.remove_watch_group(&group).await {
                    warn!(root = %root.display(), error = %e, "failed to remove review watch");
                }
                self.tracker.clear_ignored(&root);
            } else {
                remaining.push((root, group));
            }
        }
        *watches = remaining;
        Ok(())
    }

    /// Stop watching everything.
    pub async fn shutdown(&self) -> Result<()> {
        self.watches.lock().await.clear();
        self.manager.lock().await.shutdown().await
    }
}

/// Every path git ignores under `root`, plus the names excluded regardless.
///
/// Directories AND files: the event-time filter tests by prefix, and an
/// ignored file (`.env.local`) sits inside a directory that stays watched, so
/// nothing else can catch it.
async fn ignored_paths(root: &Path) -> HashSet<PathBuf> {
    let mut ignored: HashSet<PathBuf> = always_excluded(root).collect();
    match crate::review::git::ignored_entries(root).await {
        Ok(entries) => ignored.extend(entries.all().cloned()),
        Err(e) => {
            debug!(
                root = %root.display(),
                error = %e,
                "git could not list ignored paths; build output may reach the queue"
            );
        }
    }
    ignored
}

/// The backend group holding every watch for `root`.
fn watch_group_id(root: &Path) -> String {
    format!("review-{}", root.display())
}

/// Watches covering every directory under `root` that git does not ignore,
/// and none that it does.
///
/// A subtree holding no ignored directory gets a single recursive watch. That
/// is not only fewer watches: recursive mode is also what keeps directories
/// created *later* covered, so the cover stays as close to the old behaviour
/// as pruning allows. A subtree that does hold one gets a non-recursive watch
/// and a descent, so the ignored directory is never named to the backend at
/// all — which is the whole point, because the backend would walk it.
///
/// Returns `(path, recursive)` pairs, parents before children.
fn plan_watches(root: &Path, ignored: &HashSet<PathBuf>) -> Vec<(PathBuf, bool)> {
    fn walk(dir: &Path, ignored: &HashSet<PathBuf>, plan: &mut Vec<(PathBuf, bool)>) {
        if !ignored.iter().any(|path| path.starts_with(dir)) {
            plan.push((dir.to_path_buf(), true));
            return;
        }
        plan.push((dir.to_path_buf(), false));

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                // `dir` keeps its own non-recursive watch, but everything below
                // it now has none. Every other failure here says so; this one
                // used to return in silence, which reads as "nothing to watch".
                warn!(
                    dir = %dir.display(),
                    error = %e,
                    "could not read directory; changes below it will not be detected"
                );
                return;
            }
        };
        for entry in entries.flatten() {
            // `file_type` reports the link itself, so a symlinked directory is
            // not descended. notify does not follow them either, and a link
            // out of the worktree is not this root's to report.
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let child = entry.path();
            if ignored.contains(&child) {
                continue;
            }
            walk(&child, ignored, plan);
        }
    }

    let mut plan = Vec::new();
    walk(root, ignored, &mut plan);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the plan hands `dir` to the backend, directly or inside a
    /// recursive watch. This is the question the bug got wrong: the old code
    /// named only the root, recursively, so every ignored directory under it
    /// was walked and watched.
    fn plan_reaches(plan: &[(PathBuf, bool)], dir: &Path) -> bool {
        plan.iter()
            .any(|(path, recursive)| path == dir || (*recursive && dir.starts_with(path)))
    }

    fn dirs(root: &Path, relatives: &[&str]) {
        for rel in relatives {
            std::fs::create_dir_all(root.join(rel)).unwrap();
        }
    }

    #[test]
    fn plan_never_reaches_an_ignored_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        dirs(root, &["src", "target/debug/build/deps", "target/release"]);

        let ignored = HashSet::from([root.join("target")]);
        let plan = plan_watches(root, &ignored);

        assert!(!plan_reaches(&plan, &root.join("target")));
        assert!(!plan_reaches(&plan, &root.join("target/debug/build/deps")));
        assert!(plan_reaches(&plan, &root.join("src")));
    }

    #[test]
    fn plan_reaches_an_ignored_directory_nested_deep() {
        // The list this replaced joined each name onto the root, so it matched
        // at depth one only: `crates/cli/target` went into the recursive watch.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        dirs(root, &["crates/cli/src", "crates/cli/target/debug"]);

        let ignored = HashSet::from([root.join("crates/cli/target")]);
        let plan = plan_watches(root, &ignored);

        assert!(!plan_reaches(&plan, &root.join("crates/cli/target")));
        assert!(!plan_reaches(&plan, &root.join("crates/cli/target/debug")));
        assert!(plan_reaches(&plan, &root.join("crates/cli/src")));
    }

    #[test]
    fn a_clean_tree_is_one_recursive_watch() {
        // Fewer watches is the lesser point. One recursive watch is also what
        // covers directories created after registration, which a cover of
        // non-recursive watches would miss.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        dirs(root, &["a/b/c", "d"]);

        let plan = plan_watches(root, &HashSet::new());

        assert_eq!(plan, vec![(root.to_path_buf(), true)]);
    }

    #[test]
    fn only_the_branches_holding_an_ignored_directory_are_descended() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        dirs(
            root,
            &["clean/deep/deeper", "dirty/src", "dirty/node_modules"],
        );

        let ignored = HashSet::from([root.join("dirty/node_modules")]);
        let plan = plan_watches(root, &ignored);

        // `clean` is untouched by the ignore set, so it costs one watch, not
        // one per directory under it.
        assert!(plan.contains(&(root.join("clean"), true)));
        assert!(!plan
            .iter()
            .any(|(path, _)| path == &root.join("clean/deep")));

        assert!(plan.contains(&(root.join("dirty"), false)));
        assert!(plan.contains(&(root.join("dirty/src"), true)));
        assert!(!plan_reaches(&plan, &root.join("dirty/node_modules")));
    }

    #[test]
    fn a_symlinked_directory_is_not_descended() {
        // The symlink has to sit in a branch the walk actually descends, so
        // it needs an ignored sibling. In a clean branch the walk stops at one
        // recursive watch and never reads the directory at all.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        dirs(root, &["outside/inner", "dirty/node_modules", "dirty/src"]);
        std::os::unix::fs::symlink(root.join("outside"), root.join("dirty/link")).unwrap();

        let ignored = HashSet::from([root.join("dirty/node_modules")]);
        let plan = plan_watches(root, &ignored);

        assert!(plan.contains(&(root.join("dirty"), false)));
        assert!(plan.contains(&(root.join("dirty/src"), true)));
        assert!(!plan
            .iter()
            .any(|(path, _)| path == &root.join("dirty/link")));
    }

    /// A repository with one wholly ignored directory and one ignored file
    /// beside tracked ones — the two shapes that need opposite handling.
    async fn repo_with_ignored_entries(root: &Path) {
        crate::test_support::init_repo(
            root,
            &[(".gitignore", "target/\n.env.local\n"), ("src/a.rs", "")],
        )
        .await;
        dirs(root, &["target/debug/build/deps"]);
        std::fs::write(root.join("target/debug/x.o"), "").unwrap();
        std::fs::write(root.join(".env.local"), "SECRET=1").unwrap();
    }

    #[tokio::test]
    async fn git_reports_an_ignored_directory_as_one_entry() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        repo_with_ignored_entries(root).await;

        let ignored = crate::review::git::ignored_entries(root).await.unwrap();

        // One entry, not one per directory: git does not descend a wholly
        // ignored tree, which is what keeps this call cheap on a large one.
        assert_eq!(ignored.dirs, vec![root.join("target")]);
    }

    #[tokio::test]
    async fn git_reports_an_ignored_file_separately_from_a_directory() {
        // `--directory` collapses only WHOLLY ignored directories. A standalone
        // ignored file is listed on its own, with no trailing slash. Keeping
        // just the trailing-slash entries dropped it, so the event-time filter
        // never learned about `.env.local` and reported it as an external
        // change — the thing this was supposed to stop.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        repo_with_ignored_entries(root).await;

        let ignored = crate::review::git::ignored_entries(root).await.unwrap();

        assert_eq!(ignored.files, vec![root.join(".env.local")]);
    }

    #[tokio::test]
    async fn the_event_time_set_carries_ignored_files_from_git() {
        // The end-to-end shape. The other test for this built the tracker
        // state by hand, so it passed while the real pipeline produced a set
        // that could never contain a file.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        repo_with_ignored_entries(root).await;

        let ignored = ignored_paths(root).await;

        assert!(ignored.contains(&root.join(".env.local")));
        assert!(ignored.contains(&root.join("target")));
    }

    #[tokio::test]
    async fn an_ignored_file_from_git_is_untracked_at_event_time() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        repo_with_ignored_entries(&root).await;

        let tracker = tracker_over(&root);
        tracker.set_ignored(&root, ignored_paths(&root).await);

        assert_eq!(
            tracker.observe(&root.join(".env.local")),
            Ownership::Untracked
        );
        assert_eq!(tracker.observe(&root.join("src/a.rs")), Ownership::External);
    }

    #[tokio::test]
    async fn an_ignored_file_is_untracked_at_event_time() {
        // The plan cannot prune this one: `.env.local` sits in a directory
        // that has to stay watched. `git add -A` skips it, so no hunk can
        // exist for it, so recording it as external would be a lie.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let tracker = tracker_over(&root);
        tracker.set_ignored(&root, HashSet::from([root.join(".env.local")]));

        assert_eq!(
            tracker.observe(&root.join(".env.local")),
            Ownership::Untracked
        );
        assert_eq!(tracker.observe(&root.join("src/a.rs")), Ownership::External);
    }

    fn tracker_over(root: &Path) -> Arc<ExternalChangeTracker> {
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("s1", &[root.to_path_buf()]);
        tracker
    }

    #[test]
    fn track_reports_only_roots_not_already_watched() {
        let tracker = Arc::new(ExternalChangeTracker::default());
        let root = PathBuf::from("/repo");

        assert_eq!(
            tracker.track("s1", std::slice::from_ref(&root)),
            vec![root.clone()]
        );
        assert!(tracker.track("s2", std::slice::from_ref(&root)).is_empty());
    }

    #[test]
    fn root_is_released_only_when_the_last_session_lets_go() {
        let tracker = Arc::new(ExternalChangeTracker::default());
        let root = PathBuf::from("/repo");
        tracker.track("s1", std::slice::from_ref(&root));
        tracker.track("s2", std::slice::from_ref(&root));

        assert!(tracker.untrack_session("s1").is_empty());
        assert_eq!(tracker.tracked_roots(), vec![root.clone()]);
        assert_eq!(tracker.untrack_session("s2"), vec![root]);
        assert!(tracker.tracked_roots().is_empty());
    }

    #[test]
    fn root_with_an_open_bracket_survives_untrack() {
        let tracker = Arc::new(ExternalChangeTracker::default());
        let root = PathBuf::from("/repo");
        tracker.track("s1", std::slice::from_ref(&root));
        let window = tracker.capture("s1");

        assert!(tracker.untrack_session("s1").is_empty());
        assert_eq!(tracker.tracked_roots(), vec![root]);
        drop(window);
    }

    #[test]
    fn write_with_no_bracket_open_is_external() {
        let root = PathBuf::from("/repo");
        let tracker = tracker_over(&root);

        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::External
        );
        assert_eq!(
            tracker.external_paths(&root),
            vec![PathBuf::from("/repo/src/main.rs")]
        );
    }

    /// Past the linger a closed [`CaptureWindow`] leaves behind, without
    /// spending it in real time.
    async fn past_the_linger() {
        tokio::time::advance(SUPPRESSION_LINGER + Duration::from_millis(1)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn write_inside_a_capture_window_belongs_to_the_ledger() {
        let root = PathBuf::from("/repo");
        let tracker = tracker_over(&root);

        let window = tracker.capture("s1");
        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::Bracketed
        );
        assert!(tracker.external_paths(&root).is_empty());

        drop(window);
        past_the_linger().await;
        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::External
        );
    }

    /// The window has to outlive the write itself, because the write is
    /// debounced before anyone classifies it. A window that closed the
    /// instant its writer returned would suppress nothing at all.
    #[tokio::test(start_paused = true)]
    async fn suppression_outlives_the_debounce_that_delivers_the_write() {
        let root = PathBuf::from("/repo");
        let tracker = tracker_over(&root);

        drop(tracker.capture("s1"));
        // Both stages of the delivery path, each with its tick slack: the
        // notify backend, then the manager's debouncer.
        tokio::time::advance(DEBOUNCE + DEBOUNCE / 4).await;
        tokio::time::advance(DEBOUNCE + DEBOUNCE / 4).await;

        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::Bracketed,
            "a write debounced past the window's close was reported as the user's"
        );
        assert!(tracker.external_paths(&root).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_brackets_are_counted_not_flagged() {
        let root = PathBuf::from("/repo");
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("parent", std::slice::from_ref(&root));
        tracker.track("child", std::slice::from_ref(&root));

        let parent = tracker.capture("parent");
        let child = tracker.capture("child");
        drop(child);
        // Past the linger deliberately: the parent's still-open bracket is
        // what must hold the suppression here, not the child's afterglow.
        past_the_linger().await;

        // The parent's bracket is still open; the child closing its own must
        // not hand the parent's writes back to the user.
        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::Bracketed
        );
        drop(parent);
        past_the_linger().await;
        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::External
        );
    }

    #[test]
    fn capture_only_claims_the_calling_sessions_roots() {
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("s1", &[PathBuf::from("/one")]);
        tracker.track("s2", &[PathBuf::from("/two")]);

        let _window = tracker.capture("s1");
        assert_eq!(
            tracker.observe(Path::new("/one/a.rs")),
            Ownership::Bracketed
        );
        assert_eq!(tracker.observe(Path::new("/two/a.rs")), Ownership::External);
    }

    #[test]
    fn writes_outside_every_tracked_root_are_untracked() {
        let tracker = tracker_over(Path::new("/repo"));
        assert_eq!(
            tracker.observe(Path::new("/elsewhere/a.rs")),
            Ownership::Untracked
        );
    }

    #[test]
    fn nested_roots_resolve_to_the_longest_match() {
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("s1", &[PathBuf::from("/repo")]);
        tracker.track("s2", &[PathBuf::from("/repo/kiln")]);

        let _window = tracker.capture("s2");
        // The inner root is bracketed, the outer one is not; resolving to the
        // outer root would report the kiln write as a user edit.
        assert_eq!(
            tracker.observe(Path::new("/repo/kiln/note.md")),
            Ownership::Bracketed
        );
        assert_eq!(
            tracker.observe(Path::new("/repo/src/a.rs")),
            Ownership::External
        );
    }

    #[test]
    fn shared_excluded_directories_are_noise_at_any_depth() {
        // These come from `EXCLUDED_DIRS`, which the kiln subsystem owns too.
        // `.git` is the one that must stay: git never reports its own
        // directory as ignored, and every command the ledger runs writes there.
        let tracker = tracker_over(Path::new("/repo"));
        for path in [
            "/repo/.git/index",
            "/repo/node_modules/x/index.js",
            "/repo/web/node_modules/x/index.js",
            "/repo/.crucible/state.json",
        ] {
            assert_eq!(
                tracker.observe(Path::new(path)),
                Ownership::Untracked,
                "{path} should be noise"
            );
        }
    }

    #[test]
    fn build_directories_are_noise_because_git_ignores_them() {
        // This replaces a hardcoded `["target", "dist", "build", ".venv",
        // "__pycache__"]`. That list named two ecosystems and matched only at
        // depth one, so `crates/cli/target` was neither pruned nor noise.
        let root = PathBuf::from("/repo");
        let tracker = tracker_over(&root);
        tracker.set_ignored(
            &root,
            HashSet::from([
                root.join("target"),
                root.join("crates/cli/target"),
                root.join("sub/__pycache__"),
            ]),
        );

        for path in [
            "/repo/target/debug/foo",
            "/repo/crates/cli/target/debug/foo",
            "/repo/sub/__pycache__/m.pyc",
        ] {
            assert_eq!(
                tracker.observe(Path::new(path)),
                Ownership::Untracked,
                "{path} should be noise"
            );
        }
    }

    #[test]
    fn a_source_file_named_like_a_build_dir_is_still_reviewable() {
        // Git decides by path, not by name, so a file called `build` next to
        // an ignored `build/` directory stays reviewable.
        let root = PathBuf::from("/repo");
        let tracker = tracker_over(&root);
        tracker.set_ignored(&root, HashSet::from([root.join("build")]));
        assert_eq!(
            tracker.observe(Path::new("/repo/scripts/build")),
            Ownership::External
        );
    }

    #[test]
    fn editor_scratch_files_are_noise() {
        let tracker = tracker_over(Path::new("/repo"));
        for path in [
            "/repo/src/.main.rs.swp",
            "/repo/src/main.rs~",
            "/repo/src/.#main.rs",
            "/repo/src/#main.rs#",
            "/repo/src/main.rs.tmp",
        ] {
            assert_eq!(
                tracker.observe(Path::new(path)),
                Ownership::Untracked,
                "{path} should be noise"
            );
        }
        assert_eq!(
            tracker.observe(Path::new("/repo/src/main.rs")),
            Ownership::External
        );
    }

    #[tokio::test]
    async fn external_changes_are_broadcast_with_their_sessions() {
        let root = PathBuf::from("/repo");
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("s1", std::slice::from_ref(&root));
        tracker.track("s2", std::slice::from_ref(&root));
        let mut rx = tracker.subscribe();

        tracker.observe(Path::new("/repo/src/main.rs"));

        let change = rx.try_recv().expect("external change broadcast");
        assert_eq!(change.root, root);
        assert_eq!(change.path, PathBuf::from("/repo/src/main.rs"));
        assert_eq!(change.sessions, vec!["s1".to_string(), "s2".to_string()]);
    }

    #[tokio::test]
    async fn repeat_edits_to_one_file_keep_notifying() {
        let tracker = tracker_over(Path::new("/repo"));
        let mut rx = tracker.subscribe();

        tracker.observe(Path::new("/repo/src/main.rs"));
        tracker.observe(Path::new("/repo/src/main.rs"));

        assert!(rx.try_recv().is_ok());
        assert!(
            rx.try_recv().is_ok(),
            "a second edit must still say the composed diff moved"
        );
    }

    /// Proves the whole chain fires: inotify → `WatchManager` → debouncer →
    /// [`ExternalChangeHandler`] → tracker. Every other test in this module
    /// drives `observe` directly, so without this one a broken watch
    /// registration would look exactly like a quiet worktree.
    #[tokio::test]
    async fn a_build_directory_born_mid_session_still_gets_filtered() {
        // `watch_session` runs every turn but only PLANS for roots it has not
        // seen. The ignore set used to be planned with it, so it froze at the
        // first turn: a `target/` created later sits inside a recursive watch
        // (that is how directories created later stay covered) and matched
        // nothing frozen, so `cargo build` flooded the queue unfiltered. The
        // name list that used to catch it by spelling is gone.
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        crate::test_support::init_repo(&root, &[(".gitignore", "target/\n"), ("src/a.rs", "")])
            .await;

        let tracker = Arc::new(ExternalChangeTracker::default());
        let watch = ExternalChangeWatch::start(Arc::clone(&tracker))
            .await
            .unwrap();

        // Turn one: no `target/` exists yet.
        watch
            .watch_session("s1", std::slice::from_ref(&root))
            .await
            .unwrap();
        assert_eq!(
            tracker.observe(&root.join("target/debug/x.o")),
            Ownership::External,
            "nothing has told the tracker about `target/` yet"
        );

        // A build happens.
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join("target/debug/x.o"), "").unwrap();

        // Turn two: same root, already tracked.
        watch
            .watch_session("s1", std::slice::from_ref(&root))
            .await
            .unwrap();
        assert_eq!(
            tracker.observe(&root.join("target/debug/x.o")),
            Ownership::Untracked,
            "a later turn must re-ask git, or build output never gets filtered"
        );
    }

    #[tokio::test]
    async fn a_write_nobody_bracketed_reaches_the_tracker_through_the_watch() {
        let dir = tempfile::TempDir::new().unwrap();
        // notify reports canonical paths; an uncanonicalised root would fail
        // every `starts_with` on a platform where the temp dir is a symlink.
        let root = dir.path().canonicalize().unwrap();

        let tracker = Arc::new(ExternalChangeTracker::default());
        let watch = ExternalChangeWatch::start(Arc::clone(&tracker))
            .await
            .unwrap();
        watch
            .watch_session("s1", std::slice::from_ref(&root))
            .await
            .unwrap();

        let edited = root.join("main.rs");
        tokio::fs::write(&edited, "fn main() {}\n").await.unwrap();

        let observed = tokio::time::timeout(Duration::from_secs(15), async {
            while !tracker.external_paths(&root).contains(&edited) {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await;

        watch.shutdown().await.unwrap();
        assert!(
            observed.is_ok(),
            "a write with no bracket open was never recorded as external"
        );
    }

    #[tokio::test]
    async fn a_shared_root_is_watched_until_the_last_session_lets_go() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();

        let tracker = Arc::new(ExternalChangeTracker::default());
        let watch = ExternalChangeWatch::start(Arc::clone(&tracker))
            .await
            .unwrap();

        // A delegated child shares its parent's workspace by default.
        watch
            .watch_session("parent", std::slice::from_ref(&root))
            .await
            .unwrap();
        watch
            .watch_session("child", std::slice::from_ref(&root))
            .await
            .unwrap();

        watch.unwatch_session("child").await.unwrap();
        assert_eq!(tracker.tracked_roots(), vec![root.clone()]);

        watch.unwatch_session("parent").await.unwrap();
        assert!(tracker.tracked_roots().is_empty());

        watch.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn bracketed_writes_are_never_broadcast() {
        let tracker = tracker_over(Path::new("/repo"));
        let mut rx = tracker.subscribe();

        let _window = tracker.capture("s1");
        tracker.observe(Path::new("/repo/src/main.rs"));

        assert!(rx.try_recv().is_err());
    }
}
