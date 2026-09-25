use crate::status_color::StatusColorGroup;
use serde::{Deserialize, Serialize};

/// The client-facing status item.
///
/// The one wire shape of a status item: the `session.status` reply and the
/// `status_items_changed` event both carry a list of it, and the web route
/// declares it in the OpenAPI document. The daemon keeps the authored list;
/// clients only decide where and how it fits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StatusDisplayItem {
    /// Stable within the session's list.
    pub id: String,
    /// The text to draw. The producer writes it; no client rewrites it.
    pub text: String,
    /// Smaller values appear first. Pinning is separate from the order.
    pub priority: u8,
    /// Named color, which each client resolves through its own theme.
    pub color_group: StatusColorGroup,
    /// The engine method that the item opens, if it opens one.
    pub action: Option<String>,
    /// A pinned item stays visible when the other items overflow.
    pub pinned: bool,
    /// The plugin that the item is about.
    pub plugin: String,
    /// Who made the item. The TUI places each kind with its own statusline
    /// item; the web draws every kind in one slot.
    #[serde(default)]
    pub kind: StatusItemKind,
    /// How far the work that the item describes is. `None` is a state
    /// ("sandboxed: alpine"), not a bar that stalls at zero.
    #[serde(default)]
    pub progress: Option<StatusProgress>,
}

/// The source of a status item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StatusItemKind {
    /// A plugin published it through `cru.statusline.publish` or
    /// `cru.plugin.set_status`. `sl.items` draws it.
    #[default]
    Published,
    /// The engine made it from the session's plugin approval knob and the
    /// plugin turn that runs now. `sl.plugin_turns` draws it.
    PluginTurns,
}

/// How far along a status item's work is, when it is work rather than a
/// state.
///
/// Modelled on LSP `$/progress`: the producer reports and the client decides
/// how to draw it. An image pull knows its fraction; an image build does
/// not, and a fake fraction for the second is worse than saying so. On the
/// wire it is a number or the string `"indeterminate"`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum StatusProgress {
    /// Fraction complete, clamped to 0.0..=1.0.
    Fraction(f64),
    /// Work is underway with no meaningful fraction: draw a spinner.
    Unknown(IndeterminateProgress),
}

/// The one word of [`StatusProgress::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IndeterminateProgress {
    Indeterminate,
}

impl StatusProgress {
    /// Work with no fraction.
    pub const INDETERMINATE: Self = Self::Unknown(IndeterminateProgress::Indeterminate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_a_number_or_the_word_on_the_wire() {
        assert_eq!(
            serde_json::to_value(StatusProgress::Fraction(0.25)).unwrap(),
            serde_json::json!(0.25)
        );
        assert_eq!(
            serde_json::to_value(StatusProgress::INDETERMINATE).unwrap(),
            serde_json::json!("indeterminate")
        );
        for value in [serde_json::json!(0.25), serde_json::json!("indeterminate")] {
            let back: StatusProgress = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(back).unwrap(), value);
        }
    }
}
