//! Copy for the full-screen mode: OSC 52, then the native clipboard, then
//! tmux.
//!
//! OSC 52 goes first because it is the one path that reaches the user's own
//! clipboard over SSH, and Zellij passes it on to the outer terminal. The
//! terminal never answers an OSC 52 write, so a sent request is not proof of
//! a copy, and the chain goes on:
//!
//! - The native clipboard (`arboard`) runs only outside SSH. Over SSH it
//!   would write the remote machine's clipboard, which the user never sees.
//! - tmux gets the text through `tmux load-buffer -w -` when the process
//!   runs inside tmux, because tmux drops a bare OSC 52 unless its
//!   `set-clipboard` option allows it.
//!
//! On X11 and some Wayland compositors the process that owns the clipboard
//! must stay alive until the user pastes, so [`Copier`] keeps the native
//! handle.

use base64::Engine;

/// The biggest text that OSC 52 carries. Terminals reject longer payloads,
/// some by dropping them, some by hanging on the parse.
pub const OSC52_MAX_BYTES: usize = 100_000;

/// Where the process runs, for the choice of copy paths.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyEnv {
    pub ssh: bool,
    pub tmux: bool,
}

impl CopyEnv {
    /// Read the environment of this process.
    pub fn detect() -> Self {
        let set = |name: &str| std::env::var_os(name).is_some();
        Self {
            ssh: set("SSH_TTY") || set("SSH_CONNECTION"),
            tmux: set("TMUX"),
        }
    }
}

/// One copy path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Osc52,
    Native,
    Tmux,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Self::Osc52 => "OSC 52",
            Self::Native => "clipboard",
            Self::Tmux => "tmux",
        }
    }
}

/// What each path did with one copy.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CopyReport {
    pub attempts: Vec<(Backend, Result<(), String>)>,
}

impl CopyReport {
    pub fn any_ok(&self) -> bool {
        self.attempts.iter().any(|(_, r)| r.is_ok())
    }

    /// A one-line summary for a toast: "Copied 42 chars (OSC 52, tmux)".
    pub fn summary(&self, chars: usize) -> String {
        let ok: Vec<&str> = self
            .attempts
            .iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(b, _)| b.label())
            .collect();
        if ok.is_empty() {
            let errors: Vec<String> = self
                .attempts
                .iter()
                .filter_map(|(b, r)| r.as_ref().err().map(|e| format!("{}: {e}", b.label())))
                .collect();
            format!("Copy failed ({})", errors.join("; "))
        } else {
            format!("Copied {chars} chars ({})", ok.join(", "))
        }
    }
}

/// The OSC 52 sequence that sets the clipboard to `text`. Inside tmux the
/// sequence goes in a DCS passthrough, so tmux hands it to the outer
/// terminal.
pub fn osc52_sequence(text: &str, tmux: bool) -> Result<String, String> {
    if text.len() > OSC52_MAX_BYTES {
        return Err(format!(
            "{} bytes is more than the OSC 52 limit of {OSC52_MAX_BYTES}",
            text.len()
        ));
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(text);
    let osc = format!("\x1b]52;c;{payload}\x1b\\");
    Ok(if tmux {
        format!("\x1bPtmux;{}\x1b\\", osc.replace('\x1b', "\x1b\x1b"))
    } else {
        osc
    })
}

/// Run the copy chain with the paths as closures, so a test can see the
/// order and the choices without a clipboard or a terminal.
pub fn copy_with(
    text: &str,
    env: CopyEnv,
    osc52: impl FnOnce(&str) -> Result<(), String>,
    native: impl FnOnce(&str) -> Result<(), String>,
    tmux: impl FnOnce(&str) -> Result<(), String>,
) -> CopyReport {
    let mut report = CopyReport::default();
    if text.is_empty() {
        return report;
    }
    report.attempts.push((
        Backend::Osc52,
        osc52_sequence(text, env.tmux).and_then(|s| osc52(&s)),
    ));
    if !env.ssh {
        report.attempts.push((Backend::Native, native(text)));
    }
    if env.tmux {
        report.attempts.push((Backend::Tmux, tmux(text)));
    }
    report
}

/// The copy chain with the real paths. It keeps the native clipboard handle
/// alive; see the module doc.
#[derive(Default)]
pub struct Copier {
    native: Option<arboard::Clipboard>,
}

impl Copier {
    /// Copy `text`. `write_terminal` sends the OSC 52 bytes to the terminal.
    pub fn copy(
        &mut self,
        text: &str,
        env: CopyEnv,
        write_terminal: impl FnOnce(&str) -> Result<(), String>,
    ) -> CopyReport {
        let native = &mut self.native;
        copy_with(
            text,
            env,
            write_terminal,
            |t| native_copy(native, t),
            tmux_copy,
        )
    }
}

fn native_copy(slot: &mut Option<arboard::Clipboard>, text: &str) -> Result<(), String> {
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    let clipboard = slot.as_mut().ok_or("no clipboard")?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

fn tmux_copy(text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("tmux")
        .args(["load-buffer", "-w", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start tmux: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin for tmux")?
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Run the chain with recording paths; each returns `ok`.
    fn run(text: &str, env: CopyEnv, ok: [bool; 3]) -> (CopyReport, Vec<String>) {
        let calls = RefCell::new(Vec::new());
        let result = |name: &str, i: usize, arg: &str| {
            calls.borrow_mut().push(format!("{name}:{arg}"));
            if ok[i] {
                Ok(())
            } else {
                Err(format!("{name} failed"))
            }
        };
        let report = copy_with(
            text,
            env,
            |s| result("osc52", 0, s),
            |s| result("native", 1, s),
            |s| result("tmux", 2, s),
        );
        (report, calls.into_inner())
    }

    #[test]
    fn osc52_encodes_the_text_in_base64() {
        assert_eq!(
            osc52_sequence("hi 日本", false).unwrap(),
            "\x1b]52;c;aGkg5pel5pys\x1b\\"
        );
    }

    #[test]
    fn osc52_inside_tmux_goes_through_a_passthrough() {
        let seq = osc52_sequence("hi", true).unwrap();
        assert_eq!(seq, "\x1bPtmux;\x1b\x1b]52;c;aGk=\x1b\x1b\\\x1b\\");
    }

    #[test]
    fn osc52_refuses_a_payload_over_the_limit() {
        assert!(osc52_sequence(&"x".repeat(OSC52_MAX_BYTES + 1), false).is_err());
    }

    #[test]
    fn a_local_copy_sends_osc52_first_then_the_native_clipboard() {
        let (report, calls) = run("text", CopyEnv::default(), [true, true, true]);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].starts_with("osc52:\x1b]52;c;"));
        assert_eq!(calls[1], "native:text");
        assert_eq!(report.summary(4), "Copied 4 chars (OSC 52, clipboard)");
    }

    #[test]
    fn over_ssh_the_native_clipboard_is_skipped() {
        let env = CopyEnv {
            ssh: true,
            tmux: false,
        };
        let (report, calls) = run("text", env, [true, true, true]);
        assert_eq!(calls.len(), 1);
        assert_eq!(report.attempts.len(), 1);
        assert_eq!(report.attempts[0].0, Backend::Osc52);
    }

    #[test]
    fn inside_tmux_the_chain_ends_with_a_tmux_buffer() {
        let env = CopyEnv {
            ssh: true,
            tmux: true,
        };
        let (_, calls) = run("text", env, [true, true, true]);
        assert!(calls[0].starts_with("osc52:\x1bPtmux;"));
        assert_eq!(calls[1], "tmux:text");
    }

    #[test]
    fn a_failed_path_does_not_stop_the_next_one() {
        let (report, calls) = run("text", CopyEnv::default(), [false, true, true]);
        assert_eq!(calls.len(), 2);
        assert!(report.any_ok());
        assert_eq!(report.summary(4), "Copied 4 chars (clipboard)");
    }

    #[test]
    fn a_copy_that_fails_everywhere_says_why() {
        let (report, _) = run("text", CopyEnv::default(), [false, false, false]);
        assert!(!report.any_ok());
        assert_eq!(
            report.summary(4),
            "Copy failed (OSC 52: osc52 failed; clipboard: native failed)"
        );
    }

    #[test]
    fn an_empty_text_copies_nothing() {
        let (report, calls) = run("", CopyEnv::default(), [true, true, true]);
        assert!(calls.is_empty() && report.attempts.is_empty());
    }
}
