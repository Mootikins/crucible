//! The one owner of kiln index updates.
//!
//! Each change to a kiln file reaches the index through one ordered queue
//! that drops nothing ([`crate::lossless_queue`]):
//!
//! - the watcher, through the kiln's bridge (`file_watch_bridge.rs`);
//! - each daemon write, through [`landed`], which `file_write` calls under the
//!   file's write lock (the note tools, `fs.write` and so the web, and a
//!   proposal accept);
//! - a folder move by `fs.move`, through [`KilnManager::folder_moved`].
//!
//! One task ([`KilnManager::run_index_jobs`]) applies the jobs in order. A job
//! reads the disk when it runs, so a late or repeated job is harmless: the
//! pipeline's hash check skips a file that did not change, and a missing file
//! leaves the index. The index used to follow the client bus, which drops
//! events for a slow receiver, and a daemon write reached it only through the
//! watcher's echo half a second later.
//!
//! A daemon change is announced by this owner after the index has it
//! (`file_changed`, `file_deleted`, `file_moved`). The watcher then reports
//! the same change again; the bridge drops that echo (see [`IndexQueue`]), so
//! a Lua `FileChanged` handler runs once for one write.

use super::{canonical_or_self, is_excluded_dir, is_indexable_kiln_file, KilnManager};
use crate::activity::{DaemonActivity, WorkKind};
use crate::lossless_queue;
use crucible_core::events::{FileChangeKind, InternalSessionEvent};
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Who made a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeOrigin {
    /// The daemon wrote or moved the file. The owner announces the change,
    /// because the bridge drops the watcher's echo of it.
    Daemon,
    /// The watcher saw the change, and its bridge announced it already.
    Watcher,
}

/// One change for the index owner.
#[derive(Debug, Clone)]
pub(crate) enum IndexJob {
    /// The file at `path` was created or written.
    Changed {
        path: PathBuf,
        kind: FileChangeKind,
        origin: ChangeOrigin,
    },
    /// The file at `path` was removed.
    Deleted { path: PathBuf, origin: ChangeOrigin },
    /// A file or a directory moved from `from` to `to`.
    Moved {
        from: PathBuf,
        to: PathBuf,
        origin: ChangeOrigin,
    },
    /// The watcher of `kiln` lost events. Read the kiln again.
    Rescan { kiln: PathBuf },
}

/// How long a mark of a daemon change drops the watcher's report of it.
///
/// The watcher reports a change after two debounce stages of 500 ms each.
/// The window is several times that, so a loaded machine still drops the
/// echo. A report inside the window that matches the mark is the same bytes
/// the daemon already announced, so dropping it loses nothing.
const ECHO_WINDOW: Duration = Duration::from_secs(5);

/// A daemon change that the watcher will report again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Echo {
    Written { hash: String },
    Removed,
    MovedFrom(PathBuf),
}

/// The queue into one index owner, and the marks of the daemon's own changes.
pub(crate) struct IndexQueue {
    jobs: lossless_queue::Sender<IndexJob>,
    marks: DashMap<PathBuf, (Echo, Instant)>,
}

impl IndexQueue {
    pub(crate) fn new(jobs: lossless_queue::Sender<IndexJob>) -> Arc<Self> {
        let queue = Arc::new(Self {
            jobs,
            marks: DashMap::new(),
        });
        let mut queues = QUEUES.write().unwrap_or_else(PoisonError::into_inner);
        queues.retain(|queue| queue.strong_count() > 0);
        queues.push(Arc::downgrade(&queue));
        queue
    }

    pub(crate) fn push(&self, job: IndexJob) {
        if !self.jobs.send(job) {
            debug!("The index owner stopped; a file change was not queued");
        }
    }

    fn mark(&self, path: &Path, echo: Echo) {
        let now = Instant::now();
        // Old marks go when a new one comes, so the map holds only the
        // paths of the last few seconds.
        self.marks
            .retain(|_, (_, at)| now.duration_since(*at) < ECHO_WINDOW);
        self.marks.insert(echo_key(path), (echo, now));
    }

    fn marked(&self, path: &Path) -> Option<Echo> {
        let mark = self.marks.get(&echo_key(path))?;
        let (echo, at) = mark.value();
        (at.elapsed() < ECHO_WINDOW).then(|| echo.clone())
    }

    /// Whether the watcher's report of a change at `path` repeats a daemon
    /// write. It does when the file now holds the bytes of the marked write.
    pub(crate) async fn is_echo_of_write(&self, path: &Path) -> bool {
        let Some(Echo::Written { hash }) = self.marked(path) else {
            return false;
        };
        tokio::fs::read_to_string(path)
            .await
            .is_ok_and(|text| crucible_core::note_edit::disk_hash(&text) == hash)
    }

    /// Whether the watcher's report that `path` is gone repeats a daemon
    /// removal.
    pub(crate) fn is_echo_of_removal(&self, path: &Path) -> bool {
        self.marked(path) == Some(Echo::Removed) && !path.exists()
    }

    /// Whether the watcher's report of a move repeats a daemon move.
    pub(crate) fn is_echo_of_move(&self, from: &Path, to: &Path) -> bool {
        self.marked(to) == Some(Echo::MovedFrom(echo_key(from)))
    }
}

/// The key of a mark: the parent resolved, the name kept. The daemon and the
/// watcher can spell one file two ways, and a removed file has no canonical
/// form of its own.
fn echo_key(path: &Path) -> PathBuf {
    path.parent()
        .and_then(|parent| parent.canonicalize().ok())
        .zip(path.file_name())
        .map(|(parent, name)| parent.join(name))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Every index owner in the process.
///
/// `file_write` has no daemon context: the note tools, `fs.write` and a
/// proposal accept each reach it with only a path. A registry lets the one
/// write function signal every write, so no writer can forget to. Weak, so a
/// dropped daemon leaves no queue behind. One entry per daemon, so the scan is
/// cheap; each owner ignores a path outside its open kilns.
static QUEUES: RwLock<Vec<Weak<IndexQueue>>> = RwLock::new(Vec::new());

/// What a daemon write did to a file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Landed<'a> {
    /// The file now holds text that hashes to `hash`.
    Written { hash: &'a str, created: bool },
    /// The file is gone.
    Removed,
}

/// Tell each index owner that a daemon write changed `path`.
///
/// Called by the writer while it holds the file's write lock, so the jobs of
/// two writes to one file are queued in the order of the writes.
pub(crate) fn landed(path: &Path, what: Landed<'_>) {
    let queues: Vec<Arc<IndexQueue>> = QUEUES
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect();
    for queue in queues {
        let job = match what {
            Landed::Written { hash, created } => {
                queue.mark(
                    path,
                    Echo::Written {
                        hash: hash.to_string(),
                    },
                );
                IndexJob::Changed {
                    path: path.to_path_buf(),
                    kind: if created {
                        FileChangeKind::Created
                    } else {
                        FileChangeKind::Modified
                    },
                    origin: ChangeOrigin::Daemon,
                }
            }
            Landed::Removed => {
                queue.mark(path, Echo::Removed);
                IndexJob::Deleted {
                    path: path.to_path_buf(),
                    origin: ChangeOrigin::Daemon,
                }
            }
        };
        queue.push(job);
    }
}

impl KilnManager {
    /// The jobs of this manager's index owner, once. The daemon gives them
    /// to [`Self::run_index_jobs`]; a manager without a bus has none.
    pub(crate) fn take_index_jobs(&self) -> Option<lossless_queue::Receiver<IndexJob>> {
        self.index_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Wait until the index owner applied each change queued before the
    /// call. A reader that must see a write it just made calls this first.
    pub async fn settle_index(&self) {
        self.index_waiter.wait().await;
    }

    /// `fs.move` moved `from` to `to` inside an open kiln. The index follows,
    /// for a folder too: the watcher reports a folder move as one event for
    /// the folder, and nothing for the notes under it.
    pub(crate) fn folder_moved(&self, from: &Path, to: &Path) {
        let Some(queue) = self.index.as_ref() else {
            return;
        };
        queue.mark(to, Echo::MovedFrom(echo_key(from)));
        queue.push(IndexJob::Moved {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            origin: ChangeOrigin::Daemon,
        });
    }

    /// Apply index jobs until `cancel` fires or every sender is gone.
    ///
    /// The loop is not work: it waits for the daemon's whole life, and a
    /// guard held here would keep the daemon alive. Each job takes its own.
    pub(crate) async fn run_index_jobs(
        self: Arc<Self>,
        mut jobs: lossless_queue::Receiver<IndexJob>,
        activity: Arc<DaemonActivity>,
        cancel: CancellationToken,
    ) {
        loop {
            let job = tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                job = jobs.recv() => match job {
                    Some(job) => job,
                    None => break,
                },
            };
            let _working = activity.start(WorkKind::Maintenance);
            self.apply(&job).await;
            // `job` drops here, after the index has the change, so a waiter
            // sees it.
        }
    }

    async fn apply(&self, job: &IndexJob) {
        match job {
            IndexJob::Changed { path, kind, origin } => {
                let Some(kiln) = self.find_kiln_for_path(path).await else {
                    debug!(path = %path.display(), "Changed file is in no open kiln");
                    return;
                };
                self.sync(&kiln, path).await;
                if *origin == ChangeOrigin::Daemon {
                    self.announce_file(&kiln, path, || InternalSessionEvent::FileChanged {
                        path: path.clone(),
                        kind: *kind,
                    });
                }
            }
            IndexJob::Deleted { path, origin } => {
                let Some(kiln) = self.find_kiln_for_path(path).await else {
                    debug!(path = %path.display(), "Deleted file is in no open kiln");
                    return;
                };
                self.sync(&kiln, path).await;
                if *origin == ChangeOrigin::Daemon {
                    self.announce_file(&kiln, path, || InternalSessionEvent::FileDeleted {
                        path: path.clone(),
                    });
                }
            }
            IndexJob::Moved { from, to, origin } => {
                let Some(kiln) = self.find_kiln_for_path(to).await else {
                    debug!(to = %to.display(), "Moved file is in no open kiln");
                    return;
                };
                if to.is_dir() {
                    self.sync_moved_folder(&kiln, from, to).await;
                    return;
                }
                self.sync(&kiln, from).await;
                self.sync(&kiln, to).await;
                if *origin == ChangeOrigin::Daemon {
                    self.announce_file(&kiln, to, || InternalSessionEvent::FileMoved {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
            }
            IndexJob::Rescan { kiln } => {
                warn!(kiln = %kiln.display(), "The kiln watcher lost events; indexing the kiln again");
                if let Err(e) = self.open_and_process(kiln, false).await {
                    warn!(kiln = %kiln.display(), error = %e, "The rescan failed");
                }
            }
        }
    }

    /// Make the index row of `path` match the disk: index a file that is
    /// there, and drop a row whose file is gone.
    async fn sync(&self, kiln: &Path, path: &Path) {
        if !in_watched_set(kiln, path) {
            return;
        }
        let result = if path.is_file() {
            self.process_file(kiln, path).await.map(|indexed| {
                if indexed {
                    info!(path = %path.display(), "Reprocessed changed file");
                }
            })
        } else {
            // A row that was not there is no news: a rename by the daemon
            // already moved it, and the watcher then reports the old path.
            self.drop_note(kiln, path, false).await.map(|_| ())
        };
        if let Err(e) = result {
            warn!(path = %path.display(), error = %e, "Failed to update the index for a changed file");
        }
    }

    /// Move the rows of every file under a moved folder.
    async fn sync_moved_folder(&self, kiln: &Path, from: &Path, to: &Path) {
        for file in super::discover_indexable_files(to) {
            let Ok(rel) = file.strip_prefix(to) else {
                continue;
            };
            self.sync(kiln, &from.join(rel)).await;
            self.sync(kiln, &file).await;
        }
    }

    /// Announce a daemon change on the bus, as the watcher would have.
    fn announce_file(
        &self,
        kiln: &Path,
        path: &Path,
        event: impl FnOnce() -> InternalSessionEvent,
    ) {
        let Some(tx) = self.event_tx.as_ref() else {
            return;
        };
        if !in_watched_set(kiln, path) {
            return;
        }
        tx.emit(crate::event_map::message_for(&event()));
    }
}

/// Whether the kiln watcher would report `path`: an indexable file inside
/// `kiln` and outside its excluded directories (`.crucible`, `.git`, …). A
/// daemon write outside this set neither enters the index nor is announced,
/// the same answer the watcher's filter gives.
fn in_watched_set(kiln: &Path, path: &Path) -> bool {
    let canonical_kiln = canonical_or_self(kiln);
    let inside = path
        .strip_prefix(&canonical_kiln)
        .or_else(|_| path.strip_prefix(kiln))
        .is_ok_and(|rel| !rel.ancestors().any(is_excluded_dir));
    inside && is_indexable_kiln_file(path)
}
