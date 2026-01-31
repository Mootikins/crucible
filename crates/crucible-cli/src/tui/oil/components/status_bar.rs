use crate::tui::oil::chat_app::ChatMode;
use crate::tui::oil::node::{row, spacer, styled, Node};
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::utils::truncate_to_chars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationToastKind {
    Info,
    Warning,
    Error,
}

impl NotificationToastKind {
    pub fn color(&self) -> Color {
        let theme = ThemeTokens::default_ref();
        match self {
            NotificationToastKind::Info => theme.info,
            NotificationToastKind::Warning => theme.warning,
            NotificationToastKind::Error => theme.error,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NotificationToastKind::Info => "INFO",
            NotificationToastKind::Warning => "WARN",
            NotificationToastKind::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    pub mode: ChatMode,
    pub model: String,
    pub context_used: usize,
    pub context_total: usize,
    pub status: String,
    pub notification_toast: Option<(String, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn context(mut self, used: usize, total: usize) -> Self {
        self.context_used = used;
        self.context_total = total;
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn toast(mut self, text: impl Into<String>, kind: NotificationToastKind) -> Self {
        self.notification_toast = Some((text.into(), kind));
        self
    }

    pub fn counts(mut self, counts: Vec<(NotificationToastKind, usize)>) -> Self {
        self.notification_counts = counts;
        self
    }

    fn mode_style(&self) -> Style {
        let theme = ThemeTokens::default_ref();
        match self.mode {
            ChatMode::Normal => theme.mode_normal_style(),
            ChatMode::Plan => theme.mode_plan_style(),
            ChatMode::Auto => theme.mode_auto_style(),
        }
    }

    fn mode_label(&self) -> &'static str {
        match self.mode {
            ChatMode::Normal => " NORMAL ",
            ChatMode::Plan => " PLAN ",
            ChatMode::Auto => " AUTO ",
        }
    }

    fn context_display(&self) -> String {
        if self.context_total > 0 {
            let percent =
                (self.context_used as f64 / self.context_total as f64 * 100.0).round() as usize;
            format!("{}% ctx", percent)
        } else if self.context_used > 0 {
            format!("{}k tok", self.context_used / 1000)
        } else {
            String::new()
        }
    }

    fn model_display(&self) -> String {
        if self.model.is_empty() {
            "...".to_string()
        } else {
            truncate_to_chars(&self.model, 20, true).into_owned()
        }
    }
}

impl StatusBar {
    pub fn view_from_config(&self, config: &crucible_lua::statusline::StatuslineConfig) -> Node {
        use crate::tui::oil::lua_bridge::{render_component_node, StatusBarData};

        let theme = ThemeTokens::default_ref();
        let data = StatusBarData {
            mode: self.mode,
            model: self.model.clone(),
            context_used: self.context_used,
            context_total: self.context_total,
            status: self.status.clone(),
            notification_toast: self.notification_toast.clone(),
            notification_counts: self.notification_counts.clone(),
        };

        let sep = config.separator.as_deref().unwrap_or(" ");

        let mut items: Vec<Node> = Vec::new();

        let render_section = |components: &[crucible_lua::statusline::StatuslineComponent],
                              data: &StatusBarData,
                              sep: &str|
         -> Vec<Node> {
            let mut nodes = Vec::new();
            for (i, component) in components.iter().enumerate() {
                if i > 0 && !sep.is_empty() {
                    nodes.push(styled(sep.to_string(), theme.muted()));
                }
                nodes.push(render_component_node(component, data));
            }
            nodes
        };

        items.extend(render_section(&config.left, &data, sep));

        if !config.center.is_empty() {
            items.push(spacer());
            items.extend(render_section(&config.center, &data, sep));
        }

        if !config.right.is_empty() {
            items.push(spacer());
            items.extend(render_section(&config.right, &data, sep));
        }

        row(items)
    }
}

impl StatusBar {
    pub fn emergency_view(&self) -> Node {
        let theme = ThemeTokens::default_ref();
        row(vec![
            styled(self.mode_label().to_string(), self.mode_style()),
            styled(" ".to_string(), theme.muted()),
            styled(self.model_display(), theme.model_name_style()),
            spacer(),
            styled(self.context_display(), theme.muted()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn emergency_view_shows_mode() {
        let bar = StatusBar::new().mode(ChatMode::Normal);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("NORMAL"));
    }

    #[test]
    fn emergency_view_shows_model_name() {
        let bar = StatusBar::new().model("gpt-4o-mini");
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("gpt-4o-mini"));
    }

    #[test]
    fn emergency_view_truncates_long_model() {
        let bar = StatusBar::new().model("very-long-model-name-that-exceeds-twenty-characters");
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("…"));
        assert!(!plain.contains("twenty-characters"));
    }

    #[test]
    fn emergency_view_shows_context_percentage() {
        let bar = StatusBar::new().context(4000, 8000);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("50% ctx"));
    }

    #[test]
    fn emergency_view_shows_token_count_without_total() {
        let bar = StatusBar::new().context(5000, 0);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("5k tok"));
    }

    #[test]
    fn status_bar_modes_have_different_colors() {
        let normal = StatusBar::new().mode(ChatMode::Normal);
        let plan = StatusBar::new().mode(ChatMode::Plan);
        let auto = StatusBar::new().mode(ChatMode::Auto);

        assert_ne!(normal.mode_style().bg, plan.mode_style().bg);
        assert_ne!(plan.mode_style().bg, auto.mode_style().bg);
    }

    mod config_driven {
        use super::*;
        use crucible_lua::statusline::{
            ModeStyleSpec, StatuslineComponent, StatuslineConfig, StyleSpec,
        };

        fn make_config(
            left: Vec<StatuslineComponent>,
            center: Vec<StatuslineComponent>,
            right: Vec<StatuslineComponent>,
        ) -> StatuslineConfig {
            StatuslineConfig {
                left,
                center,
                right,
                separator: None,
            }
        }

        fn mode_component() -> StatuslineComponent {
            StatuslineComponent::Mode {
                normal: ModeStyleSpec::default(),
                plan: ModeStyleSpec::default(),
                auto: ModeStyleSpec::default(),
            }
        }

        #[test]
        fn config_with_left_mode_center_model_right_context() {
            let config = make_config(
                vec![mode_component()],
                vec![StatuslineComponent::Model {
                    max_length: None,
                    fallback: None,
                    style: StyleSpec::default(),
                }],
                vec![StatuslineComponent::Context {
                    format: None,
                    style: StyleSpec::default(),
                }],
            );
            let bar = StatusBar::new()
                .mode(ChatMode::Normal)
                .model("gpt-4o")
                .context(4000, 8000);
            let node = bar.view_from_config(&config);
            let plain = render_to_plain_text(&node, 120);
            assert!(
                plain.contains("NORMAL"),
                "Should contain mode label: {}",
                plain
            );
            assert!(plain.contains("gpt-4o"), "Should contain model: {}", plain);
            assert!(
                plain.contains("50% ctx"),
                "Should contain context: {}",
                plain
            );
        }

        #[test]
        fn config_with_text() {
            let config = make_config(
                vec![StatuslineComponent::Text {
                    content: "hello".to_string(),
                    style: StyleSpec::default(),
                }],
                vec![],
                vec![],
            );
            let bar = StatusBar::new();
            let node = bar.view_from_config(&config);
            let plain = render_to_plain_text(&node, 80);
            assert!(plain.contains("hello"), "Should contain text: {}", plain);
        }

        #[test]
        fn config_with_mode_spacer_model() {
            let config = make_config(
                vec![
                    mode_component(),
                    StatuslineComponent::Spacer,
                    StatuslineComponent::Model {
                        max_length: None,
                        fallback: None,
                        style: StyleSpec::default(),
                    },
                ],
                vec![],
                vec![],
            );
            let bar = StatusBar::new().mode(ChatMode::Plan).model("claude-3");
            let node = bar.view_from_config(&config);
            let plain = render_to_plain_text(&node, 120);
            assert!(plain.contains("PLAN"), "Should contain mode: {}", plain);
            assert!(
                plain.contains("claude-3"),
                "Should contain model: {}",
                plain
            );
        }

        #[test]
        fn empty_config_produces_empty_row() {
            let config = make_config(vec![], vec![], vec![]);
            let bar = StatusBar::new();
            let node = bar.view_from_config(&config);
            let plain = render_to_plain_text(&node, 80);
            assert!(
                !plain.contains("NORMAL") && !plain.contains("ctx"),
                "Empty config should produce empty output: {}",
                plain
            );
        }

        #[test]
        fn config_with_custom_separator() {
            let config = StatuslineConfig {
                left: vec![
                    StatuslineComponent::Text {
                        content: "A".to_string(),
                        style: StyleSpec::default(),
                    },
                    StatuslineComponent::Text {
                        content: "B".to_string(),
                        style: StyleSpec::default(),
                    },
                ],
                center: vec![],
                right: vec![],
                separator: Some(" | ".to_string()),
            };
            let bar = StatusBar::new();
            let node = bar.view_from_config(&config);
            let plain = render_to_plain_text(&node, 80);
            assert!(
                plain.contains("A") && plain.contains("|") && plain.contains("B"),
                "Should contain separator between components: {}",
                plain
            );
        }
    }
}
