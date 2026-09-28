//! Frame time of the native-scrollback view with a transcript of about 5,000
//! rows.
//!
//! These are measurements, not gates, so they are ignored. To get numbers
//! that mean something, use an optimized build:
//!
//! ```text
//! CARGO_PROFILE_BENCH_LTO=off CARGO_PROFILE_BENCH_CODEGEN_UNITS=16 \
//!   cargo nextest run --cargo-profile bench -p crucible-cli --lib \
//!   --run-ignored only -E 'test(frame_time_bench)' --no-capture --test-threads 1
//! ```

use super::transcript_fixtures as fixtures;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::render_frame;
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs};
use crucible_oil::focus::FocusContext;
use crucible_oil::TestRuntime;
use std::time::{Duration, Instant};

const WIDTH: u16 = 200;
const HEIGHT: u16 = 60;
/// Exchanges that give a transcript of about 5,000 rows at 200 columns.
const EXCHANGES: usize = 115;
const FRAMES: usize = 300;

fn percentile<T: Copy + Ord>(values: &[T], p: f64) -> T {
    let mut sorted = values.to_vec();
    sorted.sort();
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

fn report(label: &str, times: &[Duration], bytes: &[usize]) {
    println!(
        "{label}: frames={} time median={:?} p99={:?} max={:?} | bytes median={} p99={}",
        times.len(),
        percentile(times, 0.5),
        percentile(times, 0.99),
        percentile(times, 1.0),
        percentile(bytes, 0.5),
        percentile(bytes, 0.99),
    );
}

/// One frame through the production path, bytes included.
fn timed_frame(
    app: &mut OilChatApp,
    runtime: &mut TestRuntime,
    focus: &FocusContext,
) -> (Duration, usize) {
    app.set_frame_time(Instant::now());
    let start = Instant::now();
    render_frame(app, runtime, focus);
    let bytes = runtime.take_bytes().len();
    (start.elapsed(), bytes)
}

#[test]
#[ignore = "requires: manual inspection — a timing measurement; run it with an optimized build, see the module doc"]
fn frame_time_while_streaming_5k_rows() {
    let mut app = fixtures::app_with_exchanges(EXCHANGES);
    let mut feed = EventFeed::default();
    let mut runtime = TestRuntime::new(WIDTH, HEIGHT);
    let focus = FocusContext::new();
    // The first frame lays out the whole transcript; the samples are the
    // steady state after it.
    timed_frame(&mut app, &mut runtime, &focus);
    println!(
        "transcript rows: {}",
        runtime.viewport_content().lines().count()
    );

    app.send_msgs(feed.user(&fixtures::user_text(EXCHANGES)));
    let (mut times, mut bytes) = (Vec::new(), Vec::new());
    for delta in fixtures::stream_deltas(EXCHANGES)
        .into_iter()
        .cycle()
        .take(FRAMES)
    {
        app.send_msgs(feed.text(&delta));
        let (time, written) = timed_frame(&mut app, &mut runtime, &focus);
        times.push(time);
        bytes.push(written);
    }
    report("native stream 200x60", &times, &bytes);
}

/// A width change reprints the whole transcript at the new width.
#[test]
#[ignore = "requires: manual inspection — a timing measurement; run it with an optimized build, see the module doc"]
fn frame_time_after_a_width_change_5k_rows() {
    let mut app = fixtures::app_with_exchanges(EXCHANGES);
    let mut runtime = TestRuntime::new(WIDTH, HEIGHT);
    let focus = FocusContext::new();
    let (mut times, mut bytes) = (Vec::new(), Vec::new());
    for width in [200u16, 199, 160, 120, 200].into_iter().cycle().take(20) {
        runtime.resize(width, HEIGHT);
        let (time, written) = timed_frame(&mut app, &mut runtime, &focus);
        times.push(time);
        bytes.push(written);
    }
    report("native width change", &times, &bytes);
}
