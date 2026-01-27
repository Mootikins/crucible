use crate::tui::oil::component::Component;
use crate::tui::oil::node::{col, row, styled, text, Node};
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::ViewContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerKind {
    Messages,
    Tasks,
    Custom,
}

impl DrawerKind {
    pub fn name(&self) -> &'static str {
        match self {
            DrawerKind::Messages => "MESSAGES",
            DrawerKind::Tasks => "TASKS",
            DrawerKind::Custom => "CUSTOM",
        }
    }

    pub fn badge_bg(&self) -> Color {
        match self {
            DrawerKind::Messages => Color::Cyan,
            DrawerKind::Tasks => Color::Magenta,
            DrawerKind::Custom => colors::MODE_NORMAL,
        }
    }

    pub fn hint_fg(&self) -> Color {
        match self {
            DrawerKind::Messages => Color::Cyan,
            DrawerKind::Tasks => Color::Magenta,
            DrawerKind::Custom => colors::MODE_NORMAL,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Drawer {
    pub kind: DrawerKind,
    pub items: Vec<(String, String)>,
    pub max_items: usize,
    pub width: usize,
}

impl Drawer {
    pub fn new(kind: DrawerKind) -> Self {
        Self {
            kind,
            items: Vec::new(),
            max_items: 10,
            width: 80,
        }
    }

    pub fn items(mut self, items: Vec<(String, String)>) -> Self {
        self.items = items;
        self
    }

    pub fn max_items(mut self, n: usize) -> Self {
        self.max_items = n;
        self
    }

    pub fn width(mut self, w: usize) -> Self {
        self.width = w;
        self
    }

    fn render_border_top(&self) -> Node {
        let border: String = "▄".repeat(self.width);
        styled(border, Style::new().fg(colors::BORDER))
    }

    fn render_border_bottom(&self) -> Node {
        let border: String = "▀".repeat(self.width);
        styled(border, Style::new().fg(colors::BORDER))
    }

    fn render_content_row(&self, label: &str, content: &str) -> Node {
        let label_part = format!(" {}: ", label);
        let content_len = content.len();
        let used = label_part.len() + content_len;
        let padding = if self.width > used {
            " ".repeat(self.width - used)
        } else {
            String::new()
        };

        let style = Style::new().bg(colors::INPUT_BG).fg(colors::OVERLAY_TEXT);
        row([
            styled(label_part, style),
            styled(content.to_string(), style),
            styled(padding, Style::new().bg(colors::INPUT_BG)),
        ])
    }

    fn render_footer(&self) -> Node {
        row([
            styled(
                format!(" {} ", self.kind.name()),
                Style::new()
                    .bg(self.kind.badge_bg())
                    .fg(Color::Black)
                    .bold(),
            ),
            styled(" ".to_string(), Style::new()),
            styled("ESC/q".to_string(), Style::new().fg(self.kind.hint_fg())),
            styled(" ".to_string(), Style::new()),
            styled("close".to_string(), styles::overlay_hint()),
        ])
    }
}

impl Component for Drawer {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        let mut rows: Vec<Node> = Vec::new();

        rows.push(self.render_border_top());

        for (label, content) in self.items.iter().take(self.max_items) {
            rows.push(self.render_content_row(label, content));
        }

        rows.push(self.render_border_bottom());
        rows.push(self.render_footer());

        col(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn drawer_renders_items() {
        let drawer = Drawer::new(DrawerKind::Messages).width(60).items(vec![
            ("14:30:12".to_string(), "INFO Session saved".to_string()),
            ("14:31:00".to_string(), "WARN Low memory".to_string()),
        ]);
        let h = ComponentHarness::new(60, 10);
        let plain = render_to_plain_text(&drawer.view(&ViewContext::new(h.focus())), 60);
        assert!(plain.contains("14:30:12"));
        assert!(plain.contains("INFO Session saved"));
        assert!(plain.contains("14:31:00"));
        assert!(plain.contains("WARN Low memory"));
    }

    #[test]
    fn drawer_has_borders() {
        let drawer = Drawer::new(DrawerKind::Messages).width(40);
        let h = ComponentHarness::new(40, 10);
        let plain = render_to_plain_text(&drawer.view(&ViewContext::new(h.focus())), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
    }

    #[test]
    fn drawer_has_footer_badge() {
        let drawer = Drawer::new(DrawerKind::Messages).width(60);
        let h = ComponentHarness::new(60, 10);
        let plain = render_to_plain_text(&drawer.view(&ViewContext::new(h.focus())), 60);
        assert!(plain.contains("MESSAGES"));
        assert!(plain.contains("ESC/q"));
        assert!(plain.contains("close"));
    }

    #[test]
    fn drawer_limits_items() {
        let items: Vec<(String, String)> = (0..20)
            .map(|i| (format!("label{}", i), format!("content{}", i)))
            .collect();
        let drawer = Drawer::new(DrawerKind::Tasks)
            .width(60)
            .max_items(3)
            .items(items);
        let h = ComponentHarness::new(60, 10);
        let plain = render_to_plain_text(&drawer.view(&ViewContext::new(h.focus())), 60);
        assert!(plain.contains("label0"));
        assert!(plain.contains("label2"));
        assert!(!plain.contains("label3"));
    }

    #[test]
    fn drawer_empty_items() {
        let drawer = Drawer::new(DrawerKind::Custom).width(40);
        let h = ComponentHarness::new(40, 10);
        let plain = render_to_plain_text(&drawer.view(&ViewContext::new(h.focus())), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
        assert!(plain.contains("CUSTOM"));
        assert!(!plain.contains(':'));
    }
}
