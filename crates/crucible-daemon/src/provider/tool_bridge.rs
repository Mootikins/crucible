//! Bridge between Crucible tool definitions and genai's Tool type.

use crucible_core::traits::llm::LlmToolDefinition;
use genai::chat::Tool;

pub fn llm_tool_to_genai(tool: &LlmToolDefinition) -> Tool {
    let mut converted =
        Tool::new(tool.function.name.clone()).with_description(tool.function.description.clone());
    if let Some(schema) = tool.function.parameters.clone() {
        converted = converted.with_schema(schema);
    }
    converted
}
