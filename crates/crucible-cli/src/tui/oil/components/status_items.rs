//! Evaluating statusline item trees into oil nodes.
//!
//! Runs every frame, TUI-side. That is the point of the split: everything a bar
//! normally shows — mode, model, context usage, notifications — is a fact the
//! TUI already holds, so drawing it costs no RPC. Only [`StatusItem::Expr`]
//! comes from the daemon, and it is read from a cache that a push updates rather
//! than pulled per frame.

use crucible_lua::statusline_items::{StatusCond, StatusItem};
use crucible_oil::node::{row, spacer, styled, Node};
use crucible_oil::style::Style;

use super::status_bar::NotificationToastKind;
use crate::tui::oil::theme;
use crate::tui::oil::utils::truncate_to_chars;

/// The frame's worth of TUI-local state a bar can read.
#[derive(Debug, Clone)]
pub struct StatusBarData {
    /// Mode id, not an enum: see `chat_app::state::DEFAULT_MODE`.
    pub mode: String,
    pub model: String,
    pub context_used: usize,
    pub context_total: usize,
    pub status: String,
    pub notification_toast: Option<(String, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
    pub cache_hit_rate: Option<f64>,
}

/// Everything an item tree can read.
pub struct ItemContext<'a> {
    pub data: &'a StatusBarData,
    pub streaming: bool,
    /// Values pushed from daemon-side providers, keyed by `sl.expr` key.
    pub exprs: &'a std::collections::BTreeMap<String, String>,
}

/// One evaluated fragment: its text and the style to draw it with.
///
/// Items evaluate to a *list* of these, not one, because some render several
/// styled pieces — a notification is its text plus a reversed severity badge.
#[derive(Debug, Clone, PartialEq)]
struct Fragment {
    text: String,
    style: Style,
    /// Keep natural width when the row overflows, so short badges are not
    /// ellipsized away in favour of long prose.
    no_shrink: bool,
    /// An active notification claims the gutter column. A toast is transient and
    /// benefits from the extra cell more than the bar benefits from breathing
    /// room; the pre-item renderer made the same exception.
    claims_gutter: bool,
}

impl Fragment {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            no_shrink: false,
            claims_gutter: false,
        }
    }

    fn badge(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            no_shrink: true,
            claims_gutter: false,
        }
    }
}

/// Evaluate a bar into a node.
///
/// `Align` splits the bar into sections; a single `Align` pushes everything
/// after it to the right, two give left/centre/right.
pub fn render_bar(items: &[StatusItem], ctx: &ItemContext<'_>) -> Node {
    fn to_node(frag: &Fragment) -> Node {
        let node = styled(&frag.text, frag.style);
        if frag.no_shrink {
            node.no_shrink()
        } else {
            node
        }
    }

    // One node per *item*, not per fragment. An item that renders several
    // fragments (a notification is text plus a badge) becomes a nested row, so
    // overflow shrinks it as a unit. Flattening changes how taffy distributes
    // the deficit and silently reflows every other item.
    let mut sections: Vec<Vec<Node>> = vec![Vec::new()];
    let mut last_claims_gutter = false;

    for item in items {
        if matches!(item, StatusItem::Align) {
            sections.push(Vec::new());
            continue;
        }
        let frags = eval(item, ctx, Style::default());
        if frags.is_empty() {
            continue;
        }
        last_claims_gutter = frags.last().is_some_and(|f| f.claims_gutter);

        let node = if frags.len() == 1 {
            to_node(&frags[0])
        } else {
            row(frags.iter().map(to_node).collect::<Vec<_>>())
        };
        sections
            .last_mut()
            .expect("sections always has at least one entry")
            .push(node);
    }

    let mut children: Vec<Node> = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            children.push(spacer());
        }
        children.extend(section.iter().cloned());
    }

    // One-column right gutter, so right-aligned content is not jammed against
    // the terminal edge. Deliberate, and carried over from the pre-item
    // renderer — it is not an off-by-one. An active notification claims that
    // column instead, which is the same exception the old renderer made.
    let has_right_section = sections.len() > 1 && sections.last().is_some_and(|s| !s.is_empty());
    if has_right_section && !last_claims_gutter {
        children.push(styled(" ", Style::default()));
    }

    row(children)
}

/// Evaluate one item into zero or more fragments.
///
/// An empty list means "renders nothing", which is what makes
/// [`StatusItem::Any`] fall through and an unset expression occupy no space.
fn eval(item: &StatusItem, ctx: &ItemContext<'_>, inherited: Style) -> Vec<Fragment> {
    let text_frag = |text: String| {
        if text.is_empty() {
            vec![]
        } else {
            vec![Fragment::new(text, inherited)]
        }
    };

    match item {
        // The mode badge is colour-coded per mode, which one highlight group
        // cannot express. So it carries its built-in styling unless a theme
        // explicitly wraps it — same contract as `Notification` below.
        // `badge`, not `new`: the mode indicator must survive narrow widths
        // intact rather than ellipsize to a single glyph.
        StatusItem::Mode => vec![Fragment::badge(
            mode_label(ctx),
            builtin_or(inherited, || mode_style(ctx)),
        )],

        StatusItem::Model { max, fallback } => {
            let model = &ctx.data.model;
            let text = if model.is_empty() {
                // A session with no model yet still needs to occupy its slot,
                // or the bar reflows the moment one is chosen.
                fallback.clone().unwrap_or_else(|| "...".to_string())
            } else {
                truncate_to_chars(model, max.map_or(20, usize::from), true).into_owned()
            };
            vec![Fragment::new(
                text,
                builtin_or(inherited, || {
                    let t = theme::active();
                    Style::new().fg(t.resolve_color(t.colors.primary))
                }),
            )]
        }

        StatusItem::Context => vec![Fragment::new(
            context_label(ctx),
            builtin_or(inherited, || {
                let t = theme::active();
                Style::new().fg(t.resolve_color(t.colors.text_muted))
            }),
        )],

        StatusItem::Cache => text_frag(match ctx.data.cache_hit_rate {
            // Before any completion reports cache counts there is no rate to
            // show — `cache: 0%` would be a lie, not a zero.
            Some(rate) => format!("cache: {}%", (rate * 100.0).round() as u32),
            None => String::new(),
        }),

        StatusItem::Status => text_frag(ctx.data.status.clone()),

        // Renders as message plus a reversed severity badge, or — with no
        // active toast — one badge per pending count. Both shapes are carried
        // over from the pre-item renderer.
        StatusItem::Notification => {
            let t = theme::active();
            let claim_last = |mut frags: Vec<Fragment>| {
                if let Some(last) = frags.last_mut() {
                    last.claims_gutter = true;
                }
                frags
            };

            if let Some((text, kind)) = &ctx.data.notification_toast {
                return claim_last(vec![
                    Fragment::new(
                        text.clone(),
                        builtin_or(inherited, || {
                            Style::new().fg(t.resolve_color(t.colors.overlay_bright))
                        }),
                    ),
                    Fragment::new(" ", Style::new()),
                    Fragment::badge(
                        format!(" {} ", kind.label()),
                        Style::new().fg(kind.color()).bold().reverse(),
                    ),
                ]);
            }

            claim_last(
                ctx.data
                    .notification_counts
                    .iter()
                    .flat_map(|(kind, count)| {
                        [
                            Fragment::badge(
                                format!(" {} ", kind.label()),
                                Style::new().fg(kind.color()).bold().reverse(),
                            ),
                            Fragment::new(
                                format!(" {count} "),
                                Style::new().fg(kind.color()).bold(),
                            ),
                        ]
                    })
                    .collect(),
            )
        }

        StatusItem::Text(s) => text_frag(s.clone()),

        // Handled by `render_bar`; reaching here means it was nested inside a
        // combinator, where it has no meaning.
        StatusItem::Align => vec![],

        StatusItem::Any(items) => items
            .iter()
            .map(|i| eval(i, ctx, inherited))
            .find(|frags| !frags.is_empty())
            .unwrap_or_default(),

        StatusItem::When { cond, item } => {
            if cond_holds(cond, ctx) {
                eval(item, ctx, inherited)
            } else {
                vec![]
            }
        }

        StatusItem::Hl { group, item } => {
            let style = theme::groups::get(group).map_or(inherited, |hl| {
                let mut s = Style::new();
                if let Some(fg) = hl.fg {
                    s = s.fg(fg);
                }
                if let Some(bg) = hl.bg {
                    s = s.bg(bg);
                }
                if hl.bold {
                    s = s.bold();
                }
                if hl.dim {
                    s = s.dim();
                }
                s
            });
            eval(item, ctx, style)
        }

        StatusItem::Expr { key } => text_frag(ctx.exprs.get(key).cloned().unwrap_or_default()),
    }
}

/// Use the inherited style when a highlight group supplied one, otherwise the
/// item's built-in. This is what lets `sl.mode` keep its colour-coded badge and
/// `sl.mode:hl("X")` take the theme's instead.
fn builtin_or(inherited: Style, builtin: impl FnOnce() -> Style) -> Style {
    if inherited == Style::default() {
        builtin()
    } else {
        inherited
    }
}

fn cond_holds(cond: &StatusCond, ctx: &ItemContext<'_>) -> bool {
    match cond {
        StatusCond::Streaming => ctx.streaming,
        StatusCond::HasNotification => ctx.data.notification_toast.is_some(),
        StatusCond::ModeIs(name) => mode_name(ctx).eq_ignore_ascii_case(name),
    }
}

fn mode_name<'a>(ctx: &'a ItemContext<'_>) -> &'a str {
    &ctx.data.mode
}

/// The per-mode badge styling, matching what the pre-item statusline drew.
fn mode_style(ctx: &ItemContext<'_>) -> Style {
    crate::tui::oil::chat_app::mode_style(&ctx.data.mode)
}

fn mode_label(ctx: &ItemContext<'_>) -> String {
    crate::tui::oil::chat_app::mode_label(&ctx.data.mode)
}

fn context_label(ctx: &ItemContext<'_>) -> String {
    let (used, total) = (ctx.data.context_used, ctx.data.context_total);
    if total > 0 {
        format!(
            "{}% ctx",
            (used as f64 / total as f64 * 100.0).round() as usize
        )
    } else if used > 0 {
        format!("{}k tok", used / 1000)
    } else {
        // Not empty: the slot holds its width so the bar does not reflow once
        // the first token count arrives.
        "— ctx".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_plain_text;
    use std::collections::BTreeMap;

    fn data() -> StatusBarData {
        StatusBarData {
            mode: "normal".into(),
            model: "claude-opus-5".to_string(),
            context_used: 5_000,
            context_total: 10_000,
            status: String::new(),
            notification_toast: None,
            notification_counts: Vec::new(),
            cache_hit_rate: None,
        }
    }

    fn render(items: &[StatusItem], data: &StatusBarData, streaming: bool) -> String {
        let exprs = BTreeMap::new();
        let ctx = ItemContext {
            data,
            streaming,
            exprs: &exprs,
        };
        render_to_plain_text(&render_bar(items, &ctx), 80)
    }

    fn render_with_exprs(items: &[StatusItem], exprs: BTreeMap<String, String>) -> String {
        let d = data();
        let ctx = ItemContext {
            data: &d,
            streaming: false,
            exprs: &exprs,
        };
        render_to_plain_text(&render_bar(items, &ctx), 80)
    }

    #[test]
    fn built_ins_render_from_tui_local_state() {
        let out = render(
            &[
                StatusItem::Mode,
                StatusItem::Model {
                    max: None,
                    fallback: None,
                },
            ],
            &data(),
            false,
        );
        assert!(out.contains("NORMAL"), "got {out:?}");
        assert!(out.contains("claude-opus-5"), "got {out:?}");
    }

    #[test]
    fn model_is_truncated_to_its_max() {
        let out = render(
            &[StatusItem::Model {
                max: Some(8),
                fallback: None,
            }],
            &data(),
            false,
        );
        assert!(!out.contains("claude-opus-5"), "got {out:?}");
    }

    #[test]
    fn context_renders_as_a_percentage() {
        assert!(render(&[StatusItem::Context], &data(), false).contains("50% ctx"));
    }

    /// The fallback the old bespoke `Notification { fallback }` variant encoded.
    #[test]
    fn any_falls_through_to_the_next_item_when_the_first_is_empty() {
        let items = [StatusItem::Any(vec![
            StatusItem::Notification,
            StatusItem::Context,
        ])];
        assert!(render(&items, &data(), false).contains("50% ctx"));
    }

    #[test]
    fn any_prefers_the_first_item_that_renders() {
        let mut d = data();
        d.notification_toast = Some(("saved".to_string(), NotificationToastKind::Info));

        let items = [StatusItem::Any(vec![
            StatusItem::Notification,
            StatusItem::Context,
        ])];
        let out = render(&items, &d, false);
        assert!(out.contains("saved"), "got {out:?}");
        assert!(!out.contains("ctx"), "the fallback must not also render");
    }

    #[test]
    fn when_hides_its_item_unless_the_condition_holds() {
        let items = [StatusItem::When {
            cond: StatusCond::Streaming,
            item: Box::new(StatusItem::Text("busy".into())),
        }];

        assert!(!render(&items, &data(), false).contains("busy"));
        assert!(render(&items, &data(), true).contains("busy"));
    }

    #[test]
    fn mode_conditions_compare_against_the_active_mode() {
        let mut d = data();
        d.mode = "plan".into();

        let items = [StatusItem::When {
            cond: StatusCond::ModeIs("plan".into()),
            item: Box::new(StatusItem::Text("planning".into())),
        }];
        assert!(render(&items, &d, false).contains("planning"));

        d.mode = "normal".into();
        assert!(!render(&items, &d, false).contains("planning"));
    }

    /// An unset expression must render nothing, so a bar does not jump when a
    /// provider first pushes.
    #[test]
    fn an_unset_expression_renders_nothing() {
        let items = [
            StatusItem::Text("[".into()),
            StatusItem::Expr { key: "git".into() },
            StatusItem::Text("]".into()),
        ];
        assert!(render_with_exprs(&items, BTreeMap::new()).contains("[]"));
    }

    #[test]
    fn a_pushed_expression_renders_its_value() {
        let mut exprs = BTreeMap::new();
        exprs.insert("git".to_string(), "main*".to_string());

        let items = [StatusItem::Expr { key: "git".into() }];
        assert!(render_with_exprs(&items, exprs).contains("main*"));
    }

    #[test]
    fn align_splits_the_bar_into_sections() {
        let items = [
            StatusItem::Text("L".into()),
            StatusItem::Align,
            StatusItem::Text("R".into()),
        ];
        let out = render(&items, &data(), false);
        let l = out.find('L').expect("left section rendered");
        let r = out.find('R').expect("right section rendered");
        assert!(r > l + 1, "align should push R away from L: {out:?}");
    }

    #[test]
    fn cache_renders_nothing_before_any_rate_is_known() {
        assert!(!render(&[StatusItem::Cache], &data(), false).contains("cache"));
    }

    #[test]
    fn cache_renders_a_percentage_once_known() {
        let mut d = data();
        d.cache_hit_rate = Some(0.75);
        assert!(render(&[StatusItem::Cache], &d, false).contains("cache: 75%"));
    }

    #[test]
    fn an_empty_bar_renders_an_empty_row() {
        assert!(render(&[], &data(), false).trim().is_empty());
    }

    /// A one-column gutter keeps right-aligned content off the terminal edge.
    /// Carried over from the pre-item renderer, which appended it explicitly.
    #[test]
    fn a_right_aligned_section_keeps_a_one_column_gutter() {
        let items = [
            StatusItem::Text("L".into()),
            StatusItem::Align,
            StatusItem::Text("R".into()),
        ];
        let exprs = BTreeMap::new();
        let d = data();
        let ctx = ItemContext {
            data: &d,
            streaming: false,
            exprs: &exprs,
        };
        let out = render_to_plain_text(&render_bar(&items, &ctx), 20);
        let line = out.lines().next().expect("one line");
        // The gutter is trailing whitespace, which plain-text rendering trims —
        // so its presence shows as content stopping one column short.
        assert_eq!(
            line.len(),
            19,
            "right content should stop one column short of the 20-wide edge: {line:?}"
        );
        assert!(line.ends_with('R'), "right section rendered: {line:?}");
    }

    /// The built-in default must be renderable as-is — it is what every user
    /// sees before they write any config.
    #[test]
    fn the_builtin_default_bar_renders() {
        let bars = crucible_lua::statusline_items::builtin_default();
        let items = match &bars.prompt[1] {
            crucible_lua::statusline_items::Element::Row(items) => items,
            other => panic!("the built-in places a row after the input: {other:?}"),
        };
        let out = render(items, &data(), false);
        assert!(out.contains("NORMAL"), "got {out:?}");
        assert!(out.contains("claude-opus-5"), "got {out:?}");
        assert!(out.contains("50% ctx"), "got {out:?}");
    }
}
