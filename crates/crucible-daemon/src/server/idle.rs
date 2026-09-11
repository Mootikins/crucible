//! When an auto-spawned daemon is allowed to exit on its own.
//!
//! A daemon has no parent to reap it. `DaemonClient::connect_or_start` spawns
//! it detached on purpose — it is shared, and the next `cru` command must find
//! it already running — so nothing kills it when the client that started it
//! goes away. Before this module the daemon had no way to exit at all: no idle
//! timer, no signal handler. One box accumulated 91 of them, about 4.5 GB.
//!
//! # The policy
//!
//! The daemon exits after `server.idle_shutdown_minutes` of continuous
//! idleness, where idle means that [`crate::activity::DaemonActivity`] has no
//! outstanding work: no client holding a connection, no turn in flight, no
//! background job, no maintenance mid-run.
//!
//! This module asks that ONE question. It used to ask two — the connection
//! count and the background job count — and everything the daemon spawned
//! outside those two was invisible to it, including an in-flight turn whose
//! client had detached. See `crate::activity` for why work now reports itself.
//!
//! It exits at once, without waiting out the window, when it is idle AND its
//! socket file is gone. Nothing can reach such a daemon ever again — a client
//! finds a daemon only by that path — so the window would decide the same
//! thing later. This is what collects a daemon auto-spawned by a test whose
//! `TempDir` has since been removed.
//!
//! Nothing else counts. In particular a resident session does NOT keep the
//! daemon alive: sessions stay in memory after they end (see
//! `SessionManager::end_session`), so any daemon that ever served a turn would
//! otherwise be immortal — which is the leak. Sessions are persisted, so the
//! next client resumes them; the cost of exiting is a cold start, not work.
//!
//! Two callers deliberately do not arm the timer:
//!
//! - a daemon that runs *inside* another process (`cru --standalone`, the test
//!   servers) dies with its host, and
//! - a daemon with declarative `schedules` is meant to sit there with nobody
//!   attached; exiting would stop the schedules from firing.

use std::time::{Duration, Instant};

/// How often the idle timer looks, and the floor and ceiling on that.
///
/// A quarter of the window bounds the overshoot at 25%. The ceiling keeps a
/// one-day window from sleeping through six hours of idleness; the floor keeps
/// a very short window — which only a test asks for — from waking the daemon
/// continuously.
const PROBE_FRACTION: u32 = 4;
const PROBE_MIN: Duration = Duration::from_secs(1);
const PROBE_MAX: Duration = Duration::from_secs(60);

/// What the daemon knows about its own usefulness at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IdleSnapshot {
    /// Units of work outstanding right now, of every kind
    /// ([`crate::activity::WorkKind`]). One number from one registry: a
    /// second thing to check is exactly what let five kinds of work go
    /// uncounted.
    pub outstanding_work: usize,
    /// Whether the socket this daemon bound is still on the filesystem. A
    /// client finds a daemon only by that path, so `false` means nobody can
    /// ever reach it again.
    pub reachable: bool,
}

impl IdleSnapshot {
    /// Whether nobody needs this daemon right now.
    pub(super) fn is_idle(self) -> bool {
        self.outstanding_work == 0
    }
}

/// How long the daemon has been of use to nobody.
///
/// Time is a parameter rather than something this type reads, so the policy is
/// tested by advancing an `Instant` instead of by sleeping.
pub(super) struct IdleTimer {
    window: Duration,
    /// When the current run of idleness began. `None` while busy.
    idle_since: Option<Instant>,
}

impl IdleTimer {
    /// Arm the timer, counting from `now`.
    ///
    /// A daemon nobody ever connects to is idle from the moment it binds, so
    /// the run starts immediately rather than at the first disconnection.
    pub(super) fn new(window: Duration, now: Instant) -> Self {
        Self {
            window,
            idle_since: Some(now),
        }
    }

    /// How often [`Self::observe`] should be called for this window.
    pub(super) fn probe_period(window: Duration) -> Duration {
        (window / PROBE_FRACTION).clamp(PROBE_MIN, PROBE_MAX)
    }

    /// Record that a client connected: the idle run, if any, is over.
    pub(super) fn note_activity(&mut self) {
        self.idle_since = None;
    }

    /// Answer whether the daemon should exit now.
    pub(super) fn observe(&mut self, now: Instant, snapshot: IdleSnapshot) -> bool {
        if !snapshot.is_idle() {
            self.idle_since = None;
            return false;
        }
        if !snapshot.reachable {
            return true;
        }
        let since = *self.idle_since.get_or_insert(now);
        now.duration_since(since) >= self.window
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{DaemonActivity, WorkKind};
    use std::sync::Arc;

    const WINDOW: Duration = Duration::from_secs(600);

    fn idle() -> IdleSnapshot {
        IdleSnapshot {
            outstanding_work: 0,
            reachable: true,
        }
    }

    /// The snapshot the running daemon builds, from the registry the running
    /// daemon uses. Written this way so the policy tests below break if the
    /// registry stops counting a guard.
    fn snapshot_of(activity: &Arc<DaemonActivity>) -> IdleSnapshot {
        IdleSnapshot {
            outstanding_work: activity.outstanding(),
            reachable: true,
        }
    }

    #[test]
    fn a_daemon_nobody_connects_to_exits_after_the_window() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);

        assert!(!timer.observe(start + WINDOW / 2, idle()));
        assert!(timer.observe(start + WINDOW, idle()));
    }

    #[test]
    fn a_connected_client_keeps_the_daemon_alive_for_ever() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();
        let _client = activity.start(WorkKind::Connection);

        assert!(!timer.observe(start + WINDOW * 10, snapshot_of(&activity)));
        assert!(!timer.observe(start + WINDOW * 100, snapshot_of(&activity)));
    }

    #[test]
    fn a_running_background_job_keeps_the_daemon_alive() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();
        let _job = activity.start(WorkKind::BackgroundJob);

        assert!(!timer.observe(start + WINDOW * 10, snapshot_of(&activity)));
    }

    /// The turn the old snapshot could not see. Nobody is connected: the TUI
    /// that started this turn has gone, and the daemon survives it on purpose.
    #[test]
    fn a_turn_in_flight_keeps_the_daemon_alive_with_no_connections() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();
        let turn = activity.start(WorkKind::Turn);

        assert!(!timer.observe(start + WINDOW * 10, snapshot_of(&activity)));

        // The turn ends, and the window starts from there.
        drop(turn);
        assert!(!timer.observe(start + WINDOW * 10, snapshot_of(&activity)));
        assert!(timer.observe(start + WINDOW * 11, snapshot_of(&activity)));
    }

    /// The same for the upkeep the old snapshot could not see either: a file
    /// reprocess, an archive sweep, a startup title catch-up.
    #[test]
    fn maintenance_in_progress_keeps_the_daemon_alive() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();
        let _sweep = activity.start(WorkKind::Maintenance);

        assert!(!timer.observe(start + WINDOW * 10, snapshot_of(&activity)));
    }

    #[test]
    fn the_window_restarts_after_a_client_disconnects() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();

        // Idle for most of the window, then one client connects and leaves.
        let client = activity.start(WorkKind::Connection);
        assert!(!timer.observe(
            start + WINDOW - Duration::from_secs(1),
            snapshot_of(&activity)
        ));
        drop(client);

        // A full window has now passed since the daemon started, but only a
        // second has passed since it last had a client. It must stay.
        assert!(!timer.observe(start + WINDOW, idle()));
        // And it exits a full window after the disconnection, not before.
        assert!(!timer.observe(start + WINDOW * 2 - Duration::from_secs(2), idle()));
        assert!(timer.observe(start + WINDOW * 2, idle()));
    }

    #[test]
    fn an_accepted_connection_clears_the_idle_run_without_waiting_for_a_probe() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);

        // A client connects and disconnects entirely between two probes, so no
        // snapshot ever sees it. The accept path reports it directly.
        timer.note_activity();

        assert!(!timer.observe(start + WINDOW, idle()));
    }

    #[test]
    fn an_unreachable_daemon_exits_without_waiting_out_the_window() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);

        // The socket is gone — a test's TempDir was removed, say. No client
        // can find this daemon again, so the window would decide the same
        // thing an hour later.
        assert!(timer.observe(
            start,
            IdleSnapshot {
                outstanding_work: 0,
                reachable: false,
            }
        ));
    }

    #[test]
    fn an_unreachable_daemon_still_finishes_what_it_is_doing() {
        let start = Instant::now();
        let mut timer = IdleTimer::new(WINDOW, start);
        let activity = DaemonActivity::new();

        // The socket went away under a client that is still connected, or a
        // turn that is still running. Both outlive the path.
        for kind in [WorkKind::Connection, WorkKind::Turn] {
            let _work = activity.start(kind);
            assert!(!timer.observe(
                start,
                IdleSnapshot {
                    outstanding_work: activity.outstanding(),
                    reachable: false,
                }
            ));
        }
    }

    #[test]
    fn the_probe_period_is_a_quarter_of_the_window_within_bounds() {
        assert_eq!(
            IdleTimer::probe_period(Duration::from_secs(120)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn the_probe_period_is_clamped_at_both_ends() {
        assert_eq!(
            IdleTimer::probe_period(Duration::from_millis(200)),
            PROBE_MIN
        );
        assert_eq!(
            IdleTimer::probe_period(Duration::from_secs(86_400)),
            PROBE_MAX
        );
    }
}
