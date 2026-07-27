//! Statusline item trees — the authoring surface for status bars.
//!
//! Rejected alternative: `%X` format strings. Sigils need a parser and escaping
//! rules, and they force a distinction between "insert as literal text" and
//! "re-interpret as format" (Neovim's `%{}` versus `%{%…%}`). That distinction is
//! exactly where ANSI injection lives — a branch name or model-derived string
//! reaching a status bar could carry cursor-movement or OSC sequences. Named
//! items delete the hazard class: text is always text, styling is always
//! structural, and nothing an expression returns is ever re-parsed as markup.
//!
//! ```lua
//! local sl = crucible.statusline
//! sl.setup{
//!   main = {
//!     anchor = "footer.below_input",
//!     items  = { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
//!                sl.align,
//!                sl.any(sl.notification, sl.context) },
//!   },
//! }
//! ```
//!
//! Evaluation splits by who owns the fact. Built-ins are TUI-local and cost no
//! RPC — the TUI already knows the mode, the model, the context usage. Only
//! [`StatusItem::Expr`] needs the daemon, and it is push-cached rather than
//! pulled per frame.

use serde_json::{json, Map, Value as Json};

/// Where a bar attaches. A **closed** set: the widget tree stays private, so it
/// can be restructured without breaking configs. Opening this later is additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Top,
    Bottom,
    FooterAboveInput,
    FooterBelowInput,
}

impl Anchor {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "top" => Some(Anchor::Top),
            "bottom" => Some(Anchor::Bottom),
            "footer.above_input" => Some(Anchor::FooterAboveInput),
            "footer.below_input" => Some(Anchor::FooterBelowInput),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Anchor::Top => "top",
            Anchor::Bottom => "bottom",
            Anchor::FooterAboveInput => "footer.above_input",
            Anchor::FooterBelowInput => "footer.below_input",
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

/// A named bar: where it goes and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarDef {
    pub anchor: Anchor,
    /// Stacking position within the anchor, lower first. Matches the `priority`
    /// convention plugin hooks already use.
    ///
    /// An anchor holds any number of rows, so something has to order them. Name
    /// order would work only by accident — two bars called `context` and `main`
    /// would stack alphabetically, and the author's only lever would be renaming
    /// them. Declaration order is not available either: `setup{}` is a Lua table
    /// with string keys, and those iterate in an unspecified order.
    pub order: i32,
    pub items: Vec<StatusItem>,
}

/// Where a bar sits when it does not say. Mid-range so bars can be placed on
/// either side of the default without renumbering.
pub const DEFAULT_ORDER: i32 = 100;

/// Every bar, by name.
pub type StatusBars = std::collections::BTreeMap<String, StatusBarDef>;

/// The bars at one anchor, top to bottom.
///
/// Ties break on name so the result is stable — equal `order` must not render
/// differently between runs.
pub fn bars_at(bars: &StatusBars, anchor: Anchor) -> Vec<(&str, &StatusBarDef)> {
    let mut found: Vec<(&str, &StatusBarDef)> = bars
        .iter()
        .filter(|(_, bar)| bar.anchor == anchor)
        .map(|(name, bar)| (name.as_str(), bar))
        .collect();
    found.sort_by(|(a_name, a), (b_name, b)| a.order.cmp(&b.order).then(a_name.cmp(b_name)));
    found
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
        "text" => StatusItem::Text(v.get("v")?.as_str()?.to_string()),
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

pub fn bars_to_wire(bars: &StatusBars) -> Json {
    let mut out = Map::new();
    for (name, bar) in bars {
        out.insert(
            name.clone(),
            json!({
                "anchor": bar.anchor.name(),
                "order": bar.order,
                "items": bar.items.iter().map(item_to_wire).collect::<Vec<_>>(),
            }),
        );
    }
    Json::Object(out)
}

pub fn bars_from_wire(v: &Json) -> StatusBars {
    let mut bars = StatusBars::new();
    let Some(obj) = v.as_object() else {
        return bars;
    };

    for (name, def) in obj {
        let Some(anchor) = def
            .get("anchor")
            .and_then(Json::as_str)
            .and_then(Anchor::from_name)
        else {
            tracing::warn!("statusline bar '{name}' has an unknown anchor; skipping it");
            continue;
        };
        let items = def
            .get("items")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(item_from_wire).collect())
            .unwrap_or_default();
        let order = def
            .get("order")
            .and_then(Json::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(DEFAULT_ORDER);
        bars.insert(
            name.clone(),
            StatusBarDef {
                anchor,
                order,
                items,
            },
        );
    }
    bars
}

/// The bar Crucible ships with, expressed in the new vocabulary.
///
/// Equivalent to the old `left/center/right` default: mode and model on the
/// left, then a notification that falls back to context usage on the right.
/// That fallback used to be a bespoke `Notification { fallback }` variant; it is
/// now just [`StatusItem::Any`].
pub fn builtin_default() -> StatusBars {
    let mut bars = StatusBars::new();
    bars.insert(
        "main".to_string(),
        StatusBarDef {
            anchor: Anchor::FooterBelowInput,
            order: DEFAULT_ORDER,
            items: vec![
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
            ],
        },
    );
    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(anchor: Anchor, order: i32) -> StatusBarDef {
        StatusBarDef {
            anchor,
            order,
            items: vec![StatusItem::Mode],
        }
    }

    /// An anchor is a slot, not a hook — several bars may share one.
    #[test]
    fn an_anchor_holds_several_bars_ordered_by_order() {
        let mut bars = StatusBars::new();
        bars.insert("main".into(), bar(Anchor::FooterBelowInput, 10));
        bars.insert("ctx".into(), bar(Anchor::FooterBelowInput, 20));

        let names: Vec<_> = bars_at(&bars, Anchor::FooterBelowInput)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, ["main", "ctx"], "order must beat alphabetical");
    }

    /// The bug this replaces: with no explicit order, `ctx` sorted above `main`
    /// purely because "c" < "m", and renaming a bar was the only lever.
    #[test]
    fn order_overrides_the_accidental_alphabetical_stacking() {
        let mut alphabetical = StatusBars::new();
        alphabetical.insert("ctx".into(), bar(Anchor::FooterBelowInput, DEFAULT_ORDER));
        alphabetical.insert("main".into(), bar(Anchor::FooterBelowInput, DEFAULT_ORDER));
        let tied: Vec<_> = bars_at(&alphabetical, Anchor::FooterBelowInput)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(tied, ["ctx", "main"], "equal order falls back to name");

        let mut ordered = StatusBars::new();
        ordered.insert("ctx".into(), bar(Anchor::FooterBelowInput, 20));
        ordered.insert("main".into(), bar(Anchor::FooterBelowInput, 10));
        let placed: Vec<_> = bars_at(&ordered, Anchor::FooterBelowInput)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(placed, ["main", "ctx"]);
    }

    #[test]
    fn bars_at_returns_only_that_anchor() {
        let mut bars = StatusBars::new();
        bars.insert("top".into(), bar(Anchor::Top, DEFAULT_ORDER));
        bars.insert(
            "footer".into(),
            bar(Anchor::FooterBelowInput, DEFAULT_ORDER),
        );

        assert_eq!(bars_at(&bars, Anchor::Top).len(), 1);
        assert_eq!(bars_at(&bars, Anchor::Bottom).len(), 0);
        assert_eq!(bars_at(&bars, Anchor::FooterAboveInput).len(), 0);
    }

    #[test]
    fn order_survives_the_wire_and_defaults_when_absent() {
        let mut bars = StatusBars::new();
        bars.insert("a".into(), bar(Anchor::Top, 42));
        assert_eq!(bars_from_wire(&bars_to_wire(&bars))["a"].order, 42);

        // A payload from a client that predates `order` must still place.
        let legacy = json!({ "a": { "anchor": "top", "items": [] } });
        assert_eq!(bars_from_wire(&legacy)["a"].order, DEFAULT_ORDER);
    }

    #[test]
    fn anchor_names_round_trip() {
        for a in [
            Anchor::Top,
            Anchor::Bottom,
            Anchor::FooterAboveInput,
            Anchor::FooterBelowInput,
        ] {
            assert_eq!(Anchor::from_name(a.name()), Some(a));
        }
    }

    #[test]
    fn an_unknown_anchor_is_rejected() {
        assert_eq!(Anchor::from_name("somewhere.unexpected"), None);
    }

    #[test]
    fn the_builtin_default_survives_a_wire_round_trip() {
        let bars = builtin_default();
        assert_eq!(bars_from_wire(&bars_to_wire(&bars)), bars);
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
            "main": {
                "anchor": "footer.below_input",
                "items": [ { "t": "mode" }, { "t": "hologram" }, { "t": "context" } ],
            }
        });

        let bars = bars_from_wire(&wire);
        let items = &bars.get("main").expect("bar survives").items;
        assert_eq!(items, &vec![StatusItem::Mode, StatusItem::Context]);
    }

    /// A bar anchored somewhere this client does not know is skipped whole —
    /// rendering it in the wrong place would be worse than not rendering it.
    #[test]
    fn a_bar_with_an_unknown_anchor_is_skipped() {
        let wire = json!({
            "ghost": { "anchor": "nowhere", "items": [ { "t": "mode" } ] },
            "main":  { "anchor": "top", "items": [ { "t": "mode" } ] },
        });

        let bars = bars_from_wire(&wire);
        assert!(!bars.contains_key("ghost"));
        assert!(bars.contains_key("main"));
    }

    #[test]
    fn a_malformed_payload_yields_no_bars_rather_than_panicking() {
        assert!(bars_from_wire(&json!("nonsense")).is_empty());
    }
}
