//! Annotation discovery for Lua/Fennel scripts
//!
//! Parses LDoc-style annotations from Lua comments to discover tools, handlers, and plugins.
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

fn is_fennel_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "fnl")
}

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

/// Discovered handler from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredHandler {
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

/// Discovered slash command from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredCommand {
    /// Command name (without leading /)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameters the command accepts
    pub params: Vec<DiscoveredParam>,
    /// Hint shown after command name in UI
    pub input_hint: Option<String>,
    /// Path to source file containing the handler
    pub source_path: String,
    /// Name of the handler function in the source
    pub handler_fn: String,
    /// Whether this is a Fennel source
    pub is_fennel: bool,
}

#[derive(Debug, Clone)]
pub struct DiscoveredView {
    pub name: String,
    pub description: String,
    pub source_path: String,
    pub view_fn: String,
    pub handler_fn: Option<String>,
    pub is_fennel: bool,
}

/// Annotation parser for Lua/Fennel files
///
/// Note: For plugins managed by `PluginManager`, prefer returning a spec table
/// from `init.lua` instead of annotations. This parser remains used by
/// `LuaScriptHandlerRegistry` for handler discovery in non-plugin scripts.
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
            // Match Lua function: function name(...) or function M.name(...) or local function name(...)
            lua_function_re: Regex::new(r"(?m)^[ \t]*(?:local\s+)?function\s+(?:\w+\.)?(\w+)\s*\(")
                .unwrap(),
            // Match Fennel function: (fn name [...] or (defn name [...] or (fn M.name [...]
            fennel_function_re: Regex::new(r"(?m)\((?:fn|defn)\s+(?:\w+\.)?(\w+)\s*\[").unwrap(),
        }
    }

    pub fn parse_tools(&self, source: &str, path: &Path) -> Result<Vec<DiscoveredTool>, LuaError> {
        let is_fennel = is_fennel_path(path);
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

    pub fn parse_handlers(
        &self,
        source: &str,
        path: &Path,
    ) -> Result<Vec<DiscoveredHandler>, LuaError> {
        let is_fennel = is_fennel_path(path);
        let mut handlers = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        for block in blocks {
            // Support both @handler (preferred) and @hook (backwards compat)
            if block.has_annotation("handler") || block.has_annotation("hook") {
                let handler = self.parse_handler_from_block(&block, path, is_fennel)?;
                handlers.push(handler);
            }
        }

        Ok(handlers)
    }

    pub fn parse_plugins(
        &self,
        source: &str,
        path: &Path,
    ) -> Result<Vec<DiscoveredPlugin>, LuaError> {
        let is_fennel = is_fennel_path(path);
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

    pub fn parse_commands(
        &self,
        source: &str,
        path: &Path,
    ) -> Result<Vec<DiscoveredCommand>, LuaError> {
        let is_fennel = is_fennel_path(path);
        let mut commands = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        for block in blocks {
            if block.has_annotation("command") {
                let command = self.parse_command_from_block(&block, path, is_fennel)?;
                commands.push(command);
            }
        }

        Ok(commands)
    }

    pub fn parse_views(&self, source: &str, path: &Path) -> Result<Vec<DiscoveredView>, LuaError> {
        let is_fennel = is_fennel_path(path);
        let mut views = Vec::new();
        let blocks = self.find_annotated_blocks(source, is_fennel);

        // First pass: collect views
        for block in &blocks {
            if block.has_annotation("view") && !block.has_annotation("view.handler") {
                let view = self.parse_view_from_block(block, path, is_fennel)?;
                views.push(view);
            }
        }

        // Second pass: match handlers to views
        for block in &blocks {
            if block.has_annotation("view.handler") {
                let handler_name = self.get_view_handler_name(block);
                if let Some(name) = handler_name {
                    if let Some(view) = views.iter_mut().find(|v| v.name == name) {
                        view.handler_fn = Some(block.function_name.clone());
                    }
                }
            }
        }

        Ok(views)
    }

    fn parse_view_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredView, LuaError> {
        let mut name = block.function_name.clone();
        let mut description = block.description.clone();

        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@view") {
                if rest.starts_with('.') {
                    continue; // Skip @view.handler
                }
                if let Some(n) = extract_quoted_value(rest, "name") {
                    name = n;
                }
                if let Some(desc) = extract_quoted_value(rest, "desc") {
                    description = desc;
                }
            }
        }

        Ok(DiscoveredView {
            name,
            description,
            source_path: path.to_string_lossy().to_string(),
            view_fn: block.function_name.clone(),
            handler_fn: None,
            is_fennel,
        })
    }

    fn get_view_handler_name(&self, block: &AnnotatedBlock) -> Option<String> {
        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@view.handler") {
                if let Some(name) = extract_quoted_value(rest, "name") {
                    return Some(name);
                }
            }
        }
        None
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
                let mut annotations = Vec::new();
                let mut description = String::new();

                // Collect the doc comment (first line)
                let first_line = lines[i].trim().trim_start_matches(doc_prefix).trim();
                if !first_line.starts_with('@') {
                    description = first_line.to_string();
                }
                i += 1;

                // Collect continuation comments
                while i < lines.len()
                    && lines[i].trim().starts_with(comment_prefix)
                    && !lines[i].trim().starts_with(doc_prefix)
                {
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

    /// Parse a handler from an annotated block
    ///
    /// Supports both `@handler` (preferred) and `@hook` (backwards compat) annotations.
    fn parse_handler_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredHandler, LuaError> {
        let mut event_type = String::new();
        let mut pattern = "*".to_string();
        let mut priority = 100i64;

        for annotation in &block.annotations {
            // Try @handler first (preferred), then @hook (backwards compat)
            let rest = annotation
                .strip_prefix("@handler")
                .or_else(|| annotation.strip_prefix("@hook"));

            if let Some(rest) = rest {
                // Parse event="tool:after" pattern="search_*" priority=50
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
                "Handler '{}' missing event type",
                block.function_name
            )));
        }

        Ok(DiscoveredHandler {
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

    fn parse_command_from_block(
        &self,
        block: &AnnotatedBlock,
        path: &Path,
        is_fennel: bool,
    ) -> Result<DiscoveredCommand, LuaError> {
        let mut name = block.function_name.clone();
        let mut description = block.description.clone();
        let mut params = Vec::new();
        let mut input_hint = None;

        for annotation in &block.annotations {
            if let Some(rest) = annotation.strip_prefix("@command") {
                if let Some(n) = extract_quoted_value(rest, "name") {
                    name = n;
                }
                if let Some(desc) = extract_quoted_value(rest, "desc") {
                    description = desc;
                }
                if let Some(hint) = extract_quoted_value(rest, "hint") {
                    input_hint = Some(hint);
                }
            } else if let Some(rest) = annotation.strip_prefix("@param") {
                if let Some(param) = parse_param_annotation(rest.trim()) {
                    params.push(param);
                }
            }
        }

        Ok(DiscoveredCommand {
            name,
            description,
            params,
            input_hint,
            source_path: path.to_string_lossy().to_string(),
            handler_fn: block.function_name.clone(),
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
            params: tool
                .params
                .into_iter()
                .map(|p| ToolParam {
                    name: p.name,
                    param_type: p.param_type,
                    description: p.description,
                    required: !p.optional,
                    default: None,
                })
                .collect(),
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
        let tools = parser.parse_tools(source, Path::new("test.lua")).unwrap();

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
        let tools = parser.parse_tools(source, Path::new("test.lua")).unwrap();

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
        let tools = parser.parse_tools(source, Path::new("test.lua")).unwrap();

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
        let tools = parser.parse_tools(source, Path::new("test.fnl")).unwrap();

        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool.name, "greet");
        assert_eq!(tool.description, "Returns a greeting");
        assert!(tool.is_fennel);
        assert_eq!(tool.params.len(), 1);
        assert_eq!(tool.params[0].name, "name");
    }

    #[test]
    fn test_parse_lua_handler() {
        let source = r#"
--- Filter search results
-- @handler event="tool:after" pattern="search_*" priority=50
function filter_results(ctx, event)
    return event
end
"#;

        let parser = AnnotationParser::new();
        let handlers = parser
            .parse_handlers(source, Path::new("test.lua"))
            .unwrap();

        assert_eq!(handlers.len(), 1);
        let handler = &handlers[0];
        assert_eq!(handler.name, "filter_results");
        assert_eq!(handler.event_type, "tool:after");
        assert_eq!(handler.pattern, "search_*");
        assert_eq!(handler.priority, 50);
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
        let plugins = parser.parse_plugins(source, Path::new("test.lua")).unwrap();

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
        let tools = parser.parse_tools(source, Path::new("test.lua")).unwrap();

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "tool_one");
        assert_eq!(tools[1].name, "tool_two");
    }

    #[test]
    fn test_convert_to_lua_tool() {
        let discovered = DiscoveredTool {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            params: vec![DiscoveredParam {
                name: "x".to_string(),
                param_type: "number".to_string(),
                description: "Input".to_string(),
                optional: false,
            }],
            return_type: Some("number".to_string()),
            source_path: "test.lua".to_string(),
            is_fennel: false,
        };

        let tool: LuaTool = discovered.into();
        assert_eq!(tool.name, "test");
        assert_eq!(tool.params.len(), 1);
        assert!(tool.params[0].required);
    }

    #[test]
    fn test_parse_lua_command_simple() {
        let source = r#"
--- Summarize notes
-- @command
function summarize()
    return "Summary"
end
"#;

        let parser = AnnotationParser::new();
        let commands = parser
            .parse_commands(source, Path::new("test.lua"))
            .unwrap();

        assert_eq!(commands.len(), 1);
        let cmd = &commands[0];
        assert_eq!(cmd.name, "summarize");
        assert_eq!(cmd.description, "Summarize notes");
        assert!(!cmd.is_fennel);
    }

    #[test]
    fn test_parse_lua_command_with_name_override() {
        let source = r#"
--- Default description
-- @command name="daily" desc="Create daily note"
function create_daily_note()
    return "Created"
end
"#;

        let parser = AnnotationParser::new();
        let commands = parser
            .parse_commands(source, Path::new("test.lua"))
            .unwrap();

        assert_eq!(commands.len(), 1);
        let cmd = &commands[0];
        assert_eq!(cmd.name, "daily");
        assert_eq!(cmd.description, "Create daily note");
        assert_eq!(cmd.handler_fn, "create_daily_note");
    }

    #[test]
    fn test_parse_lua_command_with_hint() {
        let source = r#"
--- Search notes
-- @command hint="query"
-- @param query string The search query
function search(query)
    return {}
end
"#;

        let parser = AnnotationParser::new();
        let commands = parser
            .parse_commands(source, Path::new("test.lua"))
            .unwrap();

        assert_eq!(commands.len(), 1);
        let cmd = &commands[0];
        assert_eq!(cmd.name, "search");
        assert_eq!(cmd.input_hint, Some("query".to_string()));
        assert_eq!(cmd.params.len(), 1);
        assert_eq!(cmd.params[0].name, "query");
    }

    #[test]
    fn test_parse_fennel_command() {
        let source = r#"
;;; Create a new note
;; @command name="new" hint="title"
(fn new_note [title]
  (create_note title))
"#;

        let parser = AnnotationParser::new();
        let commands = parser
            .parse_commands(source, Path::new("test.fnl"))
            .unwrap();

        assert_eq!(commands.len(), 1);
        let cmd = &commands[0];
        assert_eq!(cmd.name, "new");
        assert_eq!(cmd.description, "Create a new note");
        assert_eq!(cmd.input_hint, Some("title".to_string()));
        assert!(cmd.is_fennel);
    }

    #[test]
    fn test_parse_multiple_commands() {
        let source = r#"
--- First command
-- @command
function cmd_one()
    return 1
end

-- Regular function, not a command
function not_a_command()
    return 0
end

--- Second command
-- @command
function cmd_two()
    return 2
end
"#;

        let parser = AnnotationParser::new();
        let commands = parser
            .parse_commands(source, Path::new("test.lua"))
            .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].name, "cmd_one");
        assert_eq!(commands[1].name, "cmd_two");
    }

    #[test]
    fn test_parse_lua_view_simple() {
        let source = r#"
--- Graph visualization
-- @view
function graph_view(ctx)
    return cru.oil.text("Graph")
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.name, "graph_view");
        assert_eq!(view.description, "Graph visualization");
        assert_eq!(view.view_fn, "graph_view");
        assert!(view.handler_fn.is_none());
        assert!(!view.is_fennel);
    }

    #[test]
    fn test_parse_lua_view_with_name_override() {
        let source = r#"
--- Default description
-- @view name="graph" desc="Knowledge graph view"
function graph_view(ctx)
    return cru.oil.text("Graph")
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.name, "graph");
        assert_eq!(view.description, "Knowledge graph view");
        assert_eq!(view.view_fn, "graph_view");
    }

    #[test]
    fn test_parse_lua_view_with_handler() {
        let source = r#"
--- Graph visualization
-- @view name="graph"
function graph_view(ctx)
    return cru.oil.text("Graph")
end

--- Handle keyboard events
-- @view.handler name="graph"
function graph_keypress(key, ctx)
    if key == "q" then ctx:close_view() end
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.name, "graph");
        assert_eq!(view.handler_fn, Some("graph_keypress".to_string()));
    }

    #[test]
    fn test_parse_fennel_view() {
        let source = r#"
;;; Task list view
;; @view name="tasks" desc="Task list"
(fn tasks_view [ctx]
  (cru.oil.text "Tasks"))
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.fnl")).unwrap();

        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.name, "tasks");
        assert_eq!(view.description, "Task list");
        assert!(view.is_fennel);
    }

    #[test]
    fn test_parse_multiple_views() {
        let source = r#"
--- First view
-- @view
function view_one(ctx)
    return cru.oil.text("One")
end

-- Regular function, not a view
function not_a_view()
    return 0
end

--- Second view
-- @view
function view_two(ctx)
    return cru.oil.text("Two")
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        assert_eq!(views.len(), 2);
        assert_eq!(views[0].name, "view_one");
        assert_eq!(views[1].name, "view_two");
    }

    #[test]
    fn test_view_handler_without_matching_view() {
        let source = r#"
--- Orphan handler (no matching view)
-- @view.handler name="nonexistent"
function orphan_handler(key, ctx)
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        // Handler without view should not create a view
        assert_eq!(views.len(), 0);
    }

    #[test]
    fn test_view_handler_order_independence() {
        // Handler defined before view
        let source = r#"
--- Handle keyboard events
-- @view.handler name="graph"
function graph_keypress(key, ctx)
end

--- Graph visualization
-- @view name="graph"
function graph_view(ctx)
    return cru.oil.text("Graph")
end
"#;

        let parser = AnnotationParser::new();
        let views = parser.parse_views(source, Path::new("test.lua")).unwrap();

        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.name, "graph");
        assert_eq!(view.handler_fn, Some("graph_keypress".to_string()));
    }
}
