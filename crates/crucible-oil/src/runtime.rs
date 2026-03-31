use crate::node::Node;
use crate::planning::Graduation;
use crate::terminal::Terminal;

/// Shared interface for rendering a frame. Implemented by Terminal (real I/O)
/// and TestRuntime (in-memory buffer for tests).
pub trait FrameRenderer {
    /// Render a Node tree to the viewport, writing any graduated content first.
    fn render_frame(&mut self, tree: &Node, graduation: Option<&Graduation>);

    /// Force a full redraw on the next render (clear all cached state).
    fn force_full_redraw(&mut self);

    /// Current terminal dimensions (width, height).
    fn size(&self) -> (u16, u16);
}

/// Test runtime that exercises the real Terminal write path.
///
/// Internally wraps `Terminal<Vec<u8>>` — the same escape sequence generation,
/// cursor math, and viewport diff logic as the real TUI. Output bytes can be
/// fed to a vt100 parser for screen-level assertions.
pub struct TestRuntime {
    terminal: Terminal<Vec<u8>>,
    /// Accumulated graduation content (rendered strings, not escape sequences).
    /// Used by `stdout_content()` for test assertions.
    stdout_buffer: String,
}

impl TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            terminal: Terminal::headless(width, height),
            stdout_buffer: String::new(),
        }
    }

    pub fn render(&mut self, tree: &Node) {
        self.render_with_stdout(tree, "");
    }

    pub fn render_with_stdout(&mut self, tree: &Node, stdout_delta: &str) {
        self.stdout_buffer.push_str(stdout_delta);
        let _ = self.terminal.render(tree, stdout_delta);
    }

    pub fn render_with_graduation(&mut self, tree: &Node, graduation: Option<&Graduation>) {
        if let Some(grad) = graduation {
            let rendered = grad.render();
            self.stdout_buffer.push_str(&rendered);
            self.stdout_buffer.push_str("\r\n");
            let _ = self.terminal.render(tree, &rendered);
        } else {
            let _ = self.terminal.render(tree, "");
        }
    }

    /// Accumulated graduation content as rendered strings.
    pub fn stdout_content(&self) -> &str {
        &self.stdout_buffer
    }

    /// Current viewport content from the last render.
    pub fn viewport_content(&self) -> &str {
        self.terminal
            .snapshot()
            .map(|s| s.viewport_content())
            .unwrap_or("")
    }

    pub fn trace(&self) -> Option<&crate::planning::FrameTrace> {
        self.terminal.snapshot().map(|s| s.trace())
    }

    pub fn last_snapshot(&self) -> Option<&crate::planning::FrameSnapshot> {
        self.terminal.snapshot()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.set_size(width, height);
    }

    pub fn width(&self) -> u16 {
        self.terminal.size().0
    }

    pub fn height(&self) -> u16 {
        self.terminal.size().1
    }

    /// Take the raw terminal output bytes (escape sequences + content).
    /// Useful for feeding to a vt100 parser.
    pub fn take_bytes(&mut self) -> Vec<u8> {
        self.terminal.take_bytes()
    }
}

impl FrameRenderer for TestRuntime {
    fn render_frame(&mut self, tree: &Node, graduation: Option<&Graduation>) {
        self.render_with_graduation(tree, graduation);
    }

    fn force_full_redraw(&mut self) {
        let _ = self.terminal.force_full_redraw();
    }

    fn size(&self) -> (u16, u16) {
        self.terminal.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_new() {
        let runtime = TestRuntime::new(80, 24);

        assert!(runtime.stdout_content().is_empty());
        assert!(runtime.viewport_content().is_empty());
    }

    #[test]
    fn test_runtime_renders_viewport() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Current content")]);
        runtime.render(&tree);

        assert!(runtime.viewport_content().contains("Current content"));
    }

    #[test]
    fn test_runtime_accumulates_stdout() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        runtime.render_with_stdout(&tree, "Graduated content\r\n");

        assert!(runtime.stdout_content().contains("Graduated content"));
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_renders_graduation_node() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        let grad = Graduation {
            node: col([text("Graduated via node")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad));

        assert!(runtime.stdout_content().contains("Graduated via node"));
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_graduation_none_no_stdout() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        runtime.render_with_graduation(&tree, None);

        assert!(runtime.stdout_content().is_empty());
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_graduation_accumulates_across_frames() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);

        // Frame 1: graduate content A
        let grad_a = Graduation {
            node: col([text("Content A")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad_a));

        // Frame 2: graduate content B
        let grad_b = Graduation {
            node: col([text("Content B")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad_b));

        let stdout = runtime.stdout_content();
        assert!(stdout.contains("Content A"));
        assert!(stdout.contains("Content B"));
    }

    #[test]
    fn test_frame_renderer_with_graduation() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Viewport")]);
        let grad = Graduation {
            node: col([text("Scrollback")]),
            width: 80,
        };

        // Use the FrameRenderer trait method
        FrameRenderer::render_frame(&mut runtime, &tree, Some(&grad));

        assert!(runtime.stdout_content().contains("Scrollback"));
        assert!(runtime.viewport_content().contains("Viewport"));
    }

    #[test]
    fn test_runtime_exercises_real_terminal_path() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Hello")]);
        runtime.render(&tree);

        // The headless terminal should have produced real escape sequences
        let bytes = runtime.take_bytes();
        assert!(!bytes.is_empty(), "Terminal should write escape sequences");
        let output = String::from_utf8_lossy(&bytes);
        assert!(
            output.contains("Hello"),
            "Terminal output should contain rendered text"
        );
        // Should contain ANSI escape sequences (hide cursor, etc.)
        assert!(
            output.contains("\x1b["),
            "Terminal output should contain escape sequences"
        );
    }
}
