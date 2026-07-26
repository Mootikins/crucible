//! `crucible.ui` — per-surface geometry.
//!
//! Deliberately a **closed** set of surfaces, unlike the open highlight-group
//! namespace in [`crate::hl`]. Geometry has to be drawn by a renderer that knows
//! each surface, so an open namespace would only let a theme name things nothing
//! can draw. Colour is open because plugins genuinely need their own; geometry
//! is closed because the renderer is the limit.
//!
//! ```lua
//! crucible.ui.setup{
//!   popup  = { border = "rounded", padding = 1, max_visible = 10 },
//!   modal  = { border = { "╔","═","╗","║","╝","═","╚","║" }, padding = 1 },
//!   drawer = { border = { "", "▀", "", "", "", "▄", "", "" } },
//!   prompt = {
//!     normal  = { glyph = "❯ " },
//!     command = { glyph = ": " },
//!     shell   = { glyph = "! " },
//!   },
//! }
//! ```
//!
//! Every field is optional and `None` means "the renderer keeps its built-in".
//! That is the difference between a surface a theme chose not to touch and one
//! it blanked.

use crucible_oil::style::{Border, BorderChars, Padding};
use mlua::{Lua, Table, Value};
use serde_json::{json, Map, Value as Json};

/// Geometry for one surface. All-optional by design: a theme overrides what it
/// cares about and inherits the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceStyle {
    pub border: Option<Border>,
    pub padding: Option<Padding>,
    pub max_height: Option<u16>,
    pub min_width: Option<u16>,
    /// Popup only: rows shown before scrolling.
    pub max_visible: Option<u16>,
}

/// Input-area prompt glyphs, per input mode. These are compiled-in today
/// (`InputStyle::prompt` returns `&'static str`), so this is the first thing
/// that makes them authorable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptStyle {
    pub normal: Option<String>,
    pub command: Option<String>,
    pub shell: Option<String>,
}

/// The closed surface set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiGeometry {
    pub popup: SurfaceStyle,
    pub modal: SurfaceStyle,
    pub drawer: SurfaceStyle,
    pub toast: SurfaceStyle,
    pub statusline: SurfaceStyle,
    pub prompt: PromptStyle,
}

// ─────────────────────────────────────────────────────────────────────────────
// Borders
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a border from a preset name.
fn border_from_name(s: &str) -> Option<Border> {
    match s.to_lowercase().as_str() {
        "none" | "hidden" => None,
        "single" | "sharp" => Some(Border::Single),
        "double" => Some(Border::Double),
        "rounded" => Some(Border::Rounded),
        "heavy" | "thick" => Some(Border::Heavy),
        other => {
            tracing::warn!("unknown border style '{other}'; keeping the built-in");
            None
        }
    }
}

/// One border list entry. An empty string means the edge is absent, matching
/// Neovim; anything longer than one char takes its first, since a border cell
/// holds exactly one.
fn border_cell(s: &str) -> Option<char> {
    s.chars().next()
}

// ─────────────────────────────────────────────────────────────────────────────
// Lua
// ─────────────────────────────────────────────────────────────────────────────

fn border_from_lua(v: &Value) -> Option<Border> {
    match v {
        Value::String(s) => border_from_name(&s.to_str().ok()?),
        Value::Table(t) => {
            let mut items: Vec<Option<char>> = Vec::new();
            for pair in t.clone().sequence_values::<String>() {
                items.push(border_cell(&pair.ok()?));
            }
            BorderChars::from_list(&items).map(Border::Custom)
        }
        _ => None,
    }
}

/// `padding = 1` | `{ x = 2, y = 1 }` | `{ top =, right =, bottom =, left = }`
fn padding_from_lua(v: &Value) -> Option<Padding> {
    match v {
        Value::Integer(n) => Some(Padding::all(u16::try_from(*n).ok()?)),
        Value::Table(t) => {
            let get = |k: &str| t.get::<u16>(k).ok();
            match (get("x"), get("y")) {
                (None, None) => Some(Padding {
                    top: get("top").unwrap_or(0),
                    right: get("right").unwrap_or(0),
                    bottom: get("bottom").unwrap_or(0),
                    left: get("left").unwrap_or(0),
                }),
                (x, y) => Some(Padding::xy(x.unwrap_or(0), y.unwrap_or(0))),
            }
        }
        _ => None,
    }
}

fn surface_from_lua(table: &Table) -> SurfaceStyle {
    SurfaceStyle {
        border: table
            .get::<Value>("border")
            .ok()
            .as_ref()
            .and_then(border_from_lua),
        padding: table
            .get::<Value>("padding")
            .ok()
            .as_ref()
            .and_then(padding_from_lua),
        max_height: table.get("max_height").ok(),
        min_width: table.get("min_width").ok(),
        max_visible: table.get("max_visible").ok(),
    }
}

fn prompt_from_lua(table: &Table) -> PromptStyle {
    let glyph = |mode: &str| {
        table
            .get::<Table>(mode)
            .ok()
            .and_then(|t| t.get::<String>("glyph").ok())
    };
    PromptStyle {
        normal: glyph("normal"),
        command: glyph("command"),
        shell: glyph("shell"),
    }
}

/// Parse a whole `crucible.ui.setup{...}` table.
pub fn geometry_from_lua(config: &Table) -> UiGeometry {
    let surface = |key: &str| {
        config
            .get::<Table>(key)
            .ok()
            .map(|t| surface_from_lua(&t))
            .unwrap_or_default()
    };
    UiGeometry {
        popup: surface("popup"),
        modal: surface("modal"),
        drawer: surface("drawer"),
        toast: surface("toast"),
        statusline: surface("statusline"),
        prompt: config
            .get::<Table>("prompt")
            .ok()
            .map(|t| prompt_from_lua(&t))
            .unwrap_or_default(),
    }
}

/// Register `crucible.ui` on an existing `crucible` table.
pub fn register_ui_namespace(lua: &Lua, crucible: &Table) -> Result<(), crate::error::LuaError> {
    let ui = lua.create_table()?;
    let setup_fn = lua.create_function(|_, config: Table| {
        crate::config::set_ui_geometry(geometry_from_lua(&config));
        Ok(())
    })?;
    ui.set("setup", setup_fn)?;
    crucible.set("ui", ui)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire
// ─────────────────────────────────────────────────────────────────────────────

fn border_to_wire(b: Border) -> Json {
    match b {
        Border::Single => json!("single"),
        Border::Double => json!("double"),
        Border::Rounded => json!("rounded"),
        Border::Heavy => json!("heavy"),
        // A custom set crosses as its eight cells; an absent edge is "".
        Border::Custom(c) => {
            let cell = |o: Option<char>| o.map(String::from).unwrap_or_default();
            json!([
                cell(c.top_left),
                cell(c.top),
                cell(c.top_right),
                cell(c.right),
                cell(c.bottom_right),
                cell(c.bottom),
                cell(c.bottom_left),
                cell(c.left),
            ])
        }
    }
}

fn border_from_wire(v: &Json) -> Option<Border> {
    match v {
        Json::String(s) => border_from_name(s),
        Json::Array(items) => {
            let cells: Vec<Option<char>> = items
                .iter()
                .map(|i| i.as_str().and_then(border_cell))
                .collect();
            BorderChars::from_list(&cells).map(Border::Custom)
        }
        _ => None,
    }
}

fn surface_to_wire(s: &SurfaceStyle) -> Json {
    let mut m = Map::new();
    if let Some(b) = s.border {
        m.insert("border".into(), border_to_wire(b));
    }
    if let Some(p) = s.padding {
        m.insert(
            "padding".into(),
            json!({ "top": p.top, "right": p.right, "bottom": p.bottom, "left": p.left }),
        );
    }
    for (key, val) in [
        ("max_height", s.max_height),
        ("min_width", s.min_width),
        ("max_visible", s.max_visible),
    ] {
        if let Some(n) = val {
            m.insert(key.into(), json!(n));
        }
    }
    Json::Object(m)
}

fn surface_from_wire(v: Option<&Json>) -> SurfaceStyle {
    let Some(m) = v.and_then(Json::as_object) else {
        return SurfaceStyle::default();
    };
    let num = |key: &str| {
        m.get(key)
            .and_then(Json::as_u64)
            .and_then(|n| u16::try_from(n).ok())
    };
    SurfaceStyle {
        border: m.get("border").and_then(border_from_wire),
        padding: m.get("padding").and_then(Json::as_object).map(|p| {
            let f = |k: &str| {
                p.get(k)
                    .and_then(Json::as_u64)
                    .and_then(|n| u16::try_from(n).ok())
                    .unwrap_or(0)
            };
            Padding {
                top: f("top"),
                right: f("right"),
                bottom: f("bottom"),
                left: f("left"),
            }
        }),
        max_height: num("max_height"),
        min_width: num("min_width"),
        max_visible: num("max_visible"),
    }
}

/// Serialize geometry for `ui.config`.
pub fn geometry_to_wire(g: &UiGeometry) -> Json {
    let mut prompt = Map::new();
    for (mode, glyph) in [
        ("normal", &g.prompt.normal),
        ("command", &g.prompt.command),
        ("shell", &g.prompt.shell),
    ] {
        if let Some(s) = glyph {
            prompt.insert(mode.into(), json!({ "glyph": s }));
        }
    }

    json!({
        "popup": surface_to_wire(&g.popup),
        "modal": surface_to_wire(&g.modal),
        "drawer": surface_to_wire(&g.drawer),
        "toast": surface_to_wire(&g.toast),
        "statusline": surface_to_wire(&g.statusline),
        "prompt": Json::Object(prompt),
    })
}

/// Parse geometry from `ui.config`. Unknown surfaces are ignored, so a newer
/// daemon describing a surface this client cannot draw is not fatal.
pub fn geometry_from_wire(v: &Json) -> UiGeometry {
    let Some(m) = v.as_object() else {
        return UiGeometry::default();
    };
    let glyph = |mode: &str| {
        m.get("prompt")?
            .get(mode)?
            .get("glyph")?
            .as_str()
            .map(std::string::ToString::to_string)
    };
    UiGeometry {
        popup: surface_from_wire(m.get("popup")),
        modal: surface_from_wire(m.get("modal")),
        drawer: surface_from_wire(m.get("drawer")),
        toast: surface_from_wire(m.get("toast")),
        statusline: surface_from_wire(m.get("statusline")),
        prompt: PromptStyle {
            normal: glyph("normal"),
            command: glyph("command"),
            shell: glyph("shell"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(src: &str) -> UiGeometry {
        let lua = Lua::new();
        let table: Table = lua.load(src).eval().unwrap();
        geometry_from_lua(&table)
    }

    #[test]
    fn a_preset_border_name_resolves() {
        let g = setup(r#"return { popup = { border = "rounded" } }"#);
        assert_eq!(g.popup.border, Some(Border::Rounded));
    }

    #[test]
    fn border_none_means_no_border_not_a_default_one() {
        let g = setup(r#"return { popup = { border = "none" } }"#);
        assert_eq!(g.popup.border, None);
    }

    #[test]
    fn an_eight_element_border_list_becomes_a_custom_border() {
        let g = setup(r#"return { modal = { border = {"╔","═","╗","║","╝","═","╚","║"} } }"#);
        let Some(Border::Custom(c)) = g.modal.border else {
            panic!("expected a custom border, got {:?}", g.modal.border);
        };
        assert_eq!(c.top_left, Some('╔'));
        assert_eq!(c.left, Some('║'));
    }

    /// The drawer's real shape — rules above and below, no sides.
    #[test]
    fn empty_strings_in_a_border_list_mark_absent_edges() {
        let g = setup(r#"return { drawer = { border = {"", "▀", "", "", "", "▄", "", ""} } }"#);
        let Some(Border::Custom(c)) = g.drawer.border else {
            panic!("expected a custom border");
        };
        assert_eq!(c.top, Some('▀'));
        assert_eq!(c.bottom, Some('▄'));
        assert!(!c.has_left());
        assert!(!c.has_right());
    }

    #[test]
    fn padding_accepts_a_scalar() {
        let g = setup(r#"return { popup = { padding = 2 } }"#);
        assert_eq!(g.popup.padding, Some(Padding::all(2)));
    }

    #[test]
    fn padding_accepts_x_and_y() {
        let g = setup(r#"return { popup = { padding = { x = 3, y = 1 } } }"#);
        assert_eq!(g.popup.padding, Some(Padding::xy(3, 1)));
    }

    #[test]
    fn padding_accepts_named_edges() {
        let g = setup(r#"return { popup = { padding = { top = 1, left = 4 } } }"#);
        assert_eq!(
            g.popup.padding,
            Some(Padding {
                top: 1,
                right: 0,
                bottom: 0,
                left: 4
            })
        );
    }

    #[test]
    fn prompt_glyphs_are_read_per_mode() {
        let g = setup(
            r#"return { prompt = {
                normal = { glyph = "> " },
                command = { glyph = ": " },
                shell = { glyph = "! " },
            } }"#,
        );
        assert_eq!(g.prompt.normal.as_deref(), Some("> "));
        assert_eq!(g.prompt.command.as_deref(), Some(": "));
        assert_eq!(g.prompt.shell.as_deref(), Some("! "));
    }

    /// An untouched surface must stay untouched, not be blanked.
    #[test]
    fn surfaces_absent_from_the_config_keep_their_defaults() {
        let g = setup(r#"return { popup = { padding = 1 } }"#);
        assert_eq!(g.modal, SurfaceStyle::default());
        assert_eq!(g.drawer, SurfaceStyle::default());
    }

    #[test]
    fn geometry_survives_a_wire_round_trip() {
        let g = setup(
            r#"return {
                popup = { border = "rounded", padding = 1, max_visible = 10 },
                drawer = { border = {"", "▀", "", "", "", "▄", "", ""} },
                prompt = { normal = { glyph = "❯ " } },
            }"#,
        );
        assert_eq!(geometry_from_wire(&geometry_to_wire(&g)), g);
    }

    #[test]
    fn a_malformed_payload_yields_defaults_not_a_panic() {
        assert_eq!(
            geometry_from_wire(&json!("nonsense")),
            UiGeometry::default()
        );
    }

    #[test]
    fn an_unknown_border_name_keeps_the_built_in() {
        let g = setup(r#"return { popup = { border = "squiggly" } }"#);
        assert_eq!(g.popup.border, None);
    }
}
