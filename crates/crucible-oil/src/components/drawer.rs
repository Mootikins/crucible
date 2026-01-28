use crate::node::{col, row, styled, Node};
use crate::style::{Color, Style};

/// Trait for drawer kind customization
///
/// Implement this trait to define drawer-specific styling and naming.
pub trait DrawerKind {
    /// Display name for the drawer (e.g., "MESSAGES")
    fn name(&self) -> &'static str;

    /// Background color for the badge
    fn badge_bg(&self) -> Color;

    /// Foreground color for hints
    fn hint_fg(&self) -> Color;
}

/// A drawer component for displaying items with borders and footer
///
/// The drawer renders a list of items with top/bottom borders and a footer badge.
/// It supports both pre-rendered content rows and simple label/content pairs.
#[derive(Debug, Clone)]
pub struct Drawer<K: DrawerKind> {
    pub kind: K,
    pub items: Vec<(String, String)>,
    pub content_rows: Vec<Node>,
    pub max_items: usize,
    pub width: usize,
}

impl<K: DrawerKind> Drawer<K> {
    /// Create a new drawer with the given kind
    pub fn new(kind: K) -> Self {
        Self {
            kind,
            items: Vec::new(),
            content_rows: Vec::new(),
            max_items: 10,
            width: 80,
        }
    }

    /// Set items (label, content) pairs
    #[must_use]
    pub fn items(mut self, items: Vec<(String, String)>) -> Self {
        self.items = items;
        self
    }

    /// Set pre-rendered content rows
    #[must_use]
    pub fn content_rows(mut self, rows: Vec<Node>) -> Self {
        self.content_rows = rows;
        self
    }

    /// Set maximum number of items to display
    #[must_use]
    pub fn max_items(mut self, n: usize) -> Self {
        self.max_items = n;
        self
    }

    /// Set drawer width
    #[must_use]
    pub fn width(mut self, w: usize) -> Self {
        self.width = w;
        self
    }

    fn render_border_top(&self) -> Node {
        let border: String = "▄".repeat(self.width);
        styled(border, Style::new().fg(Color::Rgb(40, 44, 52)))
    }

    fn render_border_bottom(&self) -> Node {
        let border: String = "▀".repeat(self.width);
        styled(border, Style::new().fg(Color::Rgb(40, 44, 52)))
    }

    fn render_content_row(&self, label: &str, content: &str) -> Node {
        let label_part = format!(" {}: ", label);
        let content_len = content.chars().count();
        let used = label_part.chars().count() + content_len;
        let padding = if self.width > used {
            " ".repeat(self.width - used)
        } else {
            String::new()
        };

        let style = Style::new().bg(Color::Rgb(40, 44, 52)).fg(Color::White);
        row([
            styled(label_part, style),
            styled(content.to_string(), style),
            styled(padding, Style::new().bg(Color::Rgb(40, 44, 52))),
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
            styled(
                "close".to_string(),
                Style::new().fg(Color::Rgb(100, 110, 130)),
            ),
        ])
    }

    /// Render the drawer as a Node
    pub fn view(&self) -> Node {
        let mut rows: Vec<Node> = Vec::new();

        rows.push(self.render_border_top());

        if !self.content_rows.is_empty() {
            for row_node in self.content_rows.iter().take(self.max_items) {
                rows.push(row_node.clone());
            }
        } else {
            for (label, content) in self.items.iter().take(self.max_items) {
                rows.push(self.render_content_row(label, content));
            }
        }

        rows.push(self.render_border_bottom());
        rows.push(self.render_footer());

        col(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_to_plain_text;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestDrawerKind;

    impl DrawerKind for TestDrawerKind {
        fn name(&self) -> &'static str {
            "TEST"
        }

        fn badge_bg(&self) -> Color {
            Color::Cyan
        }

        fn hint_fg(&self) -> Color {
            Color::Cyan
        }
    }

    #[test]
    fn drawer_renders_items() {
        let drawer = Drawer::new(TestDrawerKind).width(60).items(vec![
            ("14:30:12".to_string(), "INFO Session saved".to_string()),
            ("14:31:00".to_string(), "WARN Low memory".to_string()),
        ]);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("14:30:12"));
        assert!(plain.contains("INFO Session saved"));
        assert!(plain.contains("14:31:00"));
        assert!(plain.contains("WARN Low memory"));
    }

    #[test]
    fn drawer_has_borders() {
        let drawer = Drawer::new(TestDrawerKind).width(40);
        let plain = render_to_plain_text(&drawer.view(), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
    }

    #[test]
    fn drawer_has_footer_badge() {
        let drawer = Drawer::new(TestDrawerKind).width(60);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("TEST"));
        assert!(plain.contains("ESC/q"));
        assert!(plain.contains("close"));
    }

    #[test]
    fn drawer_limits_items() {
        let items: Vec<(String, String)> = (0..20)
            .map(|i| (format!("label{}", i), format!("content{}", i)))
            .collect();
        let drawer = Drawer::new(TestDrawerKind)
            .width(60)
            .max_items(3)
            .items(items);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("label0"));
        assert!(plain.contains("label2"));
        assert!(!plain.contains("label3"));
    }

    #[test]
    fn drawer_empty_items() {
        let drawer = Drawer::new(TestDrawerKind).width(40);
        let plain = render_to_plain_text(&drawer.view(), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
        assert!(plain.contains("TEST"));
    }
}
