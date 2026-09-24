//! The full-screen prototype without a daemon: two fake chat sessions and a
//! 10,000-line plugin buffer. Use it to check scroll, selection and copy in
//! a real terminal, over SSH and inside Zellij or tmux.
//!
//! ```text
//! cargo run -p crucible-cli --example fullscreen_demo
//! ```
//!
//! Keys: type and press Enter to get a streamed fake answer. PageUp and
//! PageDown or the wheel scroll. Drag, double-click or triple-click to
//! select; the button release copies. F2 turns mouse capture off and on.
//! F3 prints the transcript into the terminal's scrollback. F4 switches
//! panes. Ctrl+Q quits. On exit the demo prints the transcript of the
//! pane on screen, then frame statistics on stderr.

use crossterm::event::{self, Event as CtEvent, KeyCode, KeyModifiers};
use crucible_cli::tui::oil::fullscreen::clipboard::{Copier, CopyEnv};
use crucible_cli::tui::oil::fullscreen::shell::{
    ChatPane, FullscreenShell, PluginBuffer, ShellAction,
};
use crucible_cli::tui::oil::fullscreen::{fixtures, FullscreenView, ViewAction};
use crucible_cli::tui::oil::{theme, ChatAppMsg, Event, FocusContext, ViewContext};
use crucible_oil::terminal::{ScreenMode, Terminal};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

fn main() -> std::io::Result<()> {
    let mut terminal = Terminal::new()?.with_mode(ScreenMode::Fullscreen {
        mouse_capture: true,
    });
    terminal.enter()?;
    let result = run(&mut terminal);
    let (rows, stats) = match result {
        Ok(done) => done,
        Err(error) => {
            terminal.exit()?;
            return Err(error);
        }
    };
    terminal.exit()?;
    for row in rows {
        println!("{row}\x1b[0m");
    }
    eprintln!("{stats}");
    Ok(())
}

fn chat(name: &str, exchanges: usize) -> ChatPane {
    ChatPane {
        name: name.into(),
        app: fixtures::app_with_exchanges(exchanges),
        view: FullscreenView::new(),
    }
}

fn run(terminal: &mut Terminal) -> std::io::Result<(Vec<String>, String)> {
    let buffer = PluginBuffer::new(
        "plugin log",
        || 10_000,
        |i| format!("{i:05}  plugin buffer line with some text, 日本語 and \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"),
    );
    let mut shell = FullscreenShell::new(vec![chat("session A", 60), chat("session B", 3)], Some(buffer));
    let focus = FocusContext::new();
    let mut copier = Copier::default();
    // Deltas still to stream, per chat pane.
    let mut pending: Vec<VecDeque<String>> = vec![VecDeque::new(); shell.chats.len()];
    let mut answer = 1000;
    let (mut times, mut bytes) = (Vec::new(), Vec::new());

    loop {
        for (i, queue) in pending.iter_mut().enumerate() {
            if let Some(delta) = queue.pop_front() {
                shell.chats[i].app.on_message(ChatAppMsg::TextDelta(delta));
                if queue.is_empty() {
                    shell.chats[i].app.on_message(ChatAppMsg::StreamComplete);
                }
            }
        }

        terminal.sync_size()?;
        let start = Instant::now();
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), terminal.size());
        for pane in &mut shell.chats {
            pane.app.set_frame_time(Instant::now());
        }
        let frame = shell.frame(&ctx);
        let stats = terminal.present(&frame.grid, frame.cursor)?;
        if stats.rows_written > 0 {
            times.push(start.elapsed());
            bytes.push(stats.bytes);
        }

        if !event::poll(Duration::from_millis(16))? {
            continue;
        }
        let event = match event::read()? {
            CtEvent::Key(key)
                if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                break;
            }
            CtEvent::Key(key) => Event::Key(key),
            CtEvent::Mouse(mouse) => Event::Mouse(mouse),
            CtEvent::Paste(text) => Event::Paste(text),
            CtEvent::Resize(width, height) => {
                terminal.handle_resize()?;
                Event::Resize { width, height }
            }
            _ => continue,
        };
        match shell.handle_event(&event) {
            ShellAction::Quit => break,
            ShellAction::Sent { pane, .. } => {
                answer += 1;
                pending[pane] = fixtures::stream_deltas(answer).into();
            }
            ShellAction::View(ViewAction::Copy(text)) => {
                let report = copier.copy(&text, CopyEnv::detect(), |sequence| {
                    terminal.write_raw(sequence).map_err(|e| e.to_string())
                });
                shell.note = report.summary(text.chars().count());
            }
            ShellAction::View(ViewAction::Dump(rows)) => {
                terminal.print_to_main_screen(&rows)?;
                shell.note = format!("printed {} rows to the scrollback", rows.len());
            }
            ShellAction::View(ViewAction::ToggleMouse) => {
                let on = !terminal.mouse_captured();
                terminal.set_mouse_capture(on)?;
                shell.note = format!("mouse capture {}", if on { "on" } else { "off" });
            }
            ShellAction::View(_) | ShellAction::None => {}
        }
    }

    let rows = shell
        .active_chat_mut()
        .map(|pane| pane.view.take_dump(true))
        .unwrap_or_default();
    Ok((rows, summary(&mut times, &mut bytes)))
}

fn summary(times: &mut [Duration], bytes: &mut [usize]) -> String {
    if times.is_empty() {
        return "no frames".into();
    }
    times.sort();
    bytes.sort();
    let at = |len: usize, p: f64| ((len - 1) as f64 * p).round() as usize;
    format!(
        "frames={} build+write median={:?} p99={:?} | bytes median={} p99={} total={}",
        times.len(),
        times[at(times.len(), 0.5)],
        times[at(times.len(), 0.99)],
        bytes[at(bytes.len(), 0.5)],
        bytes[at(bytes.len(), 0.99)],
        bytes.iter().sum::<usize>(),
    )
}
