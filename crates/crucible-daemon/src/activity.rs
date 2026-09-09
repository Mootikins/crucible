//! The one answer to "does this daemon still have work to do?"
//!
//! The idle timer ([`crate::server::idle`]) may end the process, so what it
//! counts decides what gets interrupted. It used to count two things by
//! reaching for them one at a time: live socket connections, and the
//! background job manager's running count. Everything else the daemon does is
//! a plain `tokio::spawn` and was invisible — a turn whose client detached
//! (the TUI closes, the daemon deliberately survives it, the turn keeps
//! running), an ACP delegation, a file reprocess, the archive sweep, the
//! startup title catch-up. Thirty minutes later the accept loop broke
//! mid-turn.
//!
//! A list of places to check is what let those five go uncounted, and a list
//! goes stale the next time somebody spawns something. So work reports itself
//! instead: whoever starts it takes a [`WorkGuard`], and the daemon is busy
//! while any guard is alive. Nothing has to remember to add a row here.
//!
//! # What counts
//!
//! Work, not the loops that wait for work. The reprocess watcher, the archive
//! sweep timer and the auto-title watcher run for the daemon's whole life; a
//! guard held by the loop itself would make the daemon immortal, which is the
//! leak this policy exists to stop. Each takes its guard around the *body* it
//! runs when something arrives.
//!
//! Nor does a resident session count. Sessions stay in memory after they end,
//! so any daemon that ever served a turn would never exit again. They are
//! persisted; the next client resumes them.

// [`WorkKind`] is a closed set with a counter behind each variant. An added
// variant must fail to compile until somebody gives it a counter and a name,
// so neither match below carries a wildcard arm. Both lints are needed:
// clippy reports a wildcard covering one remaining variant as
// `match_wildcard_for_single_variants` and only a wildcard over two or more as
// `wildcard_enum_match_arm`.
#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A kind of work that must finish before the daemon may exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum WorkKind {
    /// A client is holding a socket connection.
    Connection,
    /// An agent turn is in flight. The client that asked for it may be long
    /// gone; the turn outlives it on purpose.
    Turn,
    /// A background command (`bash &`) is still running.
    BackgroundJob,
    /// Daemon-owned upkeep is mid-run: reprocessing a changed file, sweeping
    /// stale sessions, opening the startup kilns, titling a session.
    Maintenance,
}

impl WorkKind {
    /// Every kind, for the callers that report on all of them.
    ///
    /// Completeness is proved in the tests by walking `strum::EnumIter` —
    /// the compiler's own list — rather than by reading this array.
    const ALL: [WorkKind; 4] = [
        WorkKind::Connection,
        WorkKind::Turn,
        WorkKind::BackgroundJob,
        WorkKind::Maintenance,
    ];

    /// The name this kind is logged under.
    pub fn label(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Turn => "turn",
            Self::BackgroundJob => "background_job",
            Self::Maintenance => "maintenance",
        }
    }
}

/// How much work this daemon has outstanding, by kind.
///
/// One instance per daemon, shared by everything that starts work. Passed
/// around as an `Arc`: two instances would be two answers.
pub struct DaemonActivity {
    connections: AtomicUsize,
    turns: AtomicUsize,
    background_jobs: AtomicUsize,
    maintenance: AtomicUsize,
}

impl DaemonActivity {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            connections: AtomicUsize::new(0),
            turns: AtomicUsize::new(0),
            background_jobs: AtomicUsize::new(0),
            maintenance: AtomicUsize::new(0),
        })
    }

    /// Claim the daemon until the returned guard drops.
    pub fn start(self: &Arc<Self>, kind: WorkKind) -> WorkGuard {
        self.counter(kind).fetch_add(1, Ordering::SeqCst);
        WorkGuard {
            activity: Arc::clone(self),
            kind,
        }
    }

    /// How many units of work are outstanding right now, of every kind.
    ///
    /// Zero is the daemon's own answer to "nobody needs me"; the idle timer
    /// asks nothing else.
    pub fn outstanding(&self) -> usize {
        WorkKind::ALL
            .iter()
            .map(|kind| self.counter(*kind).load(Ordering::SeqCst))
            .sum()
    }

    /// The kinds outstanding right now, for a log line that says *why* the
    /// daemon is staying.
    pub fn busy_kinds(&self) -> Vec<&'static str> {
        WorkKind::ALL
            .iter()
            .filter(|kind| self.counter(**kind).load(Ordering::SeqCst) > 0)
            .map(|kind| kind.label())
            .collect()
    }

    fn counter(&self, kind: WorkKind) -> &AtomicUsize {
        match kind {
            WorkKind::Connection => &self.connections,
            WorkKind::Turn => &self.turns,
            WorkKind::BackgroundJob => &self.background_jobs,
            WorkKind::Maintenance => &self.maintenance,
        }
    }
}

/// Holds the daemon open for as long as it is alive.
///
/// The count has to fall however the work ends — a clean finish, an error, a
/// cancelled task, a panic — so it is a `Drop` rather than a decrement at the
/// bottom of the work.
#[must_use = "the daemon may exit as soon as this guard drops"]
pub struct WorkGuard {
    activity: Arc<DaemonActivity>,
    kind: WorkKind,
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        self.activity
            .counter(self.kind)
            .fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// Every kind must count, and count as itself. Two variants sharing one
    /// counter would show up here as a kind that stays busy after its own
    /// guard has gone.
    #[test]
    fn every_kind_of_work_is_counted_and_named_on_its_own() {
        let activity = DaemonActivity::new();

        for kind in WorkKind::iter() {
            let guard = activity.start(kind);
            assert_eq!(
                activity.outstanding(),
                1,
                "{} did not raise the outstanding count",
                kind.label()
            );
            assert_eq!(
                activity.busy_kinds(),
                vec![kind.label()],
                "the busy kinds must name {} and nothing else",
                kind.label()
            );
            drop(guard);
            assert_eq!(
                activity.outstanding(),
                0,
                "{} stayed outstanding after its guard dropped",
                kind.label()
            );
        }
    }

    /// `ALL` is what `outstanding` sums over, so a kind missing from it is a
    /// kind the idle timer cannot see. The expectation comes from the
    /// compiler's variant list, not from re-reading the array.
    #[test]
    fn the_all_array_holds_every_variant() {
        let from_the_compiler: Vec<WorkKind> = WorkKind::iter().collect();
        assert_eq!(
            WorkKind::ALL.to_vec(),
            from_the_compiler,
            "a kind missing from ALL is work the idle timer cannot count"
        );
    }

    #[test]
    fn work_of_different_kinds_accumulates() {
        let activity = DaemonActivity::new();

        let turn = activity.start(WorkKind::Turn);
        let job = activity.start(WorkKind::BackgroundJob);
        let second_turn = activity.start(WorkKind::Turn);
        assert_eq!(activity.outstanding(), 3);

        drop(job);
        drop(second_turn);
        assert_eq!(activity.outstanding(), 1);
        assert_eq!(activity.busy_kinds(), vec!["turn"]);

        drop(turn);
        assert_eq!(activity.outstanding(), 0);
        assert!(activity.busy_kinds().is_empty());
    }
}
