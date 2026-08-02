//! Default-deny for a session a plugin claimed isolation over.
//!
//! Its own module for the same reason as [`super::gate_decision`]: this is a
//! trust boundary, not a step in tool dispatch. It runs after every
//! interception seam has declined the call, so answering "allowed" here means
//! the call is about to execute wherever the daemon runs — outside whatever
//! sandbox the user believes they turned on.

use crucible_core::traits::tools::ToolSurface;

/// The refusal to show the model, or `None` when the call may proceed.
///
/// The message names the plugin because "refused" with no attribution is
/// unactionable: the user configured the sandbox somewhere, and the plugin
/// name is the thread back to it.
pub(crate) fn isolation_refusal(
    isolation: Option<&crucible_lua::IsolationRegistry>,
    session_id: &str,
    tool_name: &str,
    surface: ToolSurface,
) -> Option<String> {
    let isolation = isolation?;
    if isolation.host_execution_allowed(session_id, tool_name, surface) {
        return None;
    }
    let plugin = isolation
        .get(session_id)
        .map(|c| c.plugin)
        .unwrap_or_else(|| "a plugin".into());
    Some(format!(
        "Tool '{tool_name}' refused: this session is isolated by '{plugin}', which did not \
         handle it. Running it here would execute on the host, outside the sandbox. Add it \
         to that plugin's `exempt` list if host execution is intended."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::{IsolationClaim, IsolationRegistry};

    fn claimed() -> IsolationRegistry {
        let reg = IsolationRegistry::new();
        reg.claim(
            "s1",
            IsolationClaim {
                plugin: "oci".to_string(),
                exempt: Default::default(),
                exec_prefix: Vec::new(),
            },
        );
        reg
    }

    #[test]
    fn no_registry_never_refuses() {
        assert!(isolation_refusal(None, "s1", "bash", ToolSurface::Host).is_none());
    }

    #[test]
    fn a_host_tool_under_a_claim_is_refused_naming_the_plugin() {
        let refusal = isolation_refusal(Some(&claimed()), "s1", "bash", ToolSurface::Host)
            .expect("host tool must be refused");
        assert!(refusal.contains("isolated") && refusal.contains("oci"));
    }

    /// The whole point of the surface classification: a kiln tool is not
    /// refused, and nothing had to name it.
    #[test]
    fn a_daemon_tool_under_a_claim_is_not_refused() {
        assert!(isolation_refusal(
            Some(&claimed()),
            "s1",
            "semantic_search",
            ToolSurface::Daemon
        )
        .is_none());
    }
}
