use crucible_lua::statusline::{ColorSpec, StatuslineComponent, StyleSpec};
use crucible_oil::node::{row, spacer, styled, Node};
use crucible_oil::style::{Color, Style};

use crate::tui::oil::chat_app::ChatMode;
use crate::tui::oil::components::status_bar::NotificationToastKind;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::utils::truncate_to_chars;

pub struct StatusBarData {
    pub mode: ChatMode,
    pub model: String,
    pub context_used: usize,
    pub context_total: usize,
    pub status: String,
    pub notification_toast: Option<(String, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
}

pub fn color_spec_to_oil(spec: &ColorSpec) -> Option<Color> {
    match spec {
        ColorSpec::Named(name) => match name.to_lowercase().as_str() {
            "black" => Some(Color::Black),
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "yellow" => Some(Color::Yellow),
            "blue" => Some(Color::Blue),
            "magenta" => Some(Color::Magenta),
            "cyan" => Some(Color::Cyan),
            "white" => Some(Color::White),
            "gray" | "grey" => Some(Color::Gray),
            "darkgray" | "darkgrey" => Some(Color::DarkGray),
            "reset" => Some(Color::Reset),
            _ => None,
        },
        ColorSpec::Hex(hex) => parse_hex_color(hex),
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

pub fn style_spec_to_oil(spec: &StyleSpec, fallback: Style) -> Style {
    let mut style = fallback;
    if let Some(ref fg_spec) = spec.fg {
        if let Some(color) = color_spec_to_oil(fg_spec) {
            style.fg = Some(color);
        }
    }
    if let Some(ref bg_spec) = spec.bg {
        if let Some(color) = color_spec_to_oil(bg_spec) {
            style.bg = Some(color);
        }
    }
    if spec.bold {
        style.bold = true;
    }
    style
}

pub fn render_component_node(component: &StatuslineComponent, data: &StatusBarData) -> Node {
    let theme = ThemeTokens::default_ref();
    match component {
        StatuslineComponent::Mode { .. } => {
            let (label, style) = match data.mode {
                ChatMode::Normal => (" NORMAL ", theme.mode_normal_style()),
                ChatMode::Plan => (" PLAN ", theme.mode_plan_style()),
                ChatMode::Auto => (" AUTO ", theme.mode_auto_style()),
            };
            styled(label.to_string(), style)
        }
        StatuslineComponent::Model {
            max_length,
            fallback,
            ..
        } => {
            let max_len = max_length.unwrap_or(20);
            let display = if data.model.is_empty() {
                fallback.as_deref().unwrap_or("...").to_string()
            } else {
                truncate_to_chars(&data.model, max_len, true).into_owned()
            };
            styled(display, theme.model_name_style())
        }
        StatuslineComponent::Context { .. } => {
            let display = if data.context_total > 0 {
                let percent =
                    (data.context_used as f64 / data.context_total as f64 * 100.0).round() as usize;
                format!("{}% ctx", percent)
            } else if data.context_used > 0 {
                format!("{}k tok", data.context_used / 1000)
            } else {
                String::new()
            };
            styled(display, theme.muted())
        }
        StatuslineComponent::Text { content, .. } => styled(content.clone(), theme.muted()),
        StatuslineComponent::Spacer => spacer(),
        StatuslineComponent::Notification { fallback, .. } => {
            let mut items = Vec::new();
            if let Some((text, kind)) = &data.notification_toast {
                items.push(styled(text.clone(), theme.overlay_bright_style()));
                items.push(styled(" ".to_string(), Style::new()));
                items.push(styled(
                    format!(" {} ", kind.label()),
                    theme.notification_badge(kind.color()),
                ));
            } else if !data.notification_counts.is_empty() {
                for (kind, count) in &data.notification_counts {
                    items.push(styled(
                        format!(" {} ", kind.label()),
                        theme.notification_badge(kind.color()),
                    ));
                    items.push(styled(
                        format!(" {} ", count),
                        Style::new().fg(kind.color()).bold(),
                    ));
                }
            } else if let Some(fb) = fallback {
                return render_component_node(fb, data);
            }
            row(items)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::statusline::{ModeStyleSpec, StatuslineComponent};
    use crucible_oil::node::Node;

    fn default_data() -> StatusBarData {
        StatusBarData {
            mode: ChatMode::Normal,
            model: "gpt-4o".to_string(),
            context_used: 4000,
            context_total: 8000,
            status: String::new(),
            notification_toast: None,
            notification_counts: vec![],
        }
    }

    fn default_mode_component() -> StatuslineComponent {
        StatuslineComponent::Mode {
            normal: ModeStyleSpec::default(),
            plan: ModeStyleSpec::default(),
            auto: ModeStyleSpec::default(),
        }
    }

    fn node_contains_text(node: &Node, needle: &str) -> bool {
        let rendered = crucible_oil::render::render_to_string(node, 120);
        rendered.contains(needle)
    }

    #[test]
    fn color_spec_named_green() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Named("green".into())),
            Some(Color::Green)
        );
    }

    #[test]
    fn color_spec_named_darkgray() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Named("darkgray".into())),
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn color_spec_named_darkgrey_alias() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Named("darkgrey".into())),
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn color_spec_named_case_insensitive() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Named("RED".into())),
            Some(Color::Red)
        );
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Named("Blue".into())),
            Some(Color::Blue)
        );
    }

    #[test]
    fn color_spec_hex_red() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Hex("#ff0000".into())),
            Some(Color::Rgb(255, 0, 0))
        );
    }

    #[test]
    fn color_spec_hex_green() {
        assert_eq!(
            color_spec_to_oil(&ColorSpec::Hex("#00ff00".into())),
            Some(Color::Rgb(0, 255, 0))
        );
    }

    #[test]
    fn color_spec_named_unknown() {
        assert_eq!(color_spec_to_oil(&ColorSpec::Named("unknown".into())), None);
    }

    #[test]
    fn color_spec_hex_invalid() {
        assert_eq!(color_spec_to_oil(&ColorSpec::Hex("invalid".into())), None);
    }

    #[test]
    fn color_spec_hex_short() {
        assert_eq!(color_spec_to_oil(&ColorSpec::Hex("#fff".into())), None);
    }

    #[test]
    fn style_spec_with_fg() {
        let spec = StyleSpec {
            fg: Some(ColorSpec::Named("red".into())),
            bg: None,
            bold: false,
        };
        let result = style_spec_to_oil(&spec, Style::new());
        assert_eq!(result.fg, Some(Color::Red));
        assert_eq!(result.bg, None);
        assert!(!result.bold);
    }

    #[test]
    fn style_spec_no_fg_uses_fallback() {
        let fallback = Style::new().fg(Color::Green);
        let spec = StyleSpec {
            fg: None,
            bg: None,
            bold: false,
        };
        let result = style_spec_to_oil(&spec, fallback);
        assert_eq!(result.fg, Some(Color::Green));
    }

    #[test]
    fn style_spec_bold() {
        let spec = StyleSpec {
            fg: None,
            bg: None,
            bold: true,
        };
        let result = style_spec_to_oil(&spec, Style::new());
        assert!(result.bold);
    }

    #[test]
    fn style_spec_with_bg() {
        let spec = StyleSpec {
            fg: None,
            bg: Some(ColorSpec::Hex("#0000ff".into())),
            bold: false,
        };
        let result = style_spec_to_oil(&spec, Style::new());
        assert_eq!(result.bg, Some(Color::Rgb(0, 0, 255)));
    }

    #[test]
    fn style_spec_unknown_color_preserves_fallback() {
        let fallback = Style::new().fg(Color::Cyan);
        let spec = StyleSpec {
            fg: Some(ColorSpec::Named("doesnotexist".into())),
            bg: None,
            bold: false,
        };
        let result = style_spec_to_oil(&spec, fallback);
        assert_eq!(
            result.fg,
            Some(Color::Cyan),
            "Unknown color should preserve fallback"
        );
    }

    #[test]
    fn render_mode_normal() {
        let data = default_data();
        let node = render_component_node(&default_mode_component(), &data);
        assert!(node_contains_text(&node, "NORMAL"));
    }

    #[test]
    fn render_mode_plan() {
        let mut data = default_data();
        data.mode = ChatMode::Plan;
        let node = render_component_node(&default_mode_component(), &data);
        assert!(node_contains_text(&node, "PLAN"));
    }

    #[test]
    fn render_mode_auto() {
        let mut data = default_data();
        data.mode = ChatMode::Auto;
        let node = render_component_node(&default_mode_component(), &data);
        assert!(node_contains_text(&node, "AUTO"));
    }

    #[test]
    fn render_model_with_name() {
        let data = default_data();
        let component = StatuslineComponent::Model {
            max_length: None,
            fallback: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "gpt-4o"));
    }

    #[test]
    fn render_model_truncated() {
        let mut data = default_data();
        data.model = "very-long-model-name-exceeding-limit".to_string();
        let component = StatuslineComponent::Model {
            max_length: Some(10),
            fallback: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(!node_contains_text(
            &node,
            "very-long-model-name-exceeding-limit"
        ));
    }

    #[test]
    fn render_model_empty_uses_fallback() {
        let mut data = default_data();
        data.model = String::new();
        let component = StatuslineComponent::Model {
            max_length: None,
            fallback: Some("no model".to_string()),
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "no model"));
    }

    #[test]
    fn render_model_empty_default_fallback() {
        let mut data = default_data();
        data.model = String::new();
        let component = StatuslineComponent::Model {
            max_length: None,
            fallback: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "..."));
    }

    #[test]
    fn render_context_percentage() {
        let data = default_data();
        let component = StatuslineComponent::Context {
            format: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "50% ctx"));
    }

    #[test]
    fn render_context_tokens_only() {
        let mut data = default_data();
        data.context_total = 0;
        data.context_used = 5000;
        let component = StatuslineComponent::Context {
            format: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "5k tok"));
    }

    #[test]
    fn render_context_empty() {
        let mut data = default_data();
        data.context_total = 0;
        data.context_used = 0;
        let component = StatuslineComponent::Context {
            format: None,
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        let rendered = crucible_oil::render::render_to_string(&node, 80);
        assert!(
            !rendered.contains("ctx") && !rendered.contains("tok"),
            "Empty context should render nothing"
        );
    }

    #[test]
    fn render_text() {
        let data = default_data();
        let component = StatuslineComponent::Text {
            content: "hello world".to_string(),
            style: StyleSpec::default(),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "hello world"));
    }

    #[test]
    fn render_spacer_is_flex_box() {
        let data = default_data();
        let node = render_component_node(&StatuslineComponent::Spacer, &data);
        match node {
            Node::Box(box_node) => {
                assert_eq!(
                    box_node.size,
                    crucible_oil::node::Size::Flex(1),
                    "Spacer should produce a flex(1) box"
                );
            }
            _ => panic!("Spacer component should produce Node::Box with Flex size"),
        }
    }

    #[test]
    fn render_notification_toast() {
        let mut data = default_data();
        data.notification_toast = Some(("Processing".to_string(), NotificationToastKind::Info));
        let component = StatuslineComponent::Notification {
            style: StyleSpec::default(),
            fallback: None,
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "Processing"));
        assert!(node_contains_text(&node, "INFO"));
    }

    #[test]
    fn render_notification_counts() {
        let mut data = default_data();
        data.notification_counts = vec![(NotificationToastKind::Warning, 3)];
        let component = StatuslineComponent::Notification {
            style: StyleSpec::default(),
            fallback: None,
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "WARN"));
        assert!(node_contains_text(&node, "3"));
    }

    #[test]
    fn render_notification_fallback_when_idle() {
        let data = default_data();
        let component = StatuslineComponent::Notification {
            style: StyleSpec::default(),
            fallback: Some(Box::new(StatuslineComponent::Context {
                format: None,
                style: StyleSpec::default(),
            })),
        };
        let node = render_component_node(&component, &data);
        assert!(
            node_contains_text(&node, "50% ctx"),
            "Should render fallback context when no notification"
        );
    }

    #[test]
    fn render_notification_toast_hides_fallback() {
        let mut data = default_data();
        data.notification_toast = Some(("Saving...".to_string(), NotificationToastKind::Info));
        let component = StatuslineComponent::Notification {
            style: StyleSpec::default(),
            fallback: Some(Box::new(StatuslineComponent::Context {
                format: None,
                style: StyleSpec::default(),
            })),
        };
        let node = render_component_node(&component, &data);
        assert!(node_contains_text(&node, "Saving..."));
        assert!(node_contains_text(&node, "INFO"));
        assert!(
            !node_contains_text(&node, "50% ctx"),
            "Toast should hide fallback context"
        );
    }
}
