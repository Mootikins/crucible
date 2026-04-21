use super::types::ParsedRule;

/// Parse a permission rule string into its components
///
/// Format: `"tool:pattern"` or `"tool:server:pattern"` for MCP/plugin rules
///
/// # Examples
///
/// - `"bash:cargo test *"` → tool="bash", server=None, pattern="cargo test *"
/// - `"mcp:github:create_issue"` → tool="mcp", server=Some("github"), pattern="create_issue"
/// - `"edit:src/**"` → tool="edit", server=None, pattern="src/**"
///
/// # Errors
///
/// Returns `Err` if:
/// - No colon separator found
/// - Tool name is empty
/// - For MCP/plugin rules, server name is empty
pub fn parse_rule(s: &str) -> Result<ParsedRule, String> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();

    match parts.len() {
        2 => {
            let tool = parts[0];
            let pattern = parts[1];

            if tool.is_empty() {
                return Err("Tool name cannot be empty".to_string());
            }

            Ok(ParsedRule {
                tool: tool.to_string(),
                server: None,
                pattern: pattern.to_string(),
            })
        }
        3 => {
            let tool = parts[0];
            let second = parts[1];
            let third = parts[2];

            if tool.is_empty() {
                return Err("Tool name cannot be empty".to_string());
            }

            if matches!(tool, "mcp" | "plugin") {
                if second.is_empty() {
                    return Err("Server name cannot be empty".to_string());
                }

                return Ok(ParsedRule {
                    tool: tool.to_string(),
                    server: Some(second.to_string()),
                    pattern: third.to_string(),
                });
            }

            Ok(ParsedRule {
                tool: tool.to_string(),
                server: None,
                pattern: format!("{second}:{third}"),
            })
        }
        _ => Err("Rule must contain at least one colon (e.g., 'tool:pattern')".to_string()),
    }
}
