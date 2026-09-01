//! The closed set of settings-control kinds.
//!
//! `type` used to be a free string. `describe_node` passed it through verbatim
//! to every frontend, so a plugin declaring `type = "colour-wheel"` was
//! accepted, its value stored, and the web rendered it as a text box. A typo'd
//! `toggel` silently became a text field holding `"true"`, and `min`/`max` on a
//! node the renderer had decided was text were decoration nobody enforced.
//!
//! So the kind is a variant, and the variant states what values it admits.
//!
//! **Absence refuses the DECLARATION**, which is the one way this table differs
//! from `tools/surface.rs`, where `Unknown` refuses the CALL. A tool name can
//! arrive from an untrusted foreign server; a control kind cannot — every one
//! of them comes from this repo, so a name with no variant is a mistake in a
//! plugin's source and the plugin should be told at load rather than have its
//! settings quietly mis-rendered forever.
//!
//! The FRONTEND keeps its own fallback for an unrecognised kind, and that is
//! not dead code: a newer daemon talking to a stale cached bundle is real skew,
//! and "renders plainly" beats "renders nothing". This table is the daemon's
//! gate; the fallback is the renderer's defence in depth. Deleting either
//! because the other exists is the mistake.

// Both, and both are needed. With only the first, a variant handled by a
// `_ => Input` arm in a two-variant match passes review — the same hole
// `tools/surface.rs` records finding in `ToolSurface`.
#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use crate::signature::LuaType;

/// What a settings leaf is, and therefore what it accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum Control {
    /// A container. Carries `args`, holds no value of its own.
    Group,
    /// Free single-line text.
    Input,
    /// Free multi-line text.
    Text,
    /// A boolean.
    Toggle,
    /// A number, optionally bounded by `min` / `max` / `step`.
    Range,
    /// Exactly one of a declared set. `values` is required.
    Select,
    /// Zero or more of a declared set. `values` is required.
    MultiSelect,
    /// A filesystem path.
    Path,
    /// Write-only text. Declared now so the wire and the gate agree about it;
    /// the read refusal and its separate storage are a later stage.
    Secret,
    /// A button. Holds no value; carries `func`.
    Execute,
    /// Static prose in the pane. Holds no value.
    Description,
}

impl Control {
    /// Every variant, in declaration order.
    ///
    /// Hand-written because `strum` is a dev-dependency, and proved complete by
    /// `all_lists_every_control_once`, which walks `EnumIter` — the compiler's
    /// own account of the enum — rather than reading this array's length.
    pub const ALL: [Self; 11] = [
        Self::Group,
        Self::Input,
        Self::Text,
        Self::Toggle,
        Self::Range,
        Self::Select,
        Self::MultiSelect,
        Self::Path,
        Self::Secret,
        Self::Execute,
        Self::Description,
    ];

    /// The `type` string a plugin writes and a frontend reads.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Input => "input",
            Self::Text => "text",
            Self::Toggle => "toggle",
            Self::Range => "range",
            Self::Select => "select",
            Self::MultiSelect => "multiselect",
            Self::Path => "path",
            Self::Secret => "secret",
            Self::Execute => "execute",
            Self::Description => "description",
        }
    }

    /// The variant for a declared string, or `None` — which refuses the load.
    ///
    /// No `Default`, no `unwrap_or`: "unrecognised means input" is exactly the
    /// silent downgrade this table exists to stop.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == name)
    }

    /// Whether this control holds a value at all — i.e. whether `get` and `set`
    /// mean anything on it.
    pub const fn is_leaf(self) -> bool {
        match self {
            Self::Group | Self::Execute | Self::Description => false,
            Self::Input
            | Self::Text
            | Self::Toggle
            | Self::Range
            | Self::Select
            | Self::MultiSelect
            | Self::Path
            | Self::Secret => true,
        }
    }

    /// Whether a declared `values` list is required.
    pub const fn requires_values(self) -> bool {
        match self {
            Self::Select | Self::MultiSelect => true,
            Self::Group
            | Self::Input
            | Self::Text
            | Self::Toggle
            | Self::Range
            | Self::Path
            | Self::Secret
            | Self::Execute
            | Self::Description => false,
        }
    }

    /// Whether the value must never travel back out of the daemon.
    pub const fn is_write_only(self) -> bool {
        match self {
            Self::Secret => true,
            Self::Group
            | Self::Input
            | Self::Text
            | Self::Toggle
            | Self::Range
            | Self::Select
            | Self::MultiSelect
            | Self::Path
            | Self::Execute
            | Self::Description => false,
        }
    }

    /// The value domain, in the ONE type model this repo already has.
    ///
    /// `None` for the kinds that hold no value. This is where `signature.rs` is
    /// reused rather than duplicated: the returned type renders to JSON Schema
    /// for the write gate and to Luau for a declaration, and both come from the
    /// same parser the tool surface uses.
    ///
    /// A `Select`'s domain is deliberately NOT modelled here. Its admissible
    /// values are whatever its `values` function returns on this box at this
    /// moment, which no static type can state; the write gate checks membership
    /// against the evaluated list instead.
    pub fn value_type(self) -> Option<LuaType> {
        match self {
            Self::Group | Self::Execute | Self::Description => None,
            Self::Input | Self::Text | Self::Path | Self::Secret => Some(LuaType::String),
            Self::Toggle => Some(LuaType::Boolean),
            Self::Range => Some(LuaType::Number),
            Self::Select => Some(LuaType::Any),
            Self::MultiSelect => Some(LuaType::Array(Box::new(LuaType::Any))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use strum::IntoEnumIterator;

    #[test]
    fn all_lists_every_control_once() {
        // Derived from what the COMPILER knows about the enum, not from this
        // array's own length — an `ALL` that forgot a variant would otherwise
        // agree with itself.
        let from_iter: HashSet<Control> = Control::iter().collect();
        let from_all: HashSet<Control> = Control::ALL.into_iter().collect();
        assert_eq!(from_iter, from_all, "ALL and EnumIter disagree");
        assert_eq!(
            Control::ALL.len(),
            from_iter.len(),
            "ALL lists a variant twice"
        );
    }

    #[test]
    fn every_control_names_itself_uniquely_and_parses_back() {
        let mut seen = HashSet::new();
        for control in Control::iter() {
            let name = control.as_str();
            assert!(!name.is_empty(), "{control:?} has no name");
            assert!(seen.insert(name), "two controls answer to {name:?}");
            assert_eq!(
                Control::parse(name),
                Some(control),
                "{name:?} does not parse back"
            );
        }
    }

    #[test]
    fn an_unknown_name_has_no_control() {
        // The refusal this table exists for. `unwrap_or(Input)` here is the bug.
        assert_eq!(Control::parse("colour-wheel"), None);
        assert_eq!(Control::parse("toggel"), None);
        assert_eq!(Control::parse(""), None);
    }

    #[test]
    fn a_control_holds_a_value_exactly_when_it_has_a_value_type() {
        for control in Control::iter() {
            assert_eq!(
                control.is_leaf(),
                control.value_type().is_some(),
                "{control:?}: is_leaf and value_type disagree",
            );
        }
    }

    #[test]
    fn only_a_leaf_can_require_values_or_be_write_only() {
        for control in Control::iter() {
            if control.requires_values() || control.is_write_only() {
                assert!(control.is_leaf(), "{control:?} holds no value");
            }
        }
    }
}
