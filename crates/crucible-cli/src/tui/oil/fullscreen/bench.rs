//! Frame time and bytes per frame for the full-screen prototype.
//!
//! These are measurements, not gates, so they are ignored. To get numbers
//! that mean something, use an optimized build:
//!
//! ```text
//! CARGO_PROFILE_BENCH_LTO=off CARGO_PROFILE_BENCH_CODEGEN_UNITS=16 \
//!   cargo nextest run --cargo-profile bench -p crucible-cli --lib \
//!   --run-ignored only -E 'test(fullscreen::bench)' --no-capture
//! ```

use super::fixtures;
use super::FullscreenView;
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::theme;
use crucible_oil::focus::FocusContext;
use crucible_oil::screen::ScreenDiff;
use std::time::{Duration, Instant};

const WIDTH: u16 = 200;
const HEIGHT: u16 = 60;
/// Exchanges that give a transcript of about 5,000 rows at 200 columns.
const EXCHANGES: usize = 115;
const FRAMES: usize = 300;

struct Sample {
    time: Duration,
    bytes: usize,
    rows: usize,
}

fn percentile<T: Copy + Ord>(values: &[T], p: f64) -> T {
    let mut sorted = values.to_vec();
    sorted.sort();
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

fn report(label: &str, samples: &[Sample]) {
    let times: Vec<Duration> = samples.iter().map(|s| s.time).collect();
    let bytes: Vec<usize> = samples.iter().map(|s| s.bytes).collect();
    let rows: Vec<usize> = samples.iter().map(|s| s.rows).collect();
    println!(
        "{label}: frames={} time median={:?} p99={:?} max={:?} | bytes median={} p99={} max={} | rows median={} max={}",
        samples.len(),
        percentile(&times, 0.5),
        percentile(&times, 0.99),
        percentile(&times, 1.0),
        percentile(&bytes, 0.5),
        percentile(&bytes, 0.99),
        percentile(&bytes, 1.0),
        percentile(&rows, 0.5),
        percentile(&rows, 1.0),
    );
}

/// Stream one more answer into `app` and time each frame: build plus the row
/// diff into a byte buffer.
fn stream_frames(app: &mut OilChatApp, view: &mut FullscreenView) -> Vec<Sample> {
    let focus = FocusContext::new();
    let mut diff = ScreenDiff::new();
    let mut out: Vec<u8> = Vec::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (WIDTH, HEIGHT));
    // The first frame lays out the whole transcript; the samples are the
    // steady state after it.
    app.set_frame_time(Instant::now());
    let first = view.frame(app, &ctx);
    diff.present(&mut out, &first.grid, first.cursor).unwrap();

    app.on_message(ChatAppMsg::UserMessage(fixtures::user_text(EXCHANGES)));
    let mut samples = Vec::new();
    for delta in fixtures::stream_deltas(EXCHANGES)
        .into_iter()
        .cycle()
        .take(FRAMES)
    {
        app.on_message(ChatAppMsg::TextDelta(delta));
        app.set_frame_time(Instant::now());
        out.clear();
        let start = Instant::now();
        let frame = view.frame(app, &ctx);
        let stats = diff.present(&mut out, &frame.grid, frame.cursor).unwrap();
        samples.push(Sample {
            time: start.elapsed(),
            bytes: stats.bytes,
            rows: stats.rows_written,
        });
    }
    samples
}

#[test]
#[ignore = "measurement: run with an optimized build, see the module doc"]
fn frame_time_while_streaming_5k_rows() {
    let mut app = fixtures::app_with_exchanges(EXCHANGES);
    let mut view = FullscreenView::new();
    let samples = stream_frames(&mut app, &mut view);
    println!("transcript rows: {}", view.transcript().len());
    report("stream 200x60", &samples);
}

/// A full relayout: the first frame, and every frame after a width change.
#[test]
#[ignore = "measurement: run with an optimized build, see the module doc"]
fn frame_time_of_a_full_relayout_5k_rows() {
    let app = fixtures::app_with_exchanges(EXCHANGES);
    let focus = FocusContext::new();
    let mut view = FullscreenView::new();
    let mut samples = Vec::new();
    for (i, width) in [200u16, 199, 160, 120, 200].into_iter().cycle().take(20).enumerate() {
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, HEIGHT));
        let start = Instant::now();
        let frame = view.frame(&app, &ctx);
        let time = start.elapsed();
        if i == 0 {
            println!("transcript rows at {width}: {}", view.transcript().len());
        }
        samples.push(Sample {
            time,
            bytes: 0,
            rows: frame.grid.height(),
        });
    }
    report("relayout on width change", &samples);
}
