//! Status bar widget
//!
//! Renders the status bar showing mode, streaming state, and hints.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Status bar configuration
pub struct StatusBar {
    pub mode_name: String,
    pub mode_color: Color,
    pub is_streaming: bool,
    pub hint_text: Option<String>,
}

impl StatusBar {
    /// Render the status bar
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mode_style = Style::default()
            .fg(self.mode_color)
            .add_modifier(Modifier::BOLD);

        let (icon, status) = if self.is_streaming {
            ("⟳", "Streaming...")
        } else {
            ("●", "Ready")
        };

        let mut spans = vec![
            Span::styled(format!("[{}]", self.mode_name), mode_style),
            Span::raw(" "),
            Span::styled(icon, Style::default().fg(self.mode_color)),
            Span::raw(" "),
            Span::raw(status),
        ];

        if let Some(ref hint) = self.hint_text {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(hint, Style::default().fg(Color::DarkGray)));
        }

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_status_bar_renders_mode_name() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "plan".to_string(),
                    mode_color: Color::Cyan,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        // Verify the buffer contains the mode name
        let buffer = terminal.backend().buffer();

        // Collect all text from the buffer
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        // Check that "plan" appears in the rendered output (wrapped in brackets)
        assert!(
            line.contains("[plan]"),
            "Status bar should render mode name '[plan]', got: {}",
            line.trim()
        );
    }

    #[test]
    fn test_status_bar_renders_streaming_state() {
        // Test "Streaming..." state
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: true,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        assert!(
            line.contains("Streaming..."),
            "Status bar should show 'Streaming...' when streaming, got: {}",
            line.trim()
        );
        assert!(
            line.contains("⟳"),
            "Status bar should show streaming icon '⟳', got: {}",
            line.trim()
        );

        // Test "Ready" state
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        assert!(
            line.contains("Ready"),
            "Status bar should show 'Ready' when not streaming, got: {}",
            line.trim()
        );
        assert!(
            line.contains("●"),
            "Status bar should show ready icon '●', got: {}",
            line.trim()
        );
    }

    #[test]
    fn test_status_bar_renders_hint() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "plan".to_string(),
                    mode_color: Color::Cyan,
                    is_streaming: false,
                    hint_text: Some("Press Ctrl+C to cancel".to_string()),
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        // Check for pipe separator and hint text
        assert!(
            line.contains(" | "),
            "Status bar should show separator ' | ' before hint, got: {}",
            line.trim()
        );
        assert!(
            line.contains("Press Ctrl+C to cancel"),
            "Status bar should display hint text, got: {}",
            line.trim()
        );
    }

    #[test]
    fn test_status_bar_no_hint() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        // Should not contain the separator when there's no hint
        assert!(
            !line.contains(" | "),
            "Status bar should not show separator when there's no hint, got: {}",
            line.trim()
        );
    }

    #[test]
    fn test_status_bar_different_modes() {
        // Test Plan mode (Cyan)
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "plan".to_string(),
                    mode_color: Color::Cyan,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(
            line.contains("[plan]"),
            "Plan mode should render '[plan]', got: {}",
            line.trim()
        );

        // Verify color is applied (mode name should have Cyan color)
        let mode_cell = buffer.get(1, 0); // Position of 'p' in '[plan]'
        assert_eq!(
            mode_cell.fg,
            Color::Cyan,
            "Plan mode should have Cyan foreground color"
        );

        // Test Act mode (Green)
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(
            line.contains("[act]"),
            "Act mode should render '[act]', got: {}",
            line.trim()
        );

        // Verify color is applied
        let mode_cell = buffer.get(1, 0); // Position of 'a' in '[act]'
        assert_eq!(
            mode_cell.fg,
            Color::Green,
            "Act mode should have Green foreground color"
        );

        // Test Auto mode (Yellow)
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "auto".to_string(),
                    mode_color: Color::Yellow,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(
            line.contains("[auto]"),
            "Auto mode should render '[auto]', got: {}",
            line.trim()
        );

        // Verify color is applied
        let mode_cell = buffer.get(1, 0); // Position of 'a' in '[auto]'
        assert_eq!(
            mode_cell.fg,
            Color::Yellow,
            "Auto mode should have Yellow foreground color"
        );
    }

    #[test]
    fn test_status_bar_full_composition() {
        // Test complete status bar with all elements
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "plan".to_string(),
                    mode_color: Color::Cyan,
                    is_streaming: true,
                    hint_text: Some("Use Tab to autocomplete".to_string()),
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();

        // Verify all components are present
        assert!(
            line.contains("[plan]"),
            "Should contain mode name, got: {}",
            line.trim()
        );
        assert!(
            line.contains("⟳"),
            "Should contain streaming icon, got: {}",
            line.trim()
        );
        assert!(
            line.contains("Streaming..."),
            "Should contain streaming status, got: {}",
            line.trim()
        );
        assert!(
            line.contains(" | "),
            "Should contain separator, got: {}",
            line.trim()
        );
        assert!(
            line.contains("Use Tab to autocomplete"),
            "Should contain hint text, got: {}",
            line.trim()
        );
    }

    #[test]
    fn test_status_bar_icon_changes_with_streaming() {
        // Verify icon changes from ● to ⟳
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: false,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let ready_line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(
            ready_line.contains("●") && !ready_line.contains("⟳"),
            "Ready state should show ●, not ⟳, got: {}",
            ready_line.trim()
        );

        // Now test streaming icon
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let status = StatusBar {
                    mode_name: "act".to_string(),
                    mode_color: Color::Green,
                    is_streaming: true,
                    hint_text: None,
                };
                status.render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let streaming_line: String = (0..80).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(
            streaming_line.contains("⟳") && !streaming_line.contains("●"),
            "Streaming state should show ⟳, not ●, got: {}",
            streaming_line.trim()
        );
    }
}
