use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crucible_core::types::acp::ToolCallInfo;

/// Configuration for the ACP client.
///
/// Only `agent_path` is required. Each other field falls back to `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Path to the agent executable or script
    pub agent_path: PathBuf,

    /// Command-line arguments to pass to the agent
    pub agent_args: Option<Vec<String>>,

    /// Working directory for the agent process
    pub working_dir: Option<PathBuf>,

    /// Environment variables to pass to the agent
    pub env_vars: Option<Vec<(String, String)>>,

    /// Timeout for agent operations (in milliseconds)
    pub timeout_ms: Option<u64>,
}

#[derive(Default)]
pub(super) struct StreamingState {
    pub(super) tool_calls: Vec<ToolCallInfo>,
    pub(super) notification_count: usize,
    /// Raw accumulated text (for deduplication of full-text re-sends).
    /// Some ACP agents (e.g. cursor-acp) send the complete accumulated text
    /// as a final notification before the JSON-RPC response. We track the
    /// accumulated text here to detect and skip these re-sends.
    pub(super) accumulated_text: String,
    /// Set when a streaming callback returns `false`, meaning the receiver
    /// (the daemon's turn stream) was dropped — i.e. the turn was cancelled.
    /// The read loop reacts by sending `session/cancel` to the agent so it
    /// stops generating server-side instead of running to completion.
    pub(super) cancelled: bool,
}

impl StreamingState {
    pub(super) fn append_text(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.accumulated_text.push_str(text);
    }

    /// Check if incoming text is a full re-send of already-accumulated content.
    /// Some ACP agents (e.g. cursor-acp) emit the complete response as a final
    /// `session/update` notification. We detect this by checking if the incoming
    /// text equals the accumulated text so far.
    pub(super) fn is_duplicate_resend(&self, text: &str) -> bool {
        !self.accumulated_text.is_empty() && text.trim() == self.accumulated_text.trim()
    }

    pub(super) fn title_for_tool(&self, id: &str) -> Option<String> {
        self.tool_calls
            .iter()
            .find(|tool| tool.id.as_deref() == Some(id))
            .map(|tool| tool.title.clone())
    }
}

#[cfg(test)]
mod streaming_state_proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generator for a "text chunk" — strings the streaming aggregator might
    /// see. Mixes whitespace-only, plain text, multiline, and unicode.
    fn arb_chunk() -> impl Strategy<Value = String> {
        prop_oneof![
            // Whitespace-only (gets dropped by append_text)
            "[ \t\n]{0,8}",
            // Plain ASCII content
            "[a-zA-Z0-9 ]{1,32}",
            // Multiline content
            "[a-zA-Z0-9 ]{1,16}\n[a-zA-Z0-9 ]{1,16}",
            // Unicode-ish (BMP only to keep test fast)
            "[a-zA-Z0-9αβγδ世界 ]{1,32}",
        ]
    }

    proptest! {
        // A sequence of chunks, with bounded count for test speed.
        #![proptest_config(ProptestConfig {
            cases: 256,
            ..ProptestConfig::default()
        })]

        /// After replaying any sequence of chunks, accumulated_text equals
        /// the in-order concatenation of the non-whitespace-only inputs.
        /// This is the core "no chunk lost, none reordered" invariant.
        #[test]
        fn append_text_concatenates_non_whitespace_chunks(
            chunks in proptest::collection::vec(arb_chunk(), 0..32)
        ) {
            let mut state = StreamingState::default();
            let mut expected = String::new();
            for chunk in &chunks {
                if !chunk.trim().is_empty() {
                    expected.push_str(chunk);
                }
                state.append_text(chunk);
            }
            prop_assert_eq!(state.accumulated_text, expected);
        }

        /// After appending any non-whitespace chunk, the same chunk is
        /// recognized as a duplicate-resend if presented again. This is
        /// the cursor-acp dedup invariant. Generator constrained to
        /// strings with at least one non-whitespace character because
        /// `append_text` is documented to drop whitespace-only inputs.
        #[test]
        fn appended_text_is_recognized_as_duplicate(chunk in "[a-zA-Z0-9][a-zA-Z0-9 ]{0,40}") {
            let mut state = StreamingState::default();
            state.append_text(&chunk);
            // The chunk is now in accumulated_text; presenting it again
            // (or its trim-equivalent) must register as duplicate.
            prop_assert!(state.is_duplicate_resend(&chunk));
            let padded = format!("  {}  ", chunk);
            prop_assert!(state.is_duplicate_resend(&padded));
        }

        /// is_duplicate_resend is false on a freshly-constructed state,
        /// regardless of input. (No false positives on empty accumulator.)
        #[test]
        fn fresh_state_never_reports_duplicate(s in ".{0,40}") {
            let state = StreamingState::default();
            prop_assert!(!state.is_duplicate_resend(&s));
        }

        /// is_duplicate_resend matches the documented spec: true iff
        /// accumulated is non-empty and trims equal.
        #[test]
        fn duplicate_resend_matches_spec(
            seed in "[a-zA-Z0-9 ]{1,40}",
            candidate in "[a-zA-Z0-9 ]{0,40}",
        ) {
            let mut state = StreamingState::default();
            state.append_text(&seed);
            let expected = !state.accumulated_text.is_empty()
                && candidate.trim() == state.accumulated_text.trim();
            prop_assert_eq!(state.is_duplicate_resend(&candidate), expected);
        }
    }
}

#[cfg(test)]
mod client_config_tests {
    use super::*;

    /// Only `agent_path` is required on the wire. Every other field falls
    /// back to its default, and a removed field (`max_retries`) is ignored.
    #[test]
    fn client_config_parses_with_only_agent_path() {
        let config: ClientConfig =
            serde_json::from_str(r#"{"agent_path":"/bin/agent","max_retries":3}"#).unwrap();
        assert_eq!(config.agent_path, PathBuf::from("/bin/agent"));
        assert_eq!(config.agent_args, None);
        assert_eq!(config.working_dir, None);
        assert_eq!(config.env_vars, None);
        assert_eq!(config.timeout_ms, None);

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "agent_path": "/bin/agent",
                "agent_args": null,
                "working_dir": null,
                "env_vars": null,
                "timeout_ms": null,
            })
        );
    }
}
