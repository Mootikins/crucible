//! The closed set of Bases operations.
//!
//! The daemon RPC dispatch, the daemon Bases executor and the `cru.kiln`
//! Lua binding all use this one enum.

use strum::IntoEnumIterator;

/// Every Bases operation. The daemon's RPC methods and the `cru.kiln`
/// functions both dispatch through this one set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub enum BaseOperation {
    List,
    Views,
    Query,
    SetProperty,
    CreateEntry,
    ReorderGroups,
    EnsureBase,
    PendingWrites,
}

impl BaseOperation {
    /// The `cru.kiln` function name. The daemon also gives this name to an
    /// isolation refusal. `list` and `views` carry a `base` qualifier because
    /// `cru.kiln.list` already lists notes.
    pub const fn name(self) -> &'static str {
        match self {
            Self::List => "list_bases",
            Self::Views => "base_views",
            Self::Query => "query",
            Self::SetProperty => "set_property",
            Self::CreateEntry => "create_entry",
            Self::ReorderGroups => "reorder_groups",
            Self::EnsureBase => "ensure_base",
            Self::PendingWrites => "pending_writes",
        }
    }

    /// Whether the operation changes files, and so needs a session.
    pub const fn writes(self) -> bool {
        match self {
            Self::List | Self::Views | Self::Query | Self::PendingWrites => false,
            Self::SetProperty | Self::CreateEntry | Self::ReorderGroups | Self::EnsureBase => true,
        }
    }

    /// The `cru.kiln` names of all operations.
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::iter().map(Self::name)
    }
}
