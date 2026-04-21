//! Permission configuration types and rule parsing

mod engine;
mod hardcoded;
mod matcher;
mod normalize;
mod parse;
mod types;
mod write;

#[cfg(test)]
mod tests;

pub use engine::PermissionEngine;
pub use hardcoded::is_hardcoded_denied;
pub use matcher::{CompiledPermissions, PermissionMatcher};
pub use normalize::{normalize_path_for_matching, split_chained_commands};
pub use parse::parse_rule;
pub use types::{
    ParsedRule, PermissionConfig, PermissionDecision, PermissionMode, PermissionScope,
};
pub use write::write_permission_rule;
