use agent_client_protocol::schema::v1::ToolCallStatus;

use super::streaming::elide;
use super::CrucibleAcpClient;
use crucible_core::text::sanitize_single_line;

impl CrucibleAcpClient {
    pub(super) fn extract_tool_result(raw_output: Option<&serde_json::Value>) -> Option<String> {
        raw_output.map(Self::format_json_value)
    }

    pub(super) fn extract_tool_error(
        status: Option<ToolCallStatus>,
        raw_output: Option<&serde_json::Value>,
    ) -> Option<String> {
        let output_error = raw_output
            .and_then(|value| value.get("error"))
            .map(Self::format_json_value)
            .filter(|value| !value.is_empty());

        if output_error.is_some() {
            return output_error;
        }

        if status == Some(ToolCallStatus::Failed) {
            // claude-agent-acp forwards the failed tool_result's content
            // blocks as `rawOutput` — the actual reason ("File not found: …")
            // lives in their text, and a status with no `error` key used to
            // collapse it to the generic label below. The text is
            // agent-authored and renders as a one-line error label, so it
            // gets the same sanitising and display cap as `describe_rpc_error`.
            return Some(
                raw_output
                    .and_then(Self::content_block_text)
                    .map(|text| sanitize_single_line(&elide(&text)))
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "Tool call failed".to_string()),
            );
        }

        None
    }

    /// Text carried by a tool output that is a bare string or a content-block
    /// array (`[{"type":"text","text":…}, …]`). None when neither shape holds
    /// or no block carries text.
    fn content_block_text(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Array(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
            _ => None,
        }
    }

    fn format_json_value(value: &serde_json::Value) -> String {
        if let Some(s) = value.as_str() {
            s.to_string()
        } else {
            value.to_string()
        }
    }
}
