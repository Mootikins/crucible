//! The wire shape of a plugin surface: a panel a client draws.
//!
//! `cru.surface.declare{...}` in `crucible-lua` gives a plugin a named panel.
//! The daemon holds the live registry; these four types are what crosses the
//! RPC boundary to the TUI and the web. One core type serves both readers, so
//! a field added here reaches every client with no second copy to update.

use serde::{Deserialize, Serialize};

/// What a client draws.
///
/// One variant, because one renderer exists. `Tree`, `Table` and `KeyValue`
/// arrive **with** their renderers, not ahead of them: the exhaustive match
/// on this enum is what forces that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    /// Rows in order, one line each.
    List,
}

impl Shape {
    /// The name a plugin writes, and the name on the wire.
    ///
    /// **No wildcard arm, ever.** A new shape must fail to compile until
    /// someone names it here and in every renderer.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
        }
    }

    /// The shape for a declared name, or `None` when no client draws it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

/// A row's status, stated as a fact rather than as a glyph.
///
/// The plugin says what is true. The TUI may draw `●` and the web a coloured
/// dot; each client picks its own mark. A plugin that shipped its own glyph
/// would bind one client's medium into a contract both must honour, which is
/// the mistake this enum exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    /// Work is underway.
    Busy,
    /// Waiting on a person.
    Blocked,
    /// Finished, nothing wrong.
    Ok,
    /// Finished, something is wrong.
    Failed,
}

impl Mark {
    /// **No wildcard arm, ever** — same reason as [`Shape::as_str`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Blocked => "blocked",
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }

    /// The mark for a declared name, or `None` when no client draws it.
    ///
    /// A plugin that names an unknown mark keeps its row. The row draws with
    /// no status, rather than being refused for a spelling this build does
    /// not yet know.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "busy" => Some(Self::Busy),
            "blocked" => Some(Self::Blocked),
            "ok" => Some(Self::Ok),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One row of a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SurfaceRow {
    /// Stable identity, chosen by the plugin. What an action names later, and
    /// what a client keys a selection on across a re-push.
    pub id: String,
    /// The row's own text.
    pub text: String,
    /// Secondary text, or `null` when the row has none. Always written, so
    /// `null` means "no detail", never "unknown".
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub detail: Option<String>,
    /// Status, or `null` when the row has none. Always written, so `null`
    /// means "no status", never "unknown".
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub mark: Option<Mark>,
}

/// One declared surface, as `surface.list` and `surface.get` report it.
///
/// Rows travel with the surface. A surface is a panel a person reads, not a
/// feed a client polls piece by piece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Surface {
    /// The plugin that declared it, so a stale surface can be attributed.
    pub plugin: String,
    /// The plugin's own name for it. Stable across a reload.
    pub name: String,
    pub title: String,
    pub shape: Shape,
    /// The session this surface is about, or `null` when it is about the
    /// plugin. Always written, so `null` means "about the plugin", never
    /// "unknown".
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub session: Option<String>,
    pub rows: Vec<SurfaceRow>,
    /// Bumped on every row change, so a client redraws on a change it sees
    /// rather than on a timer.
    pub version: u64,
}
