//! Splash screen widget for TUI
//!
//! Shows a welcome screen with agent selection when conversation is empty.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Agent option for display
#[derive(Debug, Clone)]
pub struct AgentOption {
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

/// Splash screen state
#[derive(Debug, Default)]
pub struct SplashState {
    pub agents: Vec<AgentOption>,
    pub selected_index: usize,
    pub cwd: String,
}

impl SplashState {
    pub fn new(cwd: String) -> Self {
        Self {
            agents: vec![
                AgentOption {
                    name: "claude-code".to_string(),
                    description: "Claude Code via ACP".to_string(),
                    is_default: true,
                },
                AgentOption {
                    name: "internal".to_string(),
                    description: "Direct LLM (Ollama/OpenAI)".to_string(),
                    is_default: false,
                },
            ],
            selected_index: 0,
            cwd,
        }
    }

    pub fn select_next(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.agents.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.selected_index.checked_sub(1)
                .unwrap_or(self.agents.len() - 1);
        }
    }

    pub fn selected_agent(&self) -> Option<&AgentOption> {
        self.agents.get(self.selected_index)
    }
}

/// Widget that renders the splash screen
pub struct SplashWidget<'a> {
    state: &'a SplashState,
}

impl<'a> SplashWidget<'a> {
    pub fn new(state: &'a SplashState) -> Self {
        Self { state }
    }
}

impl Widget for SplashWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center content vertically
        let content_height = 12; // Approximate
        let vertical_padding = area.height.saturating_sub(content_height) / 2;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(vertical_padding),
                Constraint::Length(content_height),
                Constraint::Min(0),
            ])
            .split(area);

        let content_area = chunks[1];

        // Build content lines
        let mut lines = vec![
            // Title
            Line::from(vec![
                Span::styled("CRUCIBLE CHAT", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            ]),
            Line::from("═══════════════════"),
            Line::from(""),
            // Agent selection
            Line::from(Span::styled("Select Agent:", Style::default().add_modifier(Modifier::BOLD))),
        ];

        for (i, agent) in self.state.agents.iter().enumerate() {
            let is_selected = i == self.state.selected_index;
            let prefix = if is_selected { "▸ " } else { "  " };
            let suffix = if agent.is_default { " (default)" } else { "" };

            let style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{}{}{}", prefix, agent.name, suffix), style),
            ]));
        }

        lines.push(Line::from(""));

        // Current directory
        lines.push(Line::from(vec![
            Span::styled("cwd: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&self.state.cwd, Style::default().fg(Color::Blue)),
        ]));

        lines.push(Line::from(""));

        // Help hints
        lines.push(Line::from(vec![
            Span::styled("[↑↓]", Style::default().fg(Color::DarkGray)),
            Span::raw(" navigate  "),
            Span::styled("[Enter]", Style::default().fg(Color::DarkGray)),
            Span::raw(" select  "),
            Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ]));

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Center);

        paragraph.render(content_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_splash_state_new() {
        let state = SplashState::new("/home/test".to_string());
        assert!(!state.agents.is_empty());
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.cwd, "/home/test");
    }

    #[test]
    fn test_splash_navigation() {
        let mut state = SplashState::new("/tmp".to_string());
        assert_eq!(state.selected_index, 0);

        state.select_next();
        assert_eq!(state.selected_index, 1);

        state.select_next();
        assert_eq!(state.selected_index, 0); // Wraps around

        state.select_prev();
        assert_eq!(state.selected_index, 1); // Wraps backward
    }

    #[test]
    fn test_selected_agent() {
        let state = SplashState::new("/tmp".to_string());
        let agent = state.selected_agent().unwrap();
        assert_eq!(agent.name, "claude-code");
    }

    #[test]
    fn test_splash_widget_renders() {
        let state = SplashState::new("/home/user".to_string());
        let widget = SplashWidget::new(&state);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Check that title appears somewhere in buffer
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }

        assert!(content.contains("CRUCIBLE"));
    }
}
