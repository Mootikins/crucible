//! Splash screen widget for TUI
//!
//! Shows a welcome screen with agent selection when conversation is empty.

use crucible_acp::KnownAgent;
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
    /// Whether the agent is available (checked async)
    pub available: Option<bool>,
}

/// Splash screen state
#[derive(Debug, Default)]
pub struct SplashState {
    pub agents: Vec<AgentOption>,
    pub selected_index: usize,
    pub cwd: String,
    /// True when availability has been probed
    pub probed: bool,
}

impl SplashState {
    /// Create splash with known ACP agents (availability unknown until probed)
    pub fn new(cwd: String) -> Self {
        let known = crucible_acp::get_known_agents();
        let mut agents: Vec<AgentOption> = known
            .into_iter()
            .map(|ka| AgentOption {
                name: ka.name,
                description: ka.description,
                is_default: false,
                available: None, // Unknown until probed
            })
            .collect();

        // Add "internal" option for direct LLM (always available)
        agents.push(AgentOption {
            name: "internal".to_string(),
            description: "Direct LLM (Ollama/OpenAI)".to_string(),
            is_default: false,
            available: Some(true),
        });

        // Find first available or default to first agent
        let default_idx = 0;
        if let Some(agent) = agents.get_mut(default_idx) {
            agent.is_default = true;
        }

        Self {
            agents,
            selected_index: default_idx,
            cwd,
            probed: false,
        }
    }

    /// Update agent availability from probed results
    pub fn update_availability(&mut self, probed: Vec<KnownAgent>) {
        for ka in probed {
            if let Some(agent) = self.agents.iter_mut().find(|a| a.name == ka.name) {
                agent.available = Some(ka.available);
            }
        }
        self.probed = true;

        // Update default to first available agent
        if let Some(idx) = self.agents.iter().position(|a| a.available == Some(true)) {
            // Clear old default
            for agent in &mut self.agents {
                agent.is_default = false;
            }
            self.agents[idx].is_default = true;
            self.selected_index = idx;
        }
    }

    /// Check if an agent at index is available (or availability unknown)
    fn is_selectable(&self, index: usize) -> bool {
        self.agents
            .get(index)
            .map(|a| a.available != Some(false))
            .unwrap_or(false)
    }

    /// Find next selectable agent index (skips unavailable)
    fn next_selectable(&self, from: usize) -> usize {
        let len = self.agents.len();
        for i in 1..=len {
            let idx = (from + i) % len;
            if self.is_selectable(idx) {
                return idx;
            }
        }
        from // fallback to current if none available
    }

    /// Find previous selectable agent index (skips unavailable)
    fn prev_selectable(&self, from: usize) -> usize {
        let len = self.agents.len();
        for i in 1..=len {
            let idx = (from + len - i) % len;
            if self.is_selectable(idx) {
                return idx;
            }
        }
        from // fallback to current if none available
    }

    pub fn select_next(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.next_selectable(self.selected_index);
        }
    }

    pub fn select_prev(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.prev_selectable(self.selected_index);
        }
    }

    /// Select agent by index (0-indexed), only if available
    pub fn select_index(&mut self, index: usize) {
        if index < self.agents.len() && self.is_selectable(index) {
            self.selected_index = index;
        }
    }

    pub fn selected_agent(&self) -> Option<&AgentOption> {
        self.agents.get(self.selected_index)
    }

    /// Check if current selection can be confirmed (must be confirmed available)
    pub fn can_confirm(&self) -> bool {
        self.selected_agent()
            .map(|a| a.available == Some(true))
            .unwrap_or(false)
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
        // 4 header lines + agents + 4 footer lines
        let content_height = 4 + self.state.agents.len() as u16 + 4;
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
            Line::from(vec![Span::styled(
                "CRUCIBLE CHAT",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("═══════════════════"),
            Line::from(""),
            // Agent selection
            Line::from(Span::styled(
                "Select Agent:",
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ];

        for (i, agent) in self.state.agents.iter().enumerate() {
            let is_selected = i == self.state.selected_index;
            let selector = if is_selected { "▸" } else { " " };
            let number = i + 1; // 1-indexed for display

            // Build suffix with availability status
            let status = match agent.available {
                Some(true) => " ✓",
                Some(false) => " ✗",
                None => " …", // Still checking
            };
            let default_marker = if agent.is_default { " (default)" } else { "" };

            // Style based on selection and availability
            let (name_style, status_style, num_style) = if is_selected {
                (
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    match agent.available {
                        Some(true) => Style::default().fg(Color::Green),
                        Some(false) => Style::default().fg(Color::Red),
                        None => Style::default().fg(Color::DarkGray),
                    },
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let base = match agent.available {
                    Some(true) => Style::default().fg(Color::Gray),
                    Some(false) => Style::default().fg(Color::DarkGray),
                    None => Style::default().fg(Color::Gray),
                };
                (
                    base,
                    match agent.available {
                        Some(true) => Style::default().fg(Color::Green),
                        Some(false) => Style::default().fg(Color::DarkGray),
                        None => Style::default().fg(Color::DarkGray),
                    },
                    Style::default().fg(Color::DarkGray),
                )
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", selector), name_style),
                Span::styled(format!("{}.", number), num_style),
                Span::styled(format!(" {}", agent.name), name_style),
                Span::styled(status, status_style),
                Span::styled(default_marker, name_style),
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
            Span::styled("[j/k]", Style::default().fg(Color::DarkGray)),
            Span::raw(" navigate  "),
            Span::styled("[1-9]", Style::default().fg(Color::DarkGray)),
            Span::raw(" quick select  "),
            Span::styled("[l/Enter]", Style::default().fg(Color::DarkGray)),
            Span::raw(" confirm  "),
            Span::styled("[q]", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ]));

        let paragraph = Paragraph::new(lines).alignment(Alignment::Center);

        paragraph.render(content_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Get a platform-agnostic temp dir path for tests
    fn test_cwd() -> String {
        std::env::temp_dir().display().to_string()
    }

    #[test]
    fn test_splash_state_new() {
        let cwd = test_cwd();
        let state = SplashState::new(cwd.clone());
        assert!(!state.agents.is_empty());
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.cwd, cwd);
    }

    #[test]
    fn test_splash_navigation() {
        let mut state = SplashState::new(test_cwd());
        let num_agents = state.agents.len();
        assert!(num_agents >= 2, "Should have at least 2 agents");
        assert_eq!(state.selected_index, 0);

        // All agents start with unknown availability (except internal=true)
        // so navigation should work normally
        state.select_next();
        assert_eq!(state.selected_index, 1);

        // Navigate to end (all unknown = selectable)
        for _ in 0..num_agents - 1 {
            state.select_next();
        }
        assert_eq!(state.selected_index, 0); // Wraps around

        state.select_prev();
        assert_eq!(state.selected_index, num_agents - 1); // Wraps backward
    }

    #[test]
    fn test_navigation_skips_unavailable() {
        let mut state = SplashState::new(test_cwd());

        // Mark agents 1 and 2 as unavailable
        state.agents[1].available = Some(false);
        state.agents[2].available = Some(false);

        // Start at 0, next should skip 1 and 2, land on 3
        state.selected_index = 0;
        state.select_next();
        assert_eq!(state.selected_index, 3);

        // Going back should skip 2 and 1, land on 0
        state.select_prev();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_select_index_skips_unavailable() {
        let mut state = SplashState::new(test_cwd());

        // Mark agent 2 as unavailable
        state.agents[2].available = Some(false);

        // Trying to select index 2 should be ignored
        state.selected_index = 0;
        state.select_index(2);
        assert_eq!(state.selected_index, 0); // Unchanged

        // Selecting available index should work
        state.select_index(3);
        assert_eq!(state.selected_index, 3);
    }

    #[test]
    fn test_can_confirm_requires_known_availability() {
        let mut state = SplashState::new(test_cwd());

        // Unknown availability = cannot confirm (must wait for probe)
        assert!(!state.can_confirm());

        // Mark current selection as unavailable
        state.agents[0].available = Some(false);
        assert!(!state.can_confirm());

        // Mark as available
        state.agents[0].available = Some(true);
        assert!(state.can_confirm());
    }

    #[test]
    fn test_selected_agent() {
        let state = SplashState::new(test_cwd());
        let agent = state.selected_agent().unwrap();
        // First known agent should be opencode
        assert_eq!(agent.name, "opencode");
    }

    #[test]
    fn test_update_availability() {
        let mut state = SplashState::new(test_cwd());

        // Initially all agents have unknown availability (except internal)
        let acp_agents: Vec<_> = state
            .agents
            .iter()
            .filter(|a| a.name != "internal")
            .collect();
        assert!(
            acp_agents.iter().all(|a| a.available.is_none()),
            "ACP agents should start with unknown availability"
        );

        // Simulate probing results - only opencode available
        let probed = vec![
            KnownAgent {
                name: "opencode".to_string(),
                description: "".to_string(),
                available: true,
            },
            KnownAgent {
                name: "claude".to_string(),
                description: "".to_string(),
                available: false,
            },
        ];
        state.update_availability(probed);

        // Check availability was updated
        let opencode = state.agents.iter().find(|a| a.name == "opencode").unwrap();
        assert_eq!(opencode.available, Some(true));
        assert!(opencode.is_default, "First available should be default");

        let claude = state.agents.iter().find(|a| a.name == "claude").unwrap();
        assert_eq!(claude.available, Some(false));
    }

    #[test]
    fn test_internal_always_available() {
        let state = SplashState::new(test_cwd());
        let internal = state.agents.iter().find(|a| a.name == "internal").unwrap();
        assert_eq!(internal.available, Some(true));
    }

    #[test]
    fn test_select_index() {
        let mut state = SplashState::new(test_cwd());
        assert_eq!(state.selected_index, 0);

        // Select by valid index
        state.select_index(2);
        assert_eq!(state.selected_index, 2);

        // Out of bounds index is ignored
        state.select_index(100);
        assert_eq!(state.selected_index, 2); // Unchanged

        // Select first
        state.select_index(0);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_splash_widget_renders() {
        let state = SplashState::new(test_cwd());
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
