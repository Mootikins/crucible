//! Annotation discovery for Lua/Fennel scripts
//!
//! Parses LDoc-style annotations from Lua comments to discover tools, hooks, and plugins.
//!
//! ## Lua Annotation Format
//!
//! ```lua
//! --- Search the knowledge base
//! -- @tool desc="Search for notes"
//! -- @param query string The search query
//! -- @param limit number? Maximum results (optional)
//! -- @return {SearchResult}
//! function search(query, limit)
//!     return crucible.search(query, limit or 10)
//! end
//! ```
//!
//! ## Fennel Annotation Format
//!
//! ```fennel
//! ;;; Search the knowledge base
//! ;; @tool desc="Search for notes"
//! ;; @param query string The search query
//! (fn search [query limit]
//!   (crucible.search query (or limit 10)))
//! ```

use crate::error::LuaError;
use crate::types::{LuaTool, ToolParam};
use regex::Regex;
use std::path::Path;

/// Discovered tool from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredTool {
    pub name: String,
    pub description: String,
    pub params: Vec<DiscoveredParam>,
    pub return_type: Option<String>,
    pub source_path: String,
    pub is_fennel: bool,
}

/// Discovered parameter from annotations
#[derive(Debug, Clone)]
pub struct DiscoveredParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub optional: bool,
}

/// Discovered hook from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredHook {
    pub name: String,
    pub event_type: String,
    pub pattern: String,
    pub priority: i64,
    pub description: String,
    pub source_path: String,
    pub handler_fn: String,
    pub is_fennel: bool,
}

/// Discovered plugin from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub description: String,
    pub watch_patterns: Vec<String>,
    pub source_path: String,
    pub factory_fn: String,
    pub is_fennel: bool,
}

/// Annotation parser for Lua/Fennel files
pub struct AnnotationParser {
    /// Regex for Lua function declarations
    lua_function_re: Regex,
    /// Regex for Fennel function declarations
    fennel_function_re: Regex,
}

impl Default for AnnotationParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotationParser {
    pub fn new() -> Self {
        Self {
            // Match Lua function: function name(...) or local function name(...)
            lua_function_re: Regex::new(
                r"(?m)^[ \t]*(?:local\s+)?function\s+(\w+)\s*\("
            ).unwrap(),
            // Match Fennel function: (fn name [...] or (defn name [...]
            fennel_function_re: Regex::new(
                r"(?m)\((?:fn|defn)\s+(\w+)\s*\["
            ).unwrap(),
        }
    }

    /// Parse tools from Lua source
    pub fn parse_lua_tools(&self, source: &str, path: &Path) -> Result<Vec<DiscoveredTool>, LuaError> {
        self.parse_tools(source, path, false)
    }

    /// Parse tools from Fennel source
    pub fn parse_fennel_tools(&self, source: &str, path: &Path) -> Result<Vec<DiscoveredTool>, LuaError> {
        self.parse_tools(source, path, true)
    }

    /// Parse tools from source (Lua or Fennel)
    fn parse_tools(&self, source: &str, path: &Path, is_fennel: bool) -> Result<Vec<DiscoveredTool>, LuaError> {
        let mut tools = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        for block in blocks {
            if block.has_annotation("tool") {
                let tool = self.parse_tool_from_block(&block, path, is_fennel)?;
                tools.push(tool);
            }
        }

        Ok(tools)
    }

    /// Parse hooks from source
    pub fn parse_hooks(&self, source: &str, path: &Path, is_fennel: bool) -> Result<Vec<DiscoveredHook>, LuaError> {
        let mut hooks = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        for block in blocks {
            if block.has_annotation("hook") {
                let hook = self.parse_hook_from_block(&block, path, is_fennel)?;
                hooks.push(hook);
            }
        }

        Ok(hooks)
    }

    /// Parse plugins from source
    pub fn parse_plugins(&self, source: &str, path: &Path, is_fennel: bool) -> Result<Vec<DiscoveredPlugin>, LuaError> {
        let mut plugins = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        for block in blocks {
            if block.has_annotation("plugin") {
                let plugin = self.parse_plugin_from_block(&block, path, is_fennel)?;
                plugins.push(plugin);
            }
        }

        Ok(plugins)
    }

    /// Find all annotated code blocks in source
    fn find_annotated_blocks(&self, source: &str, is_fennel: bool) -> Vec<AnnotatedBlock> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let comment_prefix = if is_fennel { ";;" } else { "--" };
        let doc_prefix = if is_fennel { ";;;" } else { "---" };

        let mut i = 0;
        while i < lines.len() {
            // Look for doc comment start
            if lines[i].trim().starts_with(doc_prefix) {
                let start = i;
                let mut annotations = Vec::new();
                let mut description = String::new();

                // Collect the doc comment (first line)
                let first_line = lines[i].trim().trim_start_matches(doc_prefix).trim();
                if !first_line.starts_with('@') {
                    description = first_line.to_string();
                }
                i += 1;

                // Collect continuation comments
                while i < lines.len() && lines[i].trim().starts_with(comment_prefix)
                    && !lines[i].trim().starts_with(doc_prefix) {
                    let line = lines[i].trim().trim_start_matches(comment_prefix).trim();
                    if line.starts_with('@') {
                        annotations.push(line.to_string());
                    } else if !line.is_empty() && description.is_empty() {
                        description = line.to_string();
                    }
                    i += 1;
                }

                // Skip empty lines
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }

                // Look for function declaration
                if i < lines.len() {
                    let fn_name = self.extract_function_name(lines[i], is_fennel);
                    if let Some(name) = fn_name {
                        blocks.push(AnnotatedBlock {
                            description,
                            annotations,
                            function_name: name,
                            line_number: start,
                        });
                    }
                }
            } else {
                i += 1;
            }
        }

        blocks
    }

    /// Extract function name from a line
    fn extract_function_name(&self, line: &str, is_fennel: bool) -> Option<String> {
        if is_fennel {
            // Match (fn name [ or (defn name [
            self.fennel_function_re
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        } else {
            // Match function name( or local function name(
            self.lua_function_re
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        }
    }

    /// Parse a tool from an annotated block
    fn parse_tool_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredTool, LuaError> {
        let mut description = block.description.clone();
        let mut params = Vec::new();
        let mut return_type = None;

        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@tool") {
                // Parse @tool desc="..." or just @tool
                if let Some(desc) = extract_quoted_value(rest, "desc") {
                    description = desc;
                }
            } else if let Some(rest) = annotation.strip_prefix("@param") {
                // Parse @param name type Description
                if let Some(param) = parse_param_annotation(rest.trim()) {
                    params.push(param);
                }
            } else if let Some(rest) = annotation.strip_prefix("@return") {
                return_type = Some(rest.trim().to_string());
            }
        }

        Ok(DiscoveredTool {
            name: block.function_name.clone(),
            description,
            params,
            return_type,
            source_path: path.to_string_lossy().to_string(),
            is_fennel,
        })
    }

    /// Parse a hook from an annotated block
    fn parse_hook_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredHook, LuaError> {
        let mut event_type = String::new();
        let mut pattern = "*".to_string();
        let mut priority = 100i64;

        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@hook") {
                // Parse @hook event="tool:after" pattern="search_*" priority=50
                if let Some(evt) = extract_quoted_value(rest, "event") {
                    event_type = evt;
                }
                if let Some(pat) = extract_quoted_value(rest, "pattern") {
                    pattern = pat;
                }
                if let Some(pri) = extract_int_value(rest, "priority") {
                    priority = pri;
                }
            }
        }

        if event_type.is_empty() {
            return Err(LuaError::InvalidTool(format!(
                "Hook '{}' missing event type",
                block.function_name
            )));
        }

        Ok(DiscoveredHook {
            name: block.function_name.clone(),
            event_type,
            pattern,
            priority,
            description: block.description.clone(),
            source_path: path.to_string_lossy().to_string(),
            handler_fn: block.function_name.clone(),
            is_fennel,
        })
    }

    /// Parse a plugin from an annotated block
    fn parse_plugin_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredPlugin, LuaError> {
        let mut watch_patterns = Vec::new();

        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@plugin") {
                // Parse @plugin watch=["*.md", "*.txt"]
                if let Some(patterns) = extract_array_value(rest, "watch") {
                    watch_patterns = patterns;
                }
            }
        }

        Ok(DiscoveredPlugin {
            name: block.function_name.clone(),
            description: block.description.clone(),
            watch_patterns,
            source_path: path.to_string_lossy().to_string(),
            factory_fn: block.function_name.clone(),
            is_fennel,
        })
    }
}

/// An annotated block of code
#[derive(Debug)]
struct AnnotatedBlock {
    description: String,
    annotations: Vec<String>,
    function_name: String,
    #[allow(dead_code)]
    line_number: usize,
}

impl AnnotatedBlock {
    fn has_annotation(&self, name: &str) -> bool {
        let prefix = format!("@{}", name);
        self.annotations.iter().any(|a| a.starts_with(&prefix))
    }
}

/// Extract a quoted value like `key="value"` from annotation text
fn extract_quoted_value(text: &str, key: &str) -> Option<String> {
    let pattern = format!(r#"{}="([^"]*)""#, key);
    let re = Regex::new(&pattern).ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract an integer value like `key=42` from annotation text
fn extract_int_value(text: &str, key: &str) -> Option<i64> {
    let pattern = format!(r"{}=(-?\d+)", key);
    let re = Regex::new(&pattern).ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// Extract an array value like `key=["a", "b"]` from annotation text
fn extract_array_value(text: &str, key: &str) -> Option<Vec<String>> {
    let pattern = format!(r#"{}=\[([^\]]*)\]"#, key);
    let re = Regex::new(&pattern).ok()?;

    re.captures(text).map(|c| {
        let inner = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let string_re = Regex::new(r#""([^"]*)""#).unwrap();
        string_re
            .captures_iter(inner)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    })
}

/// Parse a @param annotation like "name type Description text"
fn parse_param_annotation(text: &str) -> Option<DiscoveredParam> {
    let parts: Vec<&str> = text.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].to_string();
    let mut param_type = parts[1].to_string();
    let description = parts.get(2).map(|s| s.to_string()).unwrap_or_default();

    // Check for optional marker (type?)
    let optional = param_type.ends_with('?');
    if optional {
        param_type = param_type.trim_end_matches('?').to_string();
    }

    Some(DiscoveredParam {
        name,
        param_type,
        description,
        optional,
    })
}

impl From<DiscoveredTool> for LuaTool {
    fn from(tool: DiscoveredTool) -> Self {
        LuaTool {
            name: tool.name,
            description: tool.description,
            params: tool.params.into_iter().map(|p| ToolParam {
                name: p.name,
                param_type: p.param_type,
                description: p.description,
                required: !p.optional,
                default: None,
            }).collect(),
            source_path: tool.source_path,
            is_fennel: tool.is_fennel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lua_tool_simple() {
        let source = r#"
--- Search the knowledge base
-- @tool
-- @param query string The search query
-- @param limit number? Maximum results
function search(query, limit)
    return crucible.search(query, limit or 10)
end
"#;

        let parser = AnnotationParser::new();
        let tools = parser.parse_lua_tools(source, Path::new("test.lua")).unwrap();

        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool.name, "search");
        assert_eq!(tool.description, "Search the knowledge base");
        assert_eq!(tool.params.len(), 2);

        assert_eq!(tool.params[0].name, "query");
        assert_eq!(tool.params[0].param_type, "string");
        assert!(!tool.params[0].optional);

        assert_eq!(tool.params[1].name, "limit");
        assert_eq!(tool.params[1].param_type, "number");
        assert!(tool.params[1].optional);
    }

    #[test]
    fn test_parse_lua_tool_with_desc_override() {
        let source = r#"
--- Default description
-- @tool desc="Custom description"
function handler()
    return {}
end
"#;

        let parser = AnnotationParser::new();
        let tools = parser.parse_lua_tools(source, Path::new("test.lua")).unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "Custom description");
    }

    #[test]
    fn test_parse_lua_local_function() {
        let source = r#"
--- A local function tool
-- @tool
local function helper(x)
    return x * 2
end
"#;

        let parser = AnnotationParser::new();
        let tools = parser.parse_lua_tools(source, Path::new("test.lua")).unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "helper");
    }

    #[test]
    fn test_parse_fennel_tool() {
        let source = r#"
;;; Greet the user
;; @tool desc="Returns a greeting"
;; @param name string Name to greet
(fn greet [name]
  (.. "Hello, " name "!"))
"#;

        let parser = AnnotationParser::new();
        let tools = parser.parse_fennel_tools(source, Path::new("test.fnl")).unwrap();

        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool.name, "greet");
        assert_eq!(tool.description, "Returns a greeting");
        assert!(tool.is_fennel);
        assert_eq!(tool.params.len(), 1);
        assert_eq!(tool.params[0].name, "name");
    }

    #[test]
    fn test_parse_lua_hook() {
        let source = r#"
--- Filter search results
-- @hook event="tool:after" pattern="search_*" priority=50
function filter_results(ctx, event)
    return event
end
"#;

        let parser = AnnotationParser::new();
        let hooks = parser.parse_hooks(source, Path::new("test.lua"), false).unwrap();

        assert_eq!(hooks.len(), 1);
        let hook = &hooks[0];
        assert_eq!(hook.name, "filter_results");
        assert_eq!(hook.event_type, "tool:after");
        assert_eq!(hook.pattern, "search_*");
        assert_eq!(hook.priority, 50);
    }

    #[test]
    fn test_parse_lua_plugin() {
        let source = r#"
--- My custom plugin
-- @plugin watch=["*.md", "*.txt"]
function create()
    return {
        tools = function() return {} end,
        dispatch = function(name, args) end
    }
end
"#;

        let parser = AnnotationParser::new();
        let plugins = parser.parse_plugins(source, Path::new("test.lua"), false).unwrap();

        assert_eq!(plugins.len(), 1);
        let plugin = &plugins[0];
        assert_eq!(plugin.name, "create");
        assert_eq!(plugin.watch_patterns, vec!["*.md", "*.txt"]);
    }

    #[test]
    fn test_multiple_tools_in_file() {
        let source = r#"
--- First tool
-- @tool
function tool_one()
    return 1
end

-- Regular comment, not a tool
function not_a_tool()
    return 0
end

--- Second tool
-- @tool
-- @param x number Input value
function tool_two(x)
    return x * 2
end
"#;

        let parser = AnnotationParser::new();
        let tools = parser.parse_lua_tools(source, Path::new("test.lua")).unwrap();

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "tool_one");
        assert_eq!(tools[1].name, "tool_two");
    }

    #[test]
    fn test_convert_to_lua_tool() {
        let discovered = DiscoveredTool {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            params: vec![
                DiscoveredParam {
                    name: "x".to_string(),
                    param_type: "number".to_string(),
                    description: "Input".to_string(),
                    optional: false,
                },
            ],
            return_type: Some("number".to_string()),
            source_path: "test.lua".to_string(),
            is_fennel: false,
        };

        let tool: LuaTool = discovered.into();
        assert_eq!(tool.name, "test");
        assert_eq!(tool.params.len(), 1);
        assert!(tool.params[0].required);
    }
}
