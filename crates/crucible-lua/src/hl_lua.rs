//! `cru.hl` — the Lua surface for highlight groups, and their wire form.
//!
//! ```lua
//! cru.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })
//! cru.hl.link("PopupSelected", "Visual")
//! ```
//!
//! Split from [`crate::hl`] so the resolution logic stays free of mlua and can
//! be used by the TUI, which has no Lua VM.

use crate::error::LuaError;
use crate::hl::{HlColor, HlGroup, HlRegistry};
use crate::theme::adaptive_from_lua_structured;
use crate::theme_wire::{adaptive_pair_from_wire, color_to_name};
use mlua::{Lua, Table, Value};
use serde_json::{json, Map, Value as Json};

/// Read a colour field written in any of the authoring forms.
fn color_from_lua(table: &Table, key: &str) -> Option<HlColor> {
    match table.get::<Value>(key).ok()? {
        Value::String(s) => Some(HlColor::parse(&s.to_str().ok()?)),
        other => adaptive_from_lua_structured(&other).map(HlColor::Adaptive),
    }
}

fn group_from_lua(table: &Table) -> HlGroup {
    HlGroup {
        fg: color_from_lua(table, "fg"),
        bg: color_from_lua(table, "bg"),
        bold: table.get("bold").unwrap_or(false),
        dim: table.get("dim").unwrap_or(false),
        italic: table.get("italic").unwrap_or(false),
        underline: table.get("underline").unwrap_or(false),
        link: table.get::<String>("link").ok(),
    }
}

/// Register `cru.hl` on an existing `cru` table.
pub fn register_hl_namespace(lua: &Lua, cru: &Table) -> Result<(), LuaError> {
    let mut ns = crate::host_registry::Ns::new(lua, "cru.hl")?;

    ns.func(
        "set",
        "(group: string, spec: { fg: string?, bg: string?, bold: boolean?, \
         dim: boolean?, italic: boolean?, underline: boolean?, link: string? }) -> ()",
        |_, (name, spec): (String, Table)| {
            crate::config::set_hl_group(name, group_from_lua(&spec));
            Ok(())
        },
    )?;
    ns.doc(
        "set",
        "Define one highlight group. A colour may name a palette entry, which \
         resolves against whatever `cru.colorscheme.setup` is in force rather \
         than at the moment of this call.",
    );

    ns.func(
        "link",
        "(from: string, to: string) -> ()",
        |_, (from, to): (String, String)| {
            crate::config::set_hl_group(
                from,
                HlGroup {
                    link: Some(to),
                    ..Default::default()
                },
            );
            Ok(())
        },
    )?;
    ns.doc(
        "link",
        "Point one group at another. The same as `cru.hl.set(from, { link = to })`. \
         A link to a group that does not exist is not an error; it renders \
         unstyled.",
    );

    cru.set("hl", ns.table().clone())?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire form
// ─────────────────────────────────────────────────────────────────────────────

fn color_to_wire(c: &HlColor) -> Json {
    match c {
        // Palette references cross the wire as names, unresolved — the client
        // resolves them against the theme it received in the same payload.
        HlColor::Palette(name) => Json::String(name.clone()),
        HlColor::Adaptive(a) if a.dark == a.light => Json::String(color_to_name(a.dark)),
        HlColor::Adaptive(a) => {
            json!({ "dark": color_to_name(a.dark), "light": color_to_name(a.light) })
        }
    }
}

fn color_from_wire(v: &Json) -> Option<HlColor> {
    match v {
        Json::String(s) => Some(HlColor::parse(s)),
        Json::Object(o) => adaptive_pair_from_wire(o).map(HlColor::Adaptive),
        _ => None,
    }
}

/// Serialize the highlight table for `ui.config`.
pub fn registry_to_wire(registry: &HlRegistry) -> Json {
    let mut out = Map::new();
    for (name, g) in registry {
        let mut m = Map::new();
        if let Some(ref fg) = g.fg {
            m.insert("fg".into(), color_to_wire(fg));
        }
        if let Some(ref bg) = g.bg {
            m.insert("bg".into(), color_to_wire(bg));
        }
        for (key, set) in [
            ("bold", g.bold),
            ("dim", g.dim),
            ("italic", g.italic),
            ("underline", g.underline),
        ] {
            if set {
                m.insert(key.into(), Json::Bool(true));
            }
        }
        if let Some(ref link) = g.link {
            m.insert("link".into(), Json::String(link.clone()));
        }
        out.insert(name.clone(), Json::Object(m));
    }
    Json::Object(out)
}

/// Parse the highlight table from `ui.config`. Unparseable entries are skipped
/// rather than failing the payload — one bad group must not cost the rest.
pub fn registry_from_wire(v: &Json) -> HlRegistry {
    let mut registry = HlRegistry::new();
    let Some(obj) = v.as_object() else {
        return registry;
    };

    for (name, spec) in obj {
        let Some(m) = spec.as_object() else { continue };
        let flag = |key: &str| m.get(key).and_then(Json::as_bool).unwrap_or(false);
        registry.insert(
            name.clone(),
            HlGroup {
                fg: m.get("fg").and_then(color_from_wire),
                bg: m.get("bg").and_then(color_from_wire),
                bold: flag("bold"),
                dim: flag("dim"),
                italic: flag("italic"),
                underline: flag("underline"),
                link: m
                    .get("link")
                    .and_then(Json::as_str)
                    .map(std::string::ToString::to_string),
            },
        );
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hl::resolve;
    use crate::theme::ThemeConfig;
    use crucible_oil::style::{AdaptiveColor, Color};

    fn lua_with_hl() -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru.clone()).unwrap();
        register_hl_namespace(&lua, &cru).unwrap();
        lua
    }

    #[test]
    fn set_defines_a_group_reachable_from_the_store() {
        let lua = lua_with_hl();
        lua.load(r#"cru.hl.set("Mode", { fg = "black", bg = "mode_normal", bold = true })"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        let group = registry.get("Mode").expect("group was stored");
        assert_eq!(group.bg, Some(HlColor::Palette("mode_normal".into())));
        assert!(group.bold);
    }

    #[test]
    fn link_is_sugar_for_a_link_only_group() {
        let lua = lua_with_hl();
        lua.load(r#"cru.hl.link("PopupSel", "Visual")"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        assert_eq!(
            registry.get("PopupSel").unwrap().link,
            Some("Visual".to_string())
        );
    }

    #[test]
    fn adaptive_colours_survive_the_lua_table_form() {
        let lua = lua_with_hl();
        lua.load(r#"cru.hl.set("T", { fg = { dark = "white", light = "black" } })"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        assert_eq!(
            registry.get("T").unwrap().fg,
            Some(HlColor::Adaptive(AdaptiveColor {
                dark: Color::White,
                light: Color::Black
            }))
        );
    }

    /// The highlight surface reads the same integer and `{ idx = n }` forms
    /// as the theme colours, through the same helper.
    #[test]
    fn palette_index_forms_match_the_theme_parser() {
        let lua = lua_with_hl();
        lua.load(r#"cru.hl.set("I", { fg = 4, bg = { idx = 12 } })"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        let group = registry.get("I").unwrap();
        assert_eq!(
            group.fg,
            Some(HlColor::Adaptive(AdaptiveColor::from_single(
                Color::Indexed(4)
            )))
        );
        assert_eq!(
            group.bg,
            Some(HlColor::Adaptive(AdaptiveColor::from_single(
                Color::Indexed(12)
            )))
        );
    }

    #[test]
    fn an_adaptive_pair_crosses_the_wire_as_an_object() {
        let wire = json!({ "T": { "fg": { "dark": "white", "light": "black" } } });
        let registry = registry_from_wire(&wire);
        assert_eq!(
            registry.get("T").unwrap().fg,
            Some(HlColor::Adaptive(AdaptiveColor {
                dark: Color::White,
                light: Color::Black
            }))
        );
    }

    #[test]
    fn a_registry_survives_a_wire_round_trip() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                fg: Some(HlColor::parse("black")),
                bg: Some(HlColor::parse("mode_normal")),
                bold: true,
                ..Default::default()
            },
        );
        registry.insert(
            "Sel".into(),
            HlGroup {
                link: Some("Mode".into()),
                ..Default::default()
            },
        );

        assert_eq!(registry_from_wire(&registry_to_wire(&registry)), registry);
    }

    /// Palette references must stay names across the wire, or the client cannot
    /// re-resolve them when the palette changes.
    #[test]
    fn palette_references_cross_the_wire_unresolved() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                bg: Some(HlColor::parse("mode_normal")),
                ..Default::default()
            },
        );

        assert_eq!(registry_to_wire(&registry)["Mode"]["bg"], "mode_normal");
    }

    /// End to end through the wire: a group authored against the palette
    /// resolves to whatever the receiving theme says that palette entry is.
    #[test]
    fn a_group_resolves_after_crossing_the_wire() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                bg: Some(HlColor::parse("mode_normal")),
                ..Default::default()
            },
        );

        let restored = registry_from_wire(&registry_to_wire(&registry));
        let theme = ThemeConfig::default_dark();

        assert_eq!(
            resolve("Mode", &restored, &theme).unwrap().bg,
            Some(theme.resolve_color(theme.colors.mode_normal))
        );
    }

    #[test]
    fn a_malformed_group_is_skipped_not_fatal() {
        let wire = json!({ "Good": { "fg": "cyan" }, "Bad": "not a table" });
        let registry = registry_from_wire(&wire);

        assert!(registry.contains_key("Good"));
        assert!(!registry.contains_key("Bad"));
    }
}
