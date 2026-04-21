use super::*;

mod engine;
mod hardcoded;
mod matcher;
mod normalize;
mod types;
mod write;

fn config_with_rules(
    default: PermissionMode,
    allow: &[&str],
    deny: &[&str],
    ask: &[&str],
) -> PermissionConfig {
    PermissionConfig {
        default,
        allow: allow.iter().map(|s| (*s).to_string()).collect(),
        deny: deny.iter().map(|s| (*s).to_string()).collect(),
        ask: ask.iter().map(|s| (*s).to_string()).collect(),
    }
}
