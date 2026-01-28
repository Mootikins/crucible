use crate::tui::oil::node::{col, row, spacer, styled, text, Node};
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::colors;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellStatus {
    Running,
    Completed { exit_code: i32 },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ShellModalMsg {
    Key(KeyEvent),
    Tick,
}

#[derive(Debug, Clone)]
pub enum ShellModalOutput {
    None,
    Close(ShellHistoryItem),
    InsertOutput { content: String, truncated: bool },
}

#[derive(Debug, Clone)]
pub struct ShellHistoryItem {
    pub command: String,
    pub exit_code: i32,
    pub output_tail: Vec<String>,
    pub output_path: Option<PathBuf>,
}

pub struct ShellModal {
    command: String,
    output_lines: Vec<String>,
    status: ShellStatus,
    scroll_offset: usize,
    user_scrolled: bool,
    start_time: Instant,
    duration: Option<Duration>,
    output_path: Option<PathBuf>,
    working_dir: PathBuf,
    output_receiver: Option<Receiver<String>>,
    child_pid: Option<u32>,
    pending_insert: Option<bool>,
}

impl ShellModal {
    pub fn spawn(command: String, working_dir: PathBuf) -> Result<Self, String> {
        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

        let mut modal = Self {
            command: command.clone(),
            output_lines: Vec::new(),
            status: ShellStatus::Running,
            scroll_offset: 0,
            user_scrolled: false,
            start_time: Instant::now(),
            duration: None,
            output_path: None,
            working_dir: working_dir.clone(),
            output_receiver: Some(rx),
            child_pid: None,
            pending_insert: None,
        };

        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

        match Command::new(shell)
            .arg(shell_arg)
            .arg(&command)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                modal.child_pid = Some(child.id());

                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                std::thread::spawn(move || {
                    Self::stream_output(stdout, stderr, tx, child);
                });

                Ok(modal)
            }
            Err(e) => Err(format!("Failed to execute command: {}", e)),
        }
    }

    fn stream_output(
        stdout: Option<std::process::ChildStdout>,
        stderr: Option<std::process::ChildStderr>,
        tx: Sender<String>,
        mut child: Child,
    ) {
        use std::io::BufReader;

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        let stdout_handle = stdout.map(|out| {
            std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stdout.send(line).is_err() {
                        break;
                    }
                }
            })
        });

        let stderr_handle = stderr.map(|err| {
            std::thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stderr.send(format!("\x1b[31m{}\x1b[0m", line)).is_err() {
                        break;
                    }
                }
            })
        });

        if let Some(h) = stdout_handle {
            let _ = h.join();
        }
        if let Some(h) = stderr_handle {
            let _ = h.join();
        }

        let exit_code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        let _ = tx.send(format!("\x00EXIT:{}", exit_code));
    }

    pub fn update(&mut self, msg: ShellModalMsg, visible_lines: usize) -> ShellModalOutput {
        match msg {
            ShellModalMsg::Tick => {
                self.poll_output(visible_lines);

                if let Some(truncated) = self.pending_insert.take() {
                    return ShellModalOutput::InsertOutput {
                        content: self.format_output_for_insert(truncated),
                        truncated,
                    };
                }

                ShellModalOutput::None
            }
            ShellModalMsg::Key(key) => self.handle_key(key, visible_lines),
        }
    }

    fn handle_key(&mut self, key: KeyEvent, visible_lines: usize) -> ShellModalOutput {
        let is_running = self.is_running();
        let half_page = visible_lines / 2;

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if is_running {
                    self.cancel();
                }
                ShellModalOutput::None
            }
            KeyCode::Esc | KeyCode::Char('q') if !is_running => self.close(),
            KeyCode::Char('i') if !is_running => {
                self.pending_insert = Some(false);
                self.close()
            }
            KeyCode::Char('t') if !is_running => {
                self.pending_insert = Some(true);
                self.close()
            }
            KeyCode::Char('e') if !is_running => {
                self.open_in_editor();
                ShellModalOutput::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up(1);
                ShellModalOutput::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down(1, visible_lines);
                ShellModalOutput::None
            }
            KeyCode::Char('u') => {
                self.scroll_up(half_page);
                ShellModalOutput::None
            }
            KeyCode::Char('d') => {
                self.scroll_down(half_page, visible_lines);
                ShellModalOutput::None
            }
            KeyCode::PageUp => {
                self.scroll_up(visible_lines);
                ShellModalOutput::None
            }
            KeyCode::PageDown => {
                self.scroll_down(visible_lines, visible_lines);
                ShellModalOutput::None
            }
            KeyCode::Char('g') if !is_running => {
                self.scroll_to_top();
                ShellModalOutput::None
            }
            KeyCode::Char('G') if !is_running => {
                self.scroll_to_bottom(visible_lines);
                ShellModalOutput::None
            }
            _ => ShellModalOutput::None,
        }
    }

    fn poll_output(&mut self, content_height: usize) {
        let was_running = self.is_running();

        if let Some(ref rx) = self.output_receiver {
            while let Ok(line) = rx.try_recv() {
                if let Some(code_str) = line.strip_prefix("\x00EXIT:") {
                    if let Ok(code) = code_str.parse::<i32>() {
                        if matches!(self.status, ShellStatus::Running) {
                            self.status = ShellStatus::Completed { exit_code: code };
                            self.duration = Some(self.start_time.elapsed());
                        }
                    }
                } else {
                    self.output_lines.push(line);
                }
            }
        }

        if was_running && self.is_running() && !self.user_scrolled {
            self.scroll_to_bottom(content_height);
        } else if was_running && !self.is_running() {
            self.scroll_to_top();
        }
    }

    fn close(&self) -> ShellModalOutput {
        let exit_code = match self.status {
            ShellStatus::Completed { exit_code } => exit_code,
            ShellStatus::Cancelled => -1,
            ShellStatus::Running => -1,
        };

        let output_tail: Vec<String> = self
            .output_lines
            .iter()
            .rev()
            .take(5)
            .rev()
            .cloned()
            .collect();

        ShellModalOutput::Close(ShellHistoryItem {
            command: self.command.clone(),
            exit_code,
            output_tail,
            output_path: self.output_path.clone(),
        })
    }

    fn open_in_editor(&self) {
        if self.output_lines.is_empty() {
            return;
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "less".to_string());
        let content = self.output_lines.join("\n");

        let tmp_path =
            std::env::temp_dir().join(format!("crucible-shell-{}.txt", std::process::id()));
        if std::fs::write(&tmp_path, &content).is_err() {
            return;
        }

        let _ = Command::new(&editor).arg(&tmp_path).status();
        let _ = std::fs::remove_file(&tmp_path);
    }

    fn format_output_for_insert(&self, truncated: bool) -> String {
        let lines = if truncated {
            self.output_lines
                .iter()
                .rev()
                .take(20)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
        } else {
            self.output_lines.clone()
        };

        let mut content = format!("$ {}\n", self.command);
        for line in lines {
            content.push_str(&line);
            content.push('\n');
        }
        content
    }

    pub fn save_output(&mut self, session_dir: &PathBuf) -> Option<PathBuf> {
        let shell_dir = session_dir.join("shell");
        if std::fs::create_dir_all(&shell_dir).is_err() {
            return None;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let cmd_slug: String = self
            .command
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.output", timestamp, cmd_slug);
        let path = shell_dir.join(&filename);

        let mut content = String::new();
        content.push_str(&format!("$ {}\n", self.command));
        content.push_str(&format!(
            "Exit: {}\n",
            match &self.status {
                ShellStatus::Completed { exit_code } => exit_code.to_string(),
                ShellStatus::Cancelled => "cancelled".to_string(),
                ShellStatus::Running => "running".to_string(),
            }
        ));
        if let Some(duration) = self.duration {
            content.push_str(&format!("Duration: {:.2?}\n", duration));
        }
        content.push_str(&format!("Cwd: {}\n", self.working_dir.display()));
        content.push_str("---\n");
        for line in &self.output_lines {
            content.push_str(line);
            content.push('\n');
        }

        if std::fs::write(&path, &content).is_err() {
            return None;
        }

        self.output_path = Some(path.clone());
        Some(path)
    }

    pub fn view(&self, term_width: usize, term_height: usize) -> Node {
        let content_height = term_height.saturating_sub(2);

        let header_bg = colors::POPUP_BG;
        let footer_bg = colors::INPUT_BG;

        let header_text = format!(" {} ", self.format_header());
        let header_padding = " ".repeat(term_width.saturating_sub(header_text.len()));
        let header = styled(
            format!("{}{}", header_text, header_padding),
            Style::new().bg(header_bg).bold(),
        );

        let visible = self.get_visible_lines(content_height);
        let body_lines: Vec<Node> = visible.iter().map(|line| text(line.clone())).collect();
        let body = col(body_lines);

        let footer = self.render_footer(term_width, footer_bg);

        col([header, body, spacer(), footer])
    }

    fn render_footer(&self, width: usize, bg: Color) -> Node {
        let line_info = format!("({} lines)", self.output_lines.len());
        let key_style = Style::new().bg(bg).fg(colors::TEXT_ACCENT);
        let sep_style = Style::new().bg(bg).fg(colors::TEXT_MUTED);
        let text_style = Style::new().bg(bg).fg(colors::TEXT_PRIMARY).dim();

        let content = if self.is_running() {
            row([
                styled(" ", text_style),
                styled("Ctrl+C", key_style),
                styled(" cancel  ", text_style),
                styled(&line_info, sep_style),
            ])
        } else {
            row([
                styled(" ", text_style),
                styled("i", key_style),
                styled(" insert ", text_style),
                styled("│", sep_style),
                styled(" ", text_style),
                styled("t", key_style),
                styled(" truncated ", text_style),
                styled("│", sep_style),
                styled(" ", text_style),
                styled("e", key_style),
                styled(" edit ", text_style),
                styled("│", sep_style),
                styled(" ", text_style),
                styled("q", key_style),
                styled(" quit  ", text_style),
                styled(&line_info, sep_style),
            ])
        };

        let content_str = self.format_footer_text();
        let padding_len = width.saturating_sub(content_str.len() + 1);
        let padding = styled(" ".repeat(padding_len), Style::new().bg(bg));

        row([content, padding])
    }

    #[cfg(test)]
    pub fn visible_lines(&self, max_lines: usize) -> &[String] {
        self.get_visible_lines(max_lines)
    }

    fn get_visible_lines(&self, max_lines: usize) -> &[String] {
        let total = self.output_lines.len();
        if total <= max_lines {
            &self.output_lines
        } else {
            let start = self.scroll_offset.min(total.saturating_sub(max_lines));
            let end = (start + max_lines).min(total);
            &self.output_lines[start..end]
        }
    }

    fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.user_scrolled = true;
    }

    fn scroll_down(&mut self, lines: usize, max_visible: usize) {
        let max_offset = self.output_lines.len().saturating_sub(max_visible);
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
        self.user_scrolled = true;
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self, max_visible: usize) {
        self.scroll_offset = self.output_lines.len().saturating_sub(max_visible);
    }

    pub fn is_running(&self) -> bool {
        self.status == ShellStatus::Running
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn status(&self) -> &ShellStatus {
        &self.status
    }

    pub fn output_lines(&self) -> &[String] {
        &self.output_lines
    }

    #[cfg(test)]
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Cancel a running shell command
    pub fn cancel(&mut self) {
        if !self.is_running() {
            return;
        }

        self.status = ShellStatus::Cancelled;
        self.duration = Some(self.start_time.elapsed());
        self.output_receiver = None;

        if let Some(pid) = self.child_pid {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .output();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output();
            }
        }
    }

    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn output_path(&self) -> Option<&PathBuf> {
        self.output_path.as_ref()
    }

    pub fn set_output_path(&mut self, path: PathBuf) {
        self.output_path = Some(path);
    }

    fn format_header(&self) -> String {
        let status_str = match &self.status {
            ShellStatus::Running => "● running".to_string(),
            ShellStatus::Completed { exit_code } if *exit_code == 0 => {
                format!("✓ exit 0 {:.1?}", self.duration.unwrap_or_default())
            }
            ShellStatus::Completed { exit_code } => {
                format!(
                    "✗ exit {} {:.1?}",
                    exit_code,
                    self.duration.unwrap_or_default()
                )
            }
            ShellStatus::Cancelled => "⏹ cancelled".to_string(),
        };
        format!("$ {}  {}", self.command, status_str)
    }

    fn format_footer_text(&self) -> String {
        let line_info = format!("({} lines)", self.output_lines.len());
        if self.is_running() {
            format!("Ctrl+C cancel  {}", line_info)
        } else {
            format!("i insert │ t truncated │ e edit │ q quit  {}", line_info)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_status_equality() {
        assert_eq!(ShellStatus::Running, ShellStatus::Running);
        assert_eq!(
            ShellStatus::Completed { exit_code: 0 },
            ShellStatus::Completed { exit_code: 0 }
        );
        assert_ne!(
            ShellStatus::Completed { exit_code: 0 },
            ShellStatus::Completed { exit_code: 1 }
        );
    }

    #[test]
    fn format_header_running() {
        let modal = create_test_modal(ShellStatus::Running);
        let header = modal.format_header();
        assert!(header.contains("$ echo test"));
        assert!(header.contains("● running"));
    }

    #[test]
    fn format_header_completed_success() {
        let modal = create_test_modal(ShellStatus::Completed { exit_code: 0 });
        let header = modal.format_header();
        assert!(header.contains("✓ exit 0"));
    }

    #[test]
    fn format_header_completed_failure() {
        let modal = create_test_modal(ShellStatus::Completed { exit_code: 1 });
        let header = modal.format_header();
        assert!(header.contains("✗ exit 1"));
    }

    #[test]
    fn format_header_cancelled() {
        let modal = create_test_modal(ShellStatus::Cancelled);
        let header = modal.format_header();
        assert!(header.contains("⏹ cancelled"));
    }

    #[test]
    fn visible_lines_within_limit() {
        let mut modal = create_test_modal(ShellStatus::Running);
        modal.output_lines = vec!["line1".into(), "line2".into(), "line3".into()];

        let visible = modal.visible_lines(10);
        assert_eq!(visible.len(), 3);
    }

    #[test]
    fn visible_lines_exceeds_limit() {
        let mut modal = create_test_modal(ShellStatus::Running);
        modal.output_lines = (0..20).map(|i| format!("line{}", i)).collect();

        let visible = modal.visible_lines(5);
        assert_eq!(visible.len(), 5);
    }

    #[test]
    fn scroll_operations() {
        let mut modal = create_test_modal(ShellStatus::Completed { exit_code: 0 });
        modal.output_lines = (0..50).map(|i| format!("line{}", i)).collect();

        modal.scroll_down(10, 20);
        assert_eq!(modal.scroll_offset, 10);

        modal.scroll_up(5);
        assert_eq!(modal.scroll_offset, 5);

        modal.scroll_to_top();
        assert_eq!(modal.scroll_offset, 0);

        modal.scroll_to_bottom(20);
        assert_eq!(modal.scroll_offset, 30);
    }

    #[test]
    fn close_produces_history_item() {
        let mut modal = create_test_modal(ShellStatus::Completed { exit_code: 42 });
        modal.output_lines = vec!["a".into(), "b".into(), "c".into()];

        match modal.close() {
            ShellModalOutput::Close(item) => {
                assert_eq!(item.command, "echo test");
                assert_eq!(item.exit_code, 42);
                assert_eq!(item.output_tail.len(), 3);
            }
            _ => panic!("Expected Close output"),
        }
    }

    fn create_test_modal(status: ShellStatus) -> ShellModal {
        ShellModal {
            command: "echo test".to_string(),
            output_lines: Vec::new(),
            status,
            scroll_offset: 0,
            user_scrolled: false,
            start_time: Instant::now(),
            duration: Some(Duration::from_millis(100)),
            output_path: None,
            working_dir: PathBuf::from("/tmp"),
            output_receiver: None,
            child_pid: None,
            pending_insert: None,
        }
    }
}
