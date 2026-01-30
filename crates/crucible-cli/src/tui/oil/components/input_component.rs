use crate::tui::oil::component::Component;
use crate::tui::oil::components::InputMode;
use crate::tui::oil::node::*;
use crate::tui::oil::style::Style;
use crate::tui::oil::ViewContext;

use super::INPUT_MAX_CONTENT_LINES;

pub struct InputComponent<'a> {
    pub content: &'a str,
    pub cursor: usize,
    pub mode: InputMode,
    pub focused: bool,
    pub width: usize,
    pub show_popup: bool,
}

impl<'a> InputComponent<'a> {
    pub fn new(content: &'a str, cursor: usize, width: usize) -> Self {
        Self {
            content,
            cursor,
            mode: InputMode::Normal,
            focused: true,
            width,
            show_popup: false,
        }
    }

    #[must_use]
    pub fn mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub fn show_popup(mut self, show_popup: bool) -> Self {
        self.show_popup = show_popup;
        self
    }

    fn clamp_input_lines(
        lines: &[String],
        cursor_line: usize,
        max_lines: usize,
    ) -> (Vec<String>, usize) {
        if lines.len() <= max_lines {
            return (lines.to_vec(), cursor_line);
        }

        let half = max_lines / 2;
        let start = if cursor_line <= half {
            0
        } else if cursor_line >= lines.len() - half {
            lines.len() - max_lines
        } else {
            cursor_line - half
        };

        let end = (start + max_lines).min(lines.len());
        let visible = lines[start..end].to_vec();
        let adjusted_cursor = cursor_line - start;

        (visible, adjusted_cursor)
    }
}

fn wrap_content(content: &str, max_width: usize) -> Vec<String> {
    if content.is_empty() || max_width == 0 {
        return vec![String::new()];
    }

    let chars: Vec<char> = content.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + max_width).min(chars.len());
        lines.push(chars[start..end].iter().collect());
        start = end;
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

impl Component for InputComponent<'_> {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        let is_focused = self.focused;

        let prompt = self.mode.prompt();
        let bg = self.mode.bg_color();

        let top_edge = styled("▄".repeat(self.width), Style::new().fg(bg));
        let bottom_edge = styled("▀".repeat(self.width), Style::new().fg(bg));

        let display_content = match self.mode {
            InputMode::Command => self.content.strip_prefix(':').unwrap_or(self.content),
            InputMode::Shell => self.content.strip_prefix('!').unwrap_or(self.content),
            InputMode::Normal => self.content,
        };

        let cursor_offset = if matches!(self.mode, InputMode::Command | InputMode::Shell) {
            1
        } else {
            0
        };
        let display_cursor = self.cursor.saturating_sub(cursor_offset);

        let content_width = self.width.saturating_sub(prompt.len() + 1);
        let all_lines = wrap_content(display_content, content_width);

        let (cursor_line, cursor_col) = if content_width > 0 && !all_lines.is_empty() {
            let line_idx = display_cursor / content_width;
            let col_in_line = display_cursor % content_width;
            (line_idx.min(all_lines.len() - 1), col_in_line)
        } else {
            (0, display_cursor)
        };

        let (visible_lines, visible_cursor_line) =
            Self::clamp_input_lines(&all_lines, cursor_line, INPUT_MAX_CONTENT_LINES);

        let mut rows: Vec<Node> = Vec::with_capacity(INPUT_MAX_CONTENT_LINES + 2);
        rows.push(top_edge);

        for (i, line) in visible_lines.iter().enumerate() {
            let line_len = line.chars().count();
            let line_padding = " ".repeat(content_width.saturating_sub(line_len) + 1);
            let is_first_visible = i == 0 && visible_lines.len() == all_lines.len();
            let line_prefix = if is_first_visible { prompt } else { "   " };

            if i == visible_cursor_line && is_focused {
                rows.push(row([
                    styled(line_prefix, Style::new().bg(bg)),
                    Node::Input(InputNode {
                        value: line.to_string(),
                        cursor: cursor_col.min(line_len),
                        placeholder: None,
                        style: Style::new().bg(bg),
                        focused: true,
                    }),
                    styled(line_padding, Style::new().bg(bg)),
                ]));
            } else {
                rows.push(styled(
                    format!("{}{}{}", line_prefix, line, line_padding),
                    Style::new().bg(bg),
                ));
            }
        }

        rows.push(bottom_edge);

        let input_node = col(rows);

        focusable_auto("input", input_node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn empty_input_normal_mode() {
        let input = InputComponent::new("", 0, 80).mode(InputMode::Normal);
        let h = ComponentHarness::new(80, 10);
        let plain = render_to_plain_text(&input.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains(">"), "should show normal prompt");
    }

    #[test]
    fn input_with_text_normal_mode() {
        let input = InputComponent::new("hello world", 11, 80).mode(InputMode::Normal);
        let h = ComponentHarness::new(80, 10);
        let plain = render_to_plain_text(&input.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains(">"), "should show normal prompt");
        assert!(plain.contains("hello world"), "should show content");
    }

    #[test]
    fn command_mode_strips_colon() {
        let input = InputComponent::new(":set model gpt-4", 16, 80).mode(InputMode::Command);
        let h = ComponentHarness::new(80, 10);
        let plain = render_to_plain_text(&input.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains(":"), "should show command prompt");
        assert!(
            plain.contains("set model gpt-4"),
            "should show content without colon prefix"
        );
    }

    #[test]
    fn shell_mode_strips_bang() {
        let input = InputComponent::new("!ls -la", 7, 80).mode(InputMode::Shell);
        let h = ComponentHarness::new(80, 10);
        let plain = render_to_plain_text(&input.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("!"), "should show shell prompt");
        assert!(
            plain.contains("ls -la"),
            "should show content without bang prefix"
        );
    }

    #[test]
    fn multiline_wrapping() {
        let long_text = "abcdefghijklmnopqrstuvwxyz";
        let input = InputComponent::new(long_text, 0, 20).mode(InputMode::Normal);
        let h = ComponentHarness::new(20, 10);
        let plain = render_to_plain_text(&input.view(&ViewContext::new(h.focus())), 20);
        assert!(plain.contains("a"), "should contain start of content");
        assert!(plain.contains("z"), "should contain end of content");
    }

    #[test]
    fn unfocused_hides_cursor() {
        let input = InputComponent::new("test", 4, 80)
            .mode(InputMode::Normal)
            .focused(false);
        let h = ComponentHarness::new(80, 10);
        let node = input.view(&ViewContext::new(h.focus()));
        fn has_input_node(node: &Node) -> bool {
            match node {
                Node::Input(_) => true,
                Node::Box(b) => b.children.iter().any(has_input_node),
                Node::Fragment(children) => children.iter().any(has_input_node),
                Node::Focusable(f) => has_input_node(&f.child),
                _ => false,
            }
        }
        assert!(
            !has_input_node(&node),
            "unfocused input should not contain InputNode"
        );
    }

    #[test]
    fn clamp_input_lines_within_max() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let (visible, cursor) = InputComponent::clamp_input_lines(&lines, 1, 3);
        assert_eq!(visible.len(), 2);
        assert_eq!(cursor, 1);
    }

    #[test]
    fn clamp_input_lines_exceeds_max() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let (visible, cursor) = InputComponent::clamp_input_lines(&lines, 5, 3);
        assert_eq!(visible.len(), 3);
        assert!(cursor < 3, "cursor should be within visible range");
    }
}
