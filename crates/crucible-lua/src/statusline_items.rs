//! Statusline item trees — the authoring surface for status bars.
//!
//! Rejected alternative: `%X` format strings. Sigils need a parser and escaping
//! rules, and they force a distinction between "insert as literal text" and
//! "re-interpret as format" (Neovim's `%{}` versus `%{%…%}`). That distinction is
//! exactly where ANSI injection lives — a branch name or model-derived string
//! reaching a status bar could carry cursor-movement or OSC sequences. Named
//! items delete the *re-parsing* hazard: styling is always structural, and
//! nothing an expression returns is ever re-interpreted as markup.
//!
//! That is a narrower property than "text is safe", and the difference matters.
//! A literal is authored but not necessarily typed — a config that writes
//! `{ "on " .. branch }` carries the same attacker-influenced data an
//! expression does. So text is sanitized on the way in, at both parse points,
//! rather than trusted for having come from a config file.
//!
//! ```lua
//! local sl = crucible.statusline
//! sl.setup{
//!   prompt = {
//!     sl.input,
//!     { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
//!       sl.align,
//!       sl.any(sl.notification, sl.context) },
//!   },
//! }
//! ```
//!
//! A [`Region`] (`top`, `prompt`, `bottom`) is an ordered list of rows;
//! position in the list is the arrangement. There are no named bars or
//! anchors — that model (`main = { anchor = ..., items = ... }`) was
//! replaced, see [`Region`].
//!
//! Evaluation splits by who owns the fact. Built-ins are TUI-local and cost no
//! RPC — the TUI already knows the mode, the model, the context usage. Only
//! [`StatusItem::Expr`] needs the daemon, and it is push-cached rather than
//! pulled per frame.

use serde_json::{json, Map, Value as Json};

/// A screen region. A **closed** set: the widget tree stays private, so it can
/// be restructured without breaking configs. Opening this later is additive.
///
/// Regions replaced an earlier `Anchor` + `order` pair. An anchor said *where*
/// but not *in what order*, so multiple bars sharing one needed a priority
/// integer — and neither available source of order worked: a name-keyed map
/// sorts alphabetically, and a Lua table with string keys does not iterate in
/// declaration order at all. A region that is simply an ordered **list** makes
/// position the arrangement, which is the same answer tmux reached with
/// `status-format[0]`, `[1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Top,
    Prompt,
    Bottom,
}

impl Region {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "top" => Some(Region::Top),
            "prompt" => Some(Region::Prompt),
            "bottom" => Some(Region::Bottom),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Region::Top => "top",
            Region::Prompt => "prompt",
            Region::Bottom => "bottom",
        }
    }
}

/// A condition the TUI can answer locally.
///
/// These exist because the daemon cannot see them. Lua **places** them; the TUI
/// **populates** them. That division is what keeps a pushed model honest about
/// TUI-local state instead of pretending it does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusCond {
    Streaming,
    HasNotification,
    ModeIs(String),
}

impl StatusCond {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "streaming" => Some(StatusCond::Streaming),
            "has_notification" => Some(StatusCond::HasNotification),
            other => other
                .strip_prefix("mode:")
                .map(|m| StatusCond::ModeIs(m.to_string())),
        }
    }

    pub fn name(&self) -> String {
        match self {
            StatusCond::Streaming => "streaming".into(),
            StatusCond::HasNotification => "has_notification".into(),
            StatusCond::ModeIs(m) => format!("mode:{m}"),
        }
    }
}

/// One node of a bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusItem {
    // ── Built-ins: TUI-local, evaluated every frame, zero RPC ──
    Mode,
    Model {
        max: Option<u16>,
        /// Shown when no model is selected yet, so the slot holds its width.
        fallback: Option<String>,
    },
    Context,
    Cache,
    Status,
    Notification,
    Text(String),
    /// Alignment split. One pushes the rest right; two give left/centre/right.
    Align,

    // ── Combinators ──
    /// First child that renders non-empty wins. This is how the built-in
    /// "notification, else context usage" default is expressed without a
    /// special case, and it must be evaluated TUI-side because both operands
    /// are TUI-local.
    Any(Vec<StatusItem>),
    /// Render the child only when the condition holds.
    When {
        cond: StatusCond,
        item: Box<StatusItem>,
    },
    /// Style the child with a highlight group.
    Hl {
        group: String,
        item: Box<StatusItem>,
    },

    // ── Daemon-pushed ──
    /// A value computed daemon-side and pushed. Renders nothing when unset, so
    /// a bar does not jump as values arrive.
    Expr {
        key: String,
    },
}

/// One entry in a region's list.
///
/// The input is an element rather than a fixed landmark bars hang off, which is
/// what lets an author put rows above or below it by writing them above or
/// below it. It carries no fields: the input is a whole component with its own
/// focus, cursor, popup and multi-line border, so this is a placeholder the
/// renderer substitutes, not something expressible as [`StatusItem`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    Row(Vec<StatusItem>),
    Input,
}

/// The screen, as three ordered lists.
///
/// Only [`Region::Prompt`] may hold [`Element::Input`]; the type does not
/// prevent it elsewhere, so [`Layout::from_wire`] drops a stray one rather than
/// rendering two inputs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    pub top: Vec<Element>,
    pub prompt: Vec<Element>,
    pub bottom: Vec<Element>,
}

impl Layout {
    pub fn region(&self, region: Region) -> &[Element] {
        match region {
            Region::Top => &self.top,
            Region::Prompt => &self.prompt,
            Region::Bottom => &self.bottom,
        }
    }

    pub fn region_mut(&mut self, region: Region) -> &mut Vec<Element> {
        match region {
            Region::Top => &mut self.top,
            Region::Prompt => &mut self.prompt,
            Region::Bottom => &mut self.bottom,
        }
    }

    /// Rows sitting below the input, which is what the completion popup has to
    /// clear. Counted rather than hardcoded — the popup used to reserve a
    /// constant that was only ever right for a one-row footer.
    pub fn rows_below_input(&self) -> usize {
        match self.prompt.iter().position(|e| *e == Element::Input) {
            Some(at) => self.prompt.len() - at - 1,
            // No input placed: nothing sits below it.
            None => 0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire
// ─────────────────────────────────────────────────────────────────────────────

pub fn item_to_wire(item: &StatusItem) -> Json {
    match item {
        StatusItem::Mode => json!({ "t": "mode" }),
        StatusItem::Model { max, fallback } => {
            let mut m = Map::new();
            m.insert("t".into(), json!("model"));
            if let Some(n) = max {
                m.insert("max".into(), json!(n));
            }
            if let Some(f) = fallback {
                m.insert("fallback".into(), json!(f));
            }
            Json::Object(m)
        }
        StatusItem::Context => json!({ "t": "context" }),
        StatusItem::Cache => json!({ "t": "cache" }),
        StatusItem::Status => json!({ "t": "status" }),
        StatusItem::Notification => json!({ "t": "notification" }),
        StatusItem::Text(s) => json!({ "t": "text", "v": s }),
        StatusItem::Align => json!({ "t": "align" }),
        StatusItem::Any(items) => {
            json!({ "t": "any", "items": items.iter().map(item_to_wire).collect::<Vec<_>>() })
        }
        StatusItem::When { cond, item } => {
            json!({ "t": "when", "cond": cond.name(), "item": item_to_wire(item) })
        }
        StatusItem::Hl { group, item } => {
            json!({ "t": "hl", "group": group, "item": item_to_wire(item) })
        }
        StatusItem::Expr { key } => json!({ "t": "expr", "key": key }),
    }
}

/// Parse one item. `None` for anything unrecognised, so a newer daemon's item
/// type is dropped from the bar rather than failing the whole payload.
pub fn item_from_wire(v: &Json) -> Option<StatusItem> {
    let t = v.get("t")?.as_str()?;
    Some(match t {
        "mode" => StatusItem::Mode,
        "model" => StatusItem::Model {
            max: v
                .get("max")
                .and_then(Json::as_u64)
                .and_then(|n| u16::try_from(n).ok()),
            fallback: v
                .get("fallback")
                .and_then(Json::as_str)
                .map(std::string::ToString::to_string),
        },
        "context" => StatusItem::Context,
        "cache" => StatusItem::Cache,
        "status" => StatusItem::Status,
        "notification" => StatusItem::Notification,
        "text" => StatusItem::Text(crate::statusline_exprs::sanitize_uncapped(
            v.get("v")?.as_str()?,
        )),
        "align" => StatusItem::Align,
        "any" => StatusItem::Any(
            v.get("items")?
                .as_array()?
                .iter()
                .filter_map(item_from_wire)
                .collect(),
        ),
        "when" => StatusItem::When {
            cond: StatusCond::from_name(v.get("cond")?.as_str()?)?,
            item: Box::new(item_from_wire(v.get("item")?)?),
        },
        "hl" => StatusItem::Hl {
            group: v.get("group")?.as_str()?.to_string(),
            item: Box::new(item_from_wire(v.get("item")?)?),
        },
        "expr" => StatusItem::Expr {
            key: v.get("key")?.as_str()?.to_string(),
        },
        _ => return None,
    })
}

fn element_to_wire(e: &Element) -> Json {
    match e {
        Element::Input => json!({ "t": "input" }),
        Element::Row(items) => {
            json!({ "t": "row", "items": items.iter().map(item_to_wire).collect::<Vec<_>>() })
        }
    }
}

fn element_from_wire(v: &Json) -> Option<Element> {
    match v.get("t")?.as_str()? {
        "input" => Some(Element::Input),
        "row" => Some(Element::Row(
            v.get("items")?
                .as_array()?
                .iter()
                .filter_map(item_from_wire)
                .collect(),
        )),
        _ => None,
    }
}

impl Layout {
    pub fn to_wire(&self) -> Json {
        let region = |elements: &[Element]| {
            Json::Array(elements.iter().map(element_to_wire).collect::<Vec<_>>())
        };
        json!({
            "top": region(&self.top),
            "prompt": region(&self.prompt),
            "bottom": region(&self.bottom),
        })
    }

    /// Parse a layout. An unreadable element is dropped rather than failing the
    /// payload, matching how unknown items are handled — one bad row must not
    /// cost the rest of the screen.
    pub fn from_wire(v: &Json) -> Self {
        let region = |key: &str| -> Vec<Element> {
            v.get(key)
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(element_from_wire).collect())
                .unwrap_or_default()
        };

        // Two inputs would render two editors sharing one buffer — and
        // `rows_below_input` counts from the first, so the popup would sit a
        // row short and paint over the second. The first wins; the rest are
        // dropped. Erroring here would take the whole screen down for a
        // mistake the author can see at a glance.
        let keep_first_input = |elements: Vec<Element>| {
            let mut seen = false;
            elements
                .into_iter()
                .filter(|e| {
                    if *e != Element::Input {
                        return true;
                    }
                    if seen {
                        tracing::warn!(
                            "the prompt region places more than one input; keeping the first"
                        );
                        return false;
                    }
                    seen = true;
                    true
                })
                .collect()
        };

        // A stray input outside the prompt region is dropped for the same
        // reason.
        let strip_input = |elements: Vec<Element>, region: &str| {
            elements
                .into_iter()
                .filter(|e| {
                    if *e == Element::Input {
                        tracing::warn!("the input can only sit in the prompt region, not {region}; dropping it");
                        return false;
                    }
                    true
                })
                .collect()
        };

        Self {
            top: strip_input(region("top"), "top"),
            prompt: keep_first_input(region("prompt")),
            bottom: strip_input(region("bottom"), "bottom"),
        }
    }
}

/// The layout Crucible ships with: the input, and one bar under it carrying
/// mode and model on the left with a notification that falls back to context
/// usage on the right.
pub fn builtin_default() -> Layout {
    Layout {
        top: vec![],
        prompt: vec![
            Element::Input,
            Element::Row(vec![
                StatusItem::Hl {
                    group: "StatusMode".to_string(),
                    item: Box::new(StatusItem::Mode),
                },
                StatusItem::Text(" ".to_string()),
                StatusItem::Model {
                    max: Some(25),
                    fallback: None,
                },
                StatusItem::Align,
                StatusItem::Any(vec![StatusItem::Notification, StatusItem::Context]),
            ]),
        ],
        bottom: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &str) -> Element {
        Element::Row(vec![StatusItem::Text(text.into())])
    }

    /// Position in the list is the arrangement — there is nothing else to
    /// consult, which is the whole point of dropping anchor+order.
    #[test]
    fn a_region_keeps_the_order_it_was_written_in() {
        let layout = Layout {
            prompt: vec![row("first"), Element::Input, row("last")],
            ..Layout::default()
        };

        let restored = Layout::from_wire(&layout.to_wire());
        assert_eq!(restored.prompt, layout.prompt);
        assert_eq!(restored.prompt[1], Element::Input);
    }

    /// The count the popup needs, which used to be a hardcoded constant that
    /// was only ever right for a one-row footer.
    #[test]
    fn rows_below_the_input_are_counted_not_assumed() {
        let none = Layout {
            prompt: vec![Element::Input],
            ..Layout::default()
        };
        assert_eq!(none.rows_below_input(), 0);

        let two = Layout {
            prompt: vec![row("above"), Element::Input, row("a"), row("b")],
            ..Layout::default()
        };
        assert_eq!(
            two.rows_below_input(),
            2,
            "rows above the input do not count"
        );

        // An author who placed no input has nothing below it either.
        let inputless = Layout {
            prompt: vec![row("only")],
            ..Layout::default()
        };
        assert_eq!(inputless.rows_below_input(), 0);
    }

    /// Two inputs would mean two editors; the stray one loses, and the screen
    /// still renders.
    #[test]
    fn an_input_outside_the_prompt_region_is_dropped() {
        let wire = json!({
            "top": [{ "t": "input" }, { "t": "row", "items": [] }],
            "prompt": [{ "t": "input" }],
            "bottom": [{ "t": "input" }],
        });

        let layout = Layout::from_wire(&wire);
        assert_eq!(layout.top.len(), 1, "the row survives, the input does not");
        assert!(!layout.top.contains(&Element::Input));
        assert!(!layout.bottom.contains(&Element::Input));
        assert_eq!(layout.prompt, vec![Element::Input]);
    }

    /// Literal text is authored, but a config can interpolate a branch name
    /// into it — the same untrusted data an expression carries, arriving
    /// through the variant that used to be trusted for its origin.
    #[test]
    fn literal_text_is_sanitized_on_the_way_in() {
        let wire = json!({
            "prompt": [{
                "t": "row",
                "items": [{ "t": "text", "v": "on \u{202E}main\u{001B}[2J" }],
            }],
        });

        let Element::Row(items) = &Layout::from_wire(&wire).prompt[0] else {
            panic!("a row was parsed");
        };
        let StatusItem::Text(text) = &items[0] else {
            panic!("a text item was parsed: {items:?}");
        };
        assert!(
            !text.contains('\u{202E}'),
            "bidi override survived: {text:?}"
        );
        assert!(!text.contains('\u{001B}'), "escape survived: {text:?}");
        assert!(text.starts_with("on "), "legible text kept: {text:?}");
    }

    /// Two inputs would draw two editors over one buffer, and the popup would
    /// measure from the first.
    #[test]
    fn a_second_input_in_the_prompt_region_is_dropped() {
        let wire = json!({
            "prompt": [{ "t": "input" }, { "t": "row", "items": [] }, { "t": "input" }],
        });

        let prompt = Layout::from_wire(&wire).prompt;
        assert_eq!(
            prompt.iter().filter(|e| **e == Element::Input).count(),
            1,
            "exactly one input survives: {prompt:?}"
        );
        assert_eq!(prompt[0], Element::Input, "the first one is the one kept");
    }

    #[test]
    fn an_unreadable_element_is_dropped_not_fatal() {
        let wire = json!({
            "prompt": [{ "t": "input" }, { "t": "no-such-element" }],
        });
        assert_eq!(Layout::from_wire(&wire).prompt, vec![Element::Input]);
    }

    #[test]
    fn region_names_round_trip() {
        for r in [Region::Top, Region::Prompt, Region::Bottom] {
            assert_eq!(Region::from_name(r.name()), Some(r));
        }
        assert_eq!(Region::from_name("footer.below_input"), None);
    }

    #[test]
    fn the_builtin_default_survives_a_wire_round_trip() {
        let layout = builtin_default();
        assert_eq!(Layout::from_wire(&layout.to_wire()), layout);
    }

    #[test]
    fn nested_combinators_survive_a_wire_round_trip() {
        let item = StatusItem::Hl {
            group: "G".into(),
            item: Box::new(StatusItem::Any(vec![
                StatusItem::When {
                    cond: StatusCond::Streaming,
                    item: Box::new(StatusItem::Expr { key: "git".into() }),
                },
                StatusItem::Context,
            ])),
        };
        assert_eq!(item_from_wire(&item_to_wire(&item)), Some(item));
    }

    #[test]
    fn mode_conditions_round_trip_with_their_argument() {
        let c = StatusCond::ModeIs("plan".into());
        assert_eq!(StatusCond::from_name(&c.name()), Some(c));
    }

    /// A newer daemon may send an item type this client cannot draw. Drop that
    /// item, keep the bar.
    #[test]
    fn an_unknown_item_type_is_dropped_not_fatal() {
        let wire = json!({
            "prompt": [{
                "t": "row",
                "items": [ { "t": "mode" }, { "t": "hologram" }, { "t": "context" } ],
            }],
        });

        assert_eq!(
            Layout::from_wire(&wire).prompt,
            vec![Element::Row(vec![StatusItem::Mode, StatusItem::Context])]
        );
    }
}
