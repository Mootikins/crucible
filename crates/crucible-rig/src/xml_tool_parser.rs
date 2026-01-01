//! XML Tool Call Parser
//!
//! Parses XML-style tool calls that small models output as text instead of
//! using native function calling. Supports multiple formats:
//!
//! - Anthropic-style: `<function=name><parameter=key>value</parameter></function>`
//! - Generic: `<tool_call><function=name>...</function></tool_call>`
//!
//! This is a fallback for when llama.cpp's server-side parsing doesn't work.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// A parsed tool call extracted from text
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCall {
    /// Tool/function name
    pub name: String,
    /// Arguments as key-value pairs
    pub arguments: HashMap<String, String>,
    /// Original matched text (for removal from output)
    pub matched_text: String,
}

/// Result of parsing text for tool calls
#[derive(Debug)]
pub struct ParseResult {
    /// Text with tool calls removed
    pub cleaned_text: String,
    /// Extracted tool calls
    pub tool_calls: Vec<ParsedToolCall>,
}

// Regex patterns for different XML tool call formats
static ANTHROPIC_FUNCTION_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches: <function=name><parameter=key>value</parameter>...</function>
    // Also matches: <function=name>\n<parameter=key>\nvalue\n</parameter>\n</function>
    Regex::new(
        r"(?s)<function=([^>]+)>\s*((?:<parameter=([^>]+)>\s*([^<]*?)\s*</parameter>\s*)*)</function>",
    )
    .unwrap()
});

// Partial function call - model may not close the tag
static PARTIAL_FUNCTION_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches: <function=name><parameter=key>value</parameter>... (without closing </function>)
    // This handles cases where the model outputs incomplete XML
    Regex::new(r"(?s)<function=([^>]+)>\s*((?:<parameter=([^>]+)>\s*([^<]*?)\s*</parameter>\s*)+)")
        .unwrap()
});

static PARAMETER_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches individual parameters within a function block
    Regex::new(r"(?s)<parameter=([^>]+)>\s*([^<]*?)\s*</parameter>").unwrap()
});

// Partial parameter - value may span lines, tag may not be closed
static PARTIAL_PARAMETER_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches: <parameter=key>value (possibly without closing tag)
    Regex::new(r"(?s)<parameter=([^>]+)>\s*(\S[^<]*)").unwrap()
});

static TOOL_CALL_WRAPPER_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches: <tool_call>..content..</tool_call>
    Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").unwrap()
});

/// Parse XML-style tool calls from text
///
/// Returns the cleaned text (with tool calls removed) and any extracted tool calls.
pub fn parse_tool_calls(text: &str) -> ParseResult {
    let mut cleaned = text.to_string();
    let mut tool_calls = Vec::new();

    // First, try to find <tool_call> wrappers and unwrap them
    let unwrapped = TOOL_CALL_WRAPPER_RE.replace_all(&cleaned, "$1");
    cleaned = unwrapped.to_string();

    // Parse complete Anthropic-style function calls first
    for cap in ANTHROPIC_FUNCTION_RE.captures_iter(&cleaned.clone()) {
        let full_match = cap.get(0).unwrap().as_str();
        let name = cap.get(1).unwrap().as_str().to_string();
        let params_block = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // Parse parameters from the block
        let mut arguments = HashMap::new();
        for param_cap in PARAMETER_RE.captures_iter(params_block) {
            let key = param_cap.get(1).unwrap().as_str().trim().to_string();
            let value = param_cap.get(2).unwrap().as_str().trim().to_string();
            arguments.insert(key, value);
        }

        tool_calls.push(ParsedToolCall {
            name,
            arguments,
            matched_text: full_match.to_string(),
        });
    }

    // If no complete matches, try partial function calls (missing </function>)
    if tool_calls.is_empty() {
        for cap in PARTIAL_FUNCTION_RE.captures_iter(&cleaned.clone()) {
            let full_match = cap.get(0).unwrap().as_str();
            let name = cap.get(1).unwrap().as_str().to_string();
            let params_block = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            // Parse parameters - try complete first, then partial
            let mut arguments = HashMap::new();
            for param_cap in PARAMETER_RE.captures_iter(params_block) {
                let key = param_cap.get(1).unwrap().as_str().trim().to_string();
                let value = param_cap.get(2).unwrap().as_str().trim().to_string();
                arguments.insert(key, value);
            }

            // If no complete params found, try partial
            if arguments.is_empty() {
                for param_cap in PARTIAL_PARAMETER_RE.captures_iter(params_block) {
                    let key = param_cap.get(1).unwrap().as_str().trim().to_string();
                    let value = param_cap.get(2).unwrap().as_str().trim().to_string();
                    arguments.insert(key, value);
                }
            }

            if !arguments.is_empty() {
                tool_calls.push(ParsedToolCall {
                    name,
                    arguments,
                    matched_text: full_match.to_string(),
                });
            }
        }
    }

    // Last resort: try to find standalone <function=...> with at least one complete </parameter>
    // Only use this when we have evidence the model finished outputting the parameter value
    // (i.e., at least one </parameter> closing tag exists)
    if tool_calls.is_empty() && cleaned.contains("<function=") && cleaned.contains("</parameter>") {
        // Lenient parsing for malformed XML that has params but no </function>
        if let Some(start) = cleaned.find("<function=") {
            // Find the function name
            let after_eq = &cleaned[start + 10..];
            if let Some(end) = after_eq.find('>') {
                let name = after_eq[..end].trim().to_string();
                let rest = &after_eq[end + 1..];

                // Try to find parameters - use complete parameter regex first
                let mut arguments = HashMap::new();
                for param_cap in PARAMETER_RE.captures_iter(rest) {
                    let key = param_cap.get(1).unwrap().as_str().trim().to_string();
                    let value = param_cap.get(2).unwrap().as_str().trim().to_string();
                    arguments.insert(key, value);
                }

                // Find the end of this tool call block
                let matched_end = if let Some(func_end) = rest.find("</function>") {
                    start + 10 + end + 1 + func_end + 11
                } else if let Some(param_end) = rest.rfind("</parameter>") {
                    start + 10 + end + 1 + param_end + 12
                } else {
                    cleaned.len()
                };

                let matched_text = cleaned[start..matched_end].to_string();

                if !arguments.is_empty() {
                    tool_calls.push(ParsedToolCall {
                        name,
                        arguments,
                        matched_text,
                    });
                }
            }
        }
    }

    // Remove matched tool calls from text
    for tc in &tool_calls {
        cleaned = cleaned.replace(&tc.matched_text, "");
    }

    // Clean up any remaining whitespace artifacts
    cleaned = cleaned.trim().to_string();

    ParseResult {
        cleaned_text: cleaned,
        tool_calls,
    }
}

/// Convert parsed tool calls to JSON arguments string (for ChatToolCall)
pub fn arguments_to_json(arguments: &HashMap<String, String>) -> String {
    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string())
}

/// Check if text contains potential XML tool calls
///
/// Fast check before running full regex parsing.
pub fn might_contain_tool_calls(text: &str) -> bool {
    text.contains("<function=") || text.contains("<tool_call>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_parameter() {
        let text = r#"<function=read_file>
<parameter=path>
README.md
</parameter>
</function>"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);

        let tc = &result.tool_calls[0];
        assert_eq!(tc.name, "read_file");
        assert_eq!(tc.arguments.get("path"), Some(&"README.md".to_string()));
        assert!(result.cleaned_text.is_empty());
    }

    #[test]
    fn test_parse_multiple_parameters() {
        let text = r#"<function=write_file><parameter=path>/tmp/test.txt</parameter><parameter=content>Hello world</parameter></function>"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);

        let tc = &result.tool_calls[0];
        assert_eq!(tc.name, "write_file");
        assert_eq!(tc.arguments.get("path"), Some(&"/tmp/test.txt".to_string()));
        assert_eq!(
            tc.arguments.get("content"),
            Some(&"Hello world".to_string())
        );
    }

    #[test]
    fn test_parse_with_surrounding_text() {
        let text = r#"Let me read that file for you.
<function=read_file><parameter=path>test.txt</parameter></function>
I'll show you the contents."#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);
        assert!(result.cleaned_text.contains("Let me read that file"));
        assert!(result.cleaned_text.contains("I'll show you the contents"));
        assert!(!result.cleaned_text.contains("<function"));
    }

    #[test]
    fn test_parse_tool_call_wrapper() {
        let text = r#"<tool_call>
<function=grep><parameter=pattern>fn main</parameter><parameter=path>src</parameter></function>
</tool_call>"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "grep");
    }

    #[test]
    fn test_parse_multiple_tool_calls() {
        let text = r#"<function=glob><parameter=pattern>*.rs</parameter></function>
<function=read_file><parameter=path>main.rs</parameter></function>"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].name, "glob");
        assert_eq!(result.tool_calls[1].name, "read_file");
    }

    #[test]
    fn test_no_tool_calls() {
        let text = "Just some regular text without any tool calls.";

        let result = parse_tool_calls(text);
        assert!(result.tool_calls.is_empty());
        assert_eq!(result.cleaned_text, text);
    }

    #[test]
    fn test_might_contain_tool_calls() {
        assert!(might_contain_tool_calls("<function=test>"));
        assert!(might_contain_tool_calls("<tool_call>"));
        assert!(!might_contain_tool_calls("regular text"));
    }

    #[test]
    fn test_parse_partial_no_closing_function() {
        // Model outputs without closing </function> tag
        let text = r#"<function=read_file>
<parameter=path>
README.md
</parameter>"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);

        let tc = &result.tool_calls[0];
        assert_eq!(tc.name, "read_file");
        assert_eq!(tc.arguments.get("path"), Some(&"README.md".to_string()));
    }

    #[test]
    fn test_parse_very_malformed() {
        // Very malformed - parameter value on newline, no closing tags
        let text = "<function=read_file>\n<parameter=path>\ntest.txt\n</parameter>";

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "read_file");
    }

    #[test]
    fn test_arguments_to_json() {
        let mut args = HashMap::new();
        args.insert("path".to_string(), "test.txt".to_string());
        args.insert("mode".to_string(), "r".to_string());

        let json = arguments_to_json(&args);
        assert!(json.contains("path"));
        assert!(json.contains("test.txt"));
    }

    /// Test fixture simulating streaming chunks
    mod streaming_simulation {
        use super::*;

        /// Simulates the streaming handler's behavior:
        /// - Buffers when XML detected
        /// - Parses when complete
        /// - Returns (chunks_emitted, tool_calls_found)
        fn simulate_streaming(chunks: &[&str]) -> (Vec<String>, Vec<ParsedToolCall>) {
            let mut accumulated = String::new();
            let mut is_buffering = false;
            let mut emitted_len = 0;
            let mut emitted_chunks = Vec::new();
            let mut found_tool_calls = Vec::new();

            for chunk in chunks {
                accumulated.push_str(chunk);

                let might_have_xml = might_contain_tool_calls(&accumulated);

                if might_have_xml && !is_buffering {
                    is_buffering = true;
                }

                if is_buffering {
                    let result = parse_tool_calls(&accumulated);

                    if !result.tool_calls.is_empty() {
                        // Found complete tool calls
                        found_tool_calls.extend(result.tool_calls);
                        accumulated = result.cleaned_text.clone();
                        emitted_len = 0;

                        if !result.cleaned_text.is_empty() {
                            emitted_chunks.push(result.cleaned_text);
                            emitted_len = accumulated.len();
                        }

                        is_buffering = false;
                    }
                    // If no complete tool calls, keep buffering (don't emit)
                } else {
                    // No XML, emit normally
                    emitted_chunks.push(chunk.to_string());
                    emitted_len = accumulated.len();
                }
            }

            // End of stream: emit any remaining buffered text
            if is_buffering && emitted_len < accumulated.len() {
                let remaining = &accumulated[emitted_len..];
                if !remaining.is_empty() {
                    emitted_chunks.push(remaining.to_string());
                }
            }

            (emitted_chunks, found_tool_calls)
        }

        #[test]
        fn test_streaming_complete_xml_tool_call() {
            // Simulate receiving XML in chunks like a real LLM would output
            let chunks = &[
                "<function=",
                "read_file>",
                "\n<parameter=path>",
                "\nREADME.md",
                "\n</parameter>",
                "</function>",
            ];

            let (emitted, tool_calls) = simulate_streaming(chunks);

            // Should find the tool call
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "read_file");
            assert_eq!(
                tool_calls[0].arguments.get("path"),
                Some(&"README.md".to_string())
            );

            // Should NOT emit raw XML (it was buffered and parsed)
            let all_output: String = emitted.join("");
            assert!(
                !all_output.contains("<function="),
                "Should not emit raw XML: {}",
                all_output
            );
        }

        #[test]
        fn test_streaming_partial_xml_no_closing_tag() {
            // Some models don't output </function>
            let chunks = &[
                "<function=read_file>",
                "\n<parameter=path>",
                "\ntest.txt",
                "\n</parameter>",
            ];

            let (emitted, tool_calls) = simulate_streaming(chunks);

            // Should still find the tool call (partial parsing)
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "read_file");

            // Should NOT emit raw XML
            let all_output: String = emitted.join("");
            assert!(
                !all_output.contains("<function="),
                "Should not emit raw XML: {}",
                all_output
            );
        }

        #[test]
        fn test_streaming_text_before_xml() {
            let chunks = &[
                "Let me read that file for you.\n",
                "<function=read_file>",
                "<parameter=path>test.txt</parameter>",
                "</function>",
            ];

            let (emitted, tool_calls) = simulate_streaming(chunks);

            // Should find the tool call
            assert_eq!(tool_calls.len(), 1);

            // Should emit the text before XML
            assert!(emitted.iter().any(|s| s.contains("Let me read")));

            // Should NOT emit the XML itself
            let all_output: String = emitted.join("");
            assert!(!all_output.contains("<function="));
        }

        #[test]
        fn test_streaming_no_xml() {
            let chunks = &["Hello, ", "how can I ", "help you today?"];

            let (emitted, tool_calls) = simulate_streaming(chunks);

            // No tool calls
            assert!(tool_calls.is_empty());

            // All text emitted
            let all_output: String = emitted.join("");
            assert_eq!(all_output, "Hello, how can I help you today?");
        }

        #[test]
        fn test_streaming_incomplete_xml_at_end() {
            // If stream ends mid-XML without completing, emit as text
            let chunks = &["<function=incomplete", ">but_never_finishes"];

            let (emitted, tool_calls) = simulate_streaming(chunks);

            // No tool calls (incomplete)
            assert!(tool_calls.is_empty());

            // Should emit the incomplete XML as text (fallback)
            let all_output: String = emitted.join("");
            assert!(all_output.contains("<function="));
        }
    }
}
