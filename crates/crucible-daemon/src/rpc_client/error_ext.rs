use crucible_core::traits::chat::{ChatError, ChatResult};

/// Extension trait to convert `Result<T, E: Display>` into `ChatResult<T>`.
///
/// Replaces the verbose `.map_err(|e| ChatError::Communication(e.to_string()))` pattern.
pub trait ChatResultExt<T> {
    fn chat_comm(self) -> ChatResult<T>;
}

impl<T, E: std::fmt::Display> ChatResultExt<T> for Result<T, E> {
    fn chat_comm(self) -> ChatResult<T> {
        self.map_err(|e| ChatError::Communication(e.to_string()))
    }
}

/// The message a person reads from a failed daemon call.
///
/// `DaemonClient` reports a JSON-RPC error as `RPC error: {"code":…,"message":…}`:
/// the envelope, serialised, behind a prefix. The daemon's message is the part
/// a person can act on, so a client shows it and not the envelope. Anything
/// that is not that shape (a socket error, a plain string) passes unchanged.
pub fn rpc_error_message(error: &anyhow::Error) -> String {
    let raw = error.to_string();
    raw.strip_prefix("RPC error: ")
        .and_then(|envelope| serde_json::from_str::<serde_json::Value>(envelope).ok())
        .and_then(|value| {
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .filter(|message| !message.is_empty())
                .map(str::to_string)
        })
        .unwrap_or(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rpc_error_shows_the_daemons_message() {
        let error = anyhow::anyhow!(
            r#"RPC error: {{"code":-32603,"message":"plugin 'auto-title' command 'generate' failed: no session"}}"#
        );
        assert_eq!(
            rpc_error_message(&error),
            "plugin 'auto-title' command 'generate' failed: no session"
        );
    }

    #[test]
    fn another_error_passes_unchanged() {
        let error = anyhow::anyhow!("daemon disconnected before replying");
        assert_eq!(
            rpc_error_message(&error),
            "daemon disconnected before replying"
        );
    }
}
