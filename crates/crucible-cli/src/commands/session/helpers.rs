use anyhow::{anyhow, Result};

pub(super) fn resolve_permission_mode(flag: Option<&str>) -> Result<Option<String>> {
    let raw = flag
        .map(|s| s.to_string())
        .or_else(|| std::env::var("CRUCIBLE_PERMISSIONS").ok());

    match raw {
        Some(val) => {
            let _validated: crucible_config::components::permissions::PermissionMode =
                val.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
            Ok(Some(val))
        }
        None => Ok(None),
    }
}

pub fn resolve_session_id(explicit: Option<String>) -> Result<String> {
    explicit
        .or_else(|| std::env::var("CRU_SESSION").ok())
        .ok_or_else(|| anyhow!("No session specified. Pass session ID or set CRU_SESSION env var."))
}

pub(super) fn warn_deprecated(old: &str, new: &str) {
    eprintln!("warning: '{}' is deprecated, use '{}' instead", old, new);
}

/// The set of session-type strings clap accepts on the command line. Kept as
/// a single source of truth so the clap `value_parser` list and the
/// canonicalization step below can't drift.
pub const ACCEPTED_SESSION_TYPES: &[&str] = &["chat", "agent", "workflow", "mcp"];

/// clap `value_parser` used for `cru session create -t` and `cru session
/// list -t`. Validates the input against [`ACCEPTED_SESSION_TYPES`] and
/// rewrites legacy values to their canonical form, emitting a deprecation
/// warning on stderr when we do so. The post-parse `session_type: String` is
/// therefore always one of the canonical variants.
pub fn parse_session_type_arg(input: &str) -> Result<String, String> {
    if !ACCEPTED_SESSION_TYPES.contains(&input) {
        return Err(format!(
            "invalid session type '{input}' (expected one of: {})",
            ACCEPTED_SESSION_TYPES.join(", ")
        ));
    }
    match input {
        "mcp" => {
            eprintln!(
                "warning: session type 'mcp' is deprecated and mapped to 'chat'; pass -t chat explicitly"
            );
            Ok("chat".to_string())
        }
        other => Ok(other.to_string()),
    }
}

pub(super) fn resolve_send_inputs(
    session_id_pos: Option<String>,
    message: Option<String>,
    session_id_flag: Option<String>,
) -> (Option<String>, Option<String>, bool) {
    if let Some(flag_id) = session_id_flag {
        return (Some(flag_id), session_id_pos, true);
    }

    if session_id_pos.is_some() && message.is_some() {
        return (session_id_pos, message, false);
    }

    if session_id_pos.is_some() && std::env::var("CRU_SESSION").is_ok() {
        return (None, session_id_pos, false);
    }

    (session_id_pos, message, false)
}

pub(super) fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    }
}
