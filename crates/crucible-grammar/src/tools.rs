//! Tool definition variants for A/B testing
//!
//! Compares three approaches:
//! 1. **General tools**: Abstract names (Read, Search, List) with smart defaults
//! 2. **Unix tools**: CLI names (cat, rg, fd, ls) - raw, no limits
//! 3. **Enhanced Unix**: CLI names with built-in pagination/limits
//!
//! ## Design Considerations
//!
//! | Aspect | General | Unix Raw | Unix Enhanced |
//! |--------|---------|----------|---------------|
//! | Model familiarity | Low | High | High |
//! | Token efficiency | Medium | High | Medium |
//! | Output safety | High | Low | High |
//! | Schema complexity | High | Low | Medium |

use rmcp::model::{Tool, ToolAnnotations};
use serde_json::json;
use std::borrow::Cow;
use std::sync::Arc;

/// Tool set variants for A/B testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolSetVariant {
    /// Abstract names: Read, Search, List, Find
    General,
    /// Unix names without limits: cat, rg, ls, fd
    UnixRaw,
    /// Unix names with pagination: cat, rg, ls, fd + limit params
    UnixEnhanced,
    /// Full semantic names: Ripgrep, FdFind, Cat, Ls
    SemanticUnix,
}

impl ToolSetVariant {
    pub fn name(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::UnixRaw => "UnixRaw",
            Self::UnixEnhanced => "UnixEnhanced",
            Self::SemanticUnix => "SemanticUnix",
        }
    }
}

// =============================================================================
// GENERAL TOOLS (Claude Code style)
// =============================================================================

/// General tools with abstract names and smart defaults
pub struct GeneralTools;

impl GeneralTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::read(), Self::search(), Self::list(), Self::find()]
    }

    /// Read file contents with line limits
    pub fn read() -> Tool {
        Tool {
            name: Cow::Borrowed("Read"),
            title: Some("Read File".to_string()),
            description: Some(Cow::Borrowed(
                "Read file contents. Returns up to 200 lines by default. Use offset/limit for pagination.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start from (0-indexed). Default: 0"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum lines to return. Default: 200, Max: 1000"
                        }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// Search file contents (grep/rg style)
    pub fn search() -> Tool {
        Tool {
            name: Cow::Borrowed("Search"),
            title: Some("Search Contents".to_string()),
            description: Some(Cow::Borrowed(
                "Search for pattern in files. Returns up to 50 matches by default with context.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern (regex supported)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search. Default: current directory"
                        },
                        "glob": {
                            "type": "string",
                            "description": "File pattern filter (e.g., '*.rs', '*.py')"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum matches to return. Default: 50, Max: 200"
                        },
                        "context": {
                            "type": "integer",
                            "description": "Lines of context around each match. Default: 2"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// List directory contents
    pub fn list() -> Tool {
        Tool {
            name: Cow::Borrowed("List"),
            title: Some("List Directory".to_string()),
            description: Some(Cow::Borrowed(
                "List directory contents. Returns up to 100 entries by default.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path. Default: current directory"
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Include hidden files. Default: false"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum entries to return. Default: 100"
                        }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// Find files by name pattern
    pub fn find() -> Tool {
        Tool {
            name: Cow::Borrowed("Find"),
            title: Some("Find Files".to_string()),
            description: Some(Cow::Borrowed(
                "Find files by name pattern. Returns up to 50 matches by default.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "File name pattern (glob style: *.rs, test_*.py)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in. Default: current directory"
                        },
                        "type": {
                            "type": "string",
                            "enum": ["file", "directory", "any"],
                            "description": "Filter by type. Default: any"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results. Default: 50"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }
}

// =============================================================================
// UNIX RAW TOOLS (no limits)
// =============================================================================

/// Unix tools with familiar names, no built-in limits
pub struct UnixRawTools;

impl UnixRawTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::cat(), Self::rg(), Self::ls(), Self::fd()]
    }

    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("cat"),
            title: Some("cat".to_string()),
            description: Some(Cow::Borrowed("Read and display file contents")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read"
                        }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn rg() -> Tool {
        Tool {
            name: Cow::Borrowed("rg"),
            title: Some("ripgrep".to_string()),
            description: Some(Cow::Borrowed("Search file contents using ripgrep")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern"
                        },
                        "path": {
                            "type": "string",
                            "description": "Path to search"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("ls"),
            title: Some("ls".to_string()),
            description: Some(Cow::Borrowed("List directory contents")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path"
                        }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn fd() -> Tool {
        Tool {
            name: Cow::Borrowed("fd"),
            title: Some("fd-find".to_string()),
            description: Some(Cow::Borrowed("Find files by name pattern")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "File name pattern"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }
}

// =============================================================================
// UNIX ENHANCED TOOLS (with pagination/limits)
// =============================================================================

/// Unix tools with familiar names PLUS smart defaults
pub struct UnixEnhancedTools;

impl UnixEnhancedTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::cat(), Self::rg(), Self::ls(), Self::fd()]
    }

    /// cat with line limits (like head/tail combined)
    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("cat"),
            title: Some("cat".to_string()),
            description: Some(Cow::Borrowed(
                "Read file contents. Returns first 200 lines by default. Use -n for line numbers.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read"
                        },
                        "head": {
                            "type": "integer",
                            "description": "Return first N lines. Default: 200"
                        },
                        "tail": {
                            "type": "integer",
                            "description": "Return last N lines (overrides head)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Skip first N lines before applying head/tail"
                        }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// rg with match limits
    pub fn rg() -> Tool {
        Tool {
            name: Cow::Borrowed("rg"),
            title: Some("ripgrep".to_string()),
            description: Some(Cow::Borrowed(
                "Search file contents. Returns up to 50 matches by default with 2 lines context.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern (regex)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Path to search. Default: ."
                        },
                        "glob": {
                            "type": "string",
                            "description": "File filter (e.g., '*.rs')"
                        },
                        "max_count": {
                            "type": "integer",
                            "description": "Max matches per file. Default: 10"
                        },
                        "max_files": {
                            "type": "integer",
                            "description": "Max files to search. Default: 50"
                        },
                        "context": {
                            "type": "integer",
                            "description": "Lines of context. Default: 2"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// ls with entry limits
    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("ls"),
            title: Some("ls".to_string()),
            description: Some(Cow::Borrowed(
                "List directory. Returns up to 100 entries by default, sorted by name.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path. Default: ."
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Include hidden files"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max entries. Default: 100"
                        }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// fd with result limits
    pub fn fd() -> Tool {
        Tool {
            name: Cow::Borrowed("fd"),
            title: Some("fd-find".to_string()),
            description: Some(Cow::Borrowed(
                "Find files by pattern. Returns up to 50 results by default.",
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "File name pattern (glob: *.rs, test_*)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search. Default: ."
                        },
                        "extension": {
                            "type": "string",
                            "description": "Filter by extension (e.g., 'rs', 'py')"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max results. Default: 50"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }
}

// =============================================================================
// SEMANTIC UNIX TOOLS (Full names for better semantic matching)
// =============================================================================

/// Unix tools with full semantic names (Ripgrep, FdFind, etc.)
pub struct SemanticUnixTools;

impl SemanticUnixTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::cat(), Self::ripgrep(), Self::ls(), Self::fd_find()]
    }

    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("Cat"),
            title: None,
            description: Some(Cow::Borrowed("Read and display file contents")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read"
                        }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn ripgrep() -> Tool {
        Tool {
            name: Cow::Borrowed("Ripgrep"),
            title: None,
            description: Some(Cow::Borrowed("Search for patterns in file contents using ripgrep")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern (regex supported)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("Ls"),
            title: None,
            description: Some(Cow::Borrowed("List directory contents")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory to list"
                        }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn fd_find() -> Tool {
        Tool {
            name: Cow::Borrowed("FdFind"),
            title: None,
            description: Some(Cow::Borrowed("Find files by name pattern recursively using fd")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "File name pattern (regex)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }
}

// =============================================================================
// TOOL SET FACTORY
// =============================================================================

/// Get tools for a specific variant
pub fn get_tools(variant: ToolSetVariant) -> Vec<Tool> {
    match variant {
        ToolSetVariant::General => GeneralTools::all(),
        ToolSetVariant::UnixRaw => UnixRawTools::all(),
        ToolSetVariant::UnixEnhanced => UnixEnhancedTools::all(),
        ToolSetVariant::SemanticUnix => SemanticUnixTools::all(),
    }
}

/// Count tokens in tool definitions (approximate by JSON size)
pub fn estimate_tool_tokens(tools: &[Tool]) -> usize {
    tools
        .iter()
        .map(|t| {
            let name_len = t.name.len();
            let desc_len = t.description.as_ref().map(|d| d.len()).unwrap_or(0);
            let schema_len = serde_json::to_string(&t.input_schema).unwrap_or_default().len();
            // Rough estimate: ~4 chars per token
            (name_len + desc_len + schema_len) / 4
        })
        .sum()
}

// =============================================================================
// PROS/CONS ANALYSIS
// =============================================================================

/// Documented pros/cons for each approach
pub struct ToolAnalysis {
    pub variant: ToolSetVariant,
    pub pros: Vec<&'static str>,
    pub cons: Vec<&'static str>,
    pub best_for: Vec<&'static str>,
}

pub fn analyze_variants() -> Vec<ToolAnalysis> {
    vec![
        ToolAnalysis {
            variant: ToolSetVariant::General,
            pros: vec![
                "Clear, descriptive names",
                "Built-in pagination by default",
                "Token-safe output limits",
                "Consistent interface across operations",
                "Self-documenting parameters",
            ],
            cons: vec![
                "Model must learn new tool names",
                "Larger schema = more prompt tokens",
                "May conflict with model's Unix knowledge",
                "More complex to implement server-side",
            ],
            best_for: vec![
                "New/custom agents",
                "Agents that need strict output limits",
                "Multi-modal contexts",
            ],
        },
        ToolAnalysis {
            variant: ToolSetVariant::UnixRaw,
            pros: vec![
                "Model already knows these tools",
                "Minimal schema = fewer tokens",
                "Direct mapping to actual commands",
                "Predictable behavior from training",
            ],
            cons: vec![
                "No output limits - can blow up context",
                "Model may use wrong flags",
                "Different flags per platform",
                "Raw output needs post-processing",
            ],
            best_for: vec![
                "Quick prototyping",
                "Small file operations",
                "When output size is known/bounded",
            ],
        },
        ToolAnalysis {
            variant: ToolSetVariant::UnixEnhanced,
            pros: vec![
                "Familiar names + safe defaults",
                "Best of both worlds",
                "Model knows 'cat', 'rg' from training",
                "Built-in pagination prevents blowup",
                "Moderate schema complexity",
            ],
            cons: vec![
                "Parameters differ from real CLI flags",
                "May confuse model expecting exact CLI",
                "More implementation complexity",
            ],
            best_for: vec![
                "Production agents",
                "Large codebase exploration",
                "Token-conscious applications",
            ],
        },
    ]
}

// =============================================================================
// SCHEMA DETAIL LEVELS
// =============================================================================

/// How much detail to include in tool schemas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemaDetail {
    /// Just tool name and required params, minimal descriptions
    Minimal,
    /// Standard descriptions (current default)
    Standard,
    /// Rich descriptions with examples and edge case notes
    Detailed,
}

impl SchemaDetail {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Standard => "Standard",
            Self::Detailed => "Detailed",
        }
    }
}

/// Detailed Unix tools with extensive documentation and examples
pub struct DetailedUnixTools;

impl DetailedUnixTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::cat(), Self::rg(), Self::ls(), Self::fd()]
    }

    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("cat"),
            title: Some("cat - Read File".to_string()),
            description: Some(Cow::Borrowed(
                "Read and display file contents. Use for viewing source code, config files, logs, etc.\n\n\
                WHEN TO USE:\n\
                - Reading source files: cat path=\"src/main.rs\"\n\
                - Viewing configs: cat path=\"Cargo.toml\"\n\
                - Checking logs: cat path=\"app.log\" tail=100\n\n\
                WHEN NOT TO USE:\n\
                - Finding files (use fd)\n\
                - Searching content (use rg)\n\
                - Listing directories (use ls)"
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read. Can be relative (./file.txt) or absolute (/etc/hosts). Required."
                        },
                        "head": {
                            "type": "integer",
                            "description": "Return only first N lines. Use for previewing large files. Example: head=50 for first 50 lines. Default: 200"
                        },
                        "tail": {
                            "type": "integer",
                            "description": "Return only last N lines. Useful for log files. Example: tail=100 for recent logs. Overrides head."
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Skip first N lines before applying head/tail. Use for pagination. Example: offset=100, head=50 for lines 100-150."
                        }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn rg() -> Tool {
        Tool {
            name: Cow::Borrowed("rg"),
            title: Some("rg - Search File Contents".to_string()),
            description: Some(Cow::Borrowed(
                "Search for patterns in file contents using ripgrep. Supports regex.\n\n\
                WHEN TO USE:\n\
                - Finding code: rg pattern=\"fn main\" glob=\"*.rs\"\n\
                - Finding TODOs: rg pattern=\"TODO|FIXME\"\n\
                - Finding imports: rg pattern=\"^import|^use\"\n\n\
                WHEN NOT TO USE:\n\
                - Finding files by name (use fd)\n\
                - Reading specific file (use cat)\n\
                - Listing directory (use ls)\n\n\
                EXAMPLES:\n\
                - rg pattern=\"error\" path=\"src/\" - find 'error' in src/\n\
                - rg pattern=\"impl.*Trait\" glob=\"*.rs\" - find trait implementations\n\
                - rg pattern=\"TODO\" context=3 - show 3 lines around TODOs"
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern. Supports regex: 'fn\\s+\\w+' matches function definitions. For literal search, escape special chars. Required."
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search. Default: current directory. Examples: 'src/', './config.toml', '/var/log/'"
                        },
                        "glob": {
                            "type": "string",
                            "description": "Filter files by pattern. Examples: '*.rs' (Rust files), '*.{js,ts}' (JS/TS), 'test_*.py' (Python tests)"
                        },
                        "max_count": {
                            "type": "integer",
                            "description": "Maximum matches per file. Prevents flooding from files with many matches. Default: 10"
                        },
                        "max_files": {
                            "type": "integer",
                            "description": "Maximum files to search. Useful in large codebases. Default: 50"
                        },
                        "context": {
                            "type": "integer",
                            "description": "Lines of context before and after each match. 0 for match only, 2-3 for understanding context. Default: 2"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("ls"),
            title: Some("ls - List Directory".to_string()),
            description: Some(Cow::Borrowed(
                "List files and directories. Use to explore project structure.\n\n\
                WHEN TO USE:\n\
                - Exploring project: ls path=\".\"\n\
                - Checking subdirectory: ls path=\"src/\"\n\
                - Seeing hidden files: ls path=\".\" all=true\n\n\
                WHEN NOT TO USE:\n\
                - Finding specific files (use fd)\n\
                - Reading file contents (use cat)\n\
                - Searching in files (use rg)"
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory to list. Default: current directory. Examples: '.', 'src/', '/home/user/projects'"
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Include hidden files (starting with dot). Set true to see .gitignore, .env, etc. Default: false"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum entries to return. Use for large directories. Default: 100"
                        }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    pub fn fd() -> Tool {
        Tool {
            name: Cow::Borrowed("fd"),
            title: Some("fd - Find Files".to_string()),
            description: Some(Cow::Borrowed(
                "Find files by name pattern recursively. Faster and smarter than 'find'.\n\n\
                WHEN TO USE:\n\
                - Finding Rust files: fd pattern=\".rs$\"\n\
                - Finding tests: fd pattern=\"test\" extension=\"py\"\n\
                - Finding configs: fd pattern=\"config|settings\"\n\n\
                WHEN NOT TO USE:\n\
                - Searching file contents (use rg)\n\
                - Reading a known file (use cat)\n\
                - Listing one directory (use ls)\n\n\
                EXAMPLES:\n\
                - fd pattern=\".rs$\" - all Rust files\n\
                - fd pattern=\"test_\" path=\"tests/\" - test files in tests/\n\
                - fd pattern=\"README\" - find all READMEs"
            )),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "File name pattern (regex). Examples: '\\.rs$' (ends in .rs), '^test_' (starts with test_), 'config' (contains config). Required."
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search recursively. Default: current directory."
                        },
                        "extension": {
                            "type": "string",
                            "description": "Filter by file extension without dot. Examples: 'rs', 'py', 'js'. More specific than pattern."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results to return. Use in large codebases. Default: 50"
                        }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }
}

/// Minimal Unix tools with bare-bones descriptions
pub struct MinimalUnixTools;

impl MinimalUnixTools {
    pub fn all() -> Vec<Tool> {
        vec![Self::cat(), Self::rg(), Self::ls(), Self::fd()]
    }

    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("cat"),
            title: None,
            description: Some(Cow::Borrowed("Read file")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    pub fn rg() -> Tool {
        Tool {
            name: Cow::Borrowed("rg"),
            title: None,
            description: Some(Cow::Borrowed("Search pattern")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "path": { "type": "string" }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("ls"),
            title: None,
            description: Some(Cow::Borrowed("List directory")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": []
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    pub fn fd() -> Tool {
        Tool {
            name: Cow::Borrowed("fd"),
            title: None,
            description: Some(Cow::Borrowed("Find files")),
            input_schema: Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "path": { "type": "string" }
                    },
                    "required": ["pattern"]
                }))
                .unwrap(),
            ),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }
}

/// Get tools by detail level
pub fn get_tools_by_detail(detail: SchemaDetail) -> Vec<Tool> {
    match detail {
        SchemaDetail::Minimal => MinimalUnixTools::all(),
        SchemaDetail::Standard => UnixEnhancedTools::all(),
        SchemaDetail::Detailed => DetailedUnixTools::all(),
    }
}

// =============================================================================
// SYSTEM PROMPT VARIATIONS
// =============================================================================

/// System prompt styles for tool use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemPromptStyle {
    /// Minimal: Just list tools
    Minimal,
    /// Standard: Tools with usage instructions
    Standard,
    /// Detailed: Tools + examples + decision tree
    Detailed,
    /// JSON-focused: Emphasize JSON output format
    JsonFocused,
}

impl SystemPromptStyle {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Standard => "Standard",
            Self::Detailed => "Detailed",
            Self::JsonFocused => "JsonFocused",
        }
    }

    /// Generate system prompt with this style
    pub fn generate(&self, tools: &[Tool]) -> String {
        match self {
            Self::Minimal => self.minimal_prompt(tools),
            Self::Standard => self.standard_prompt(tools),
            Self::Detailed => self.detailed_prompt(tools),
            Self::JsonFocused => self.json_focused_prompt(tools),
        }
    }

    fn minimal_prompt(&self, tools: &[Tool]) -> String {
        let tool_list: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
        format!("Tools: {}\nRespond with JSON tool call.", tool_list.join(", "))
    }

    fn standard_prompt(&self, tools: &[Tool]) -> String {
        let mut prompt = String::from("You have access to the following tools:\n\n");

        for tool in tools {
            prompt.push_str(&format!("## {}\n", tool.name));
            if let Some(desc) = &tool.description {
                prompt.push_str(&format!("{}\n", desc));
            }
            prompt.push_str(&format!(
                "Schema: {}\n\n",
                serde_json::to_string(&tool.input_schema).unwrap_or_default()
            ));
        }

        prompt.push_str("Respond with a JSON tool call: {\"name\": \"tool\", \"arguments\": {...}}");
        prompt
    }

    fn detailed_prompt(&self, tools: &[Tool]) -> String {
        let mut prompt = String::from(
            "# Tool Use Instructions\n\n\
            You are an assistant with access to filesystem tools. Choose the RIGHT tool for each task.\n\n\
            ## Decision Guide\n\n\
            | Task | Tool | Example |\n\
            |------|------|--------|\n\
            | Read a file | cat | {\"name\": \"cat\", \"arguments\": {\"path\": \"README.md\"}} |\n\
            | Search in files | rg | {\"name\": \"rg\", \"arguments\": {\"pattern\": \"TODO\"}} |\n\
            | List directory | ls | {\"name\": \"ls\", \"arguments\": {\"path\": \".\"}} |\n\
            | Find files by name | fd | {\"name\": \"fd\", \"arguments\": {\"pattern\": \".rs$\"}} |\n\n\
            ## Available Tools\n\n"
        );

        for tool in tools {
            prompt.push_str(&format!("### {}\n", tool.name));
            if let Some(desc) = &tool.description {
                prompt.push_str(&format!("{}\n\n", desc));
            }
            prompt.push_str(&format!(
                "**Schema:**\n```json\n{}\n```\n\n",
                serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default()
            ));
        }

        prompt.push_str(
            "## Response Format\n\n\
            ALWAYS respond with valid JSON:\n\
            ```json\n\
            {\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n\
            ```\n\n\
            Do NOT include explanations. Output ONLY the JSON tool call."
        );
        prompt
    }

    fn json_focused_prompt(&self, tools: &[Tool]) -> String {
        let tool_names: Vec<_> = tools.iter().map(|t| format!("\"{}\"", t.name)).collect();

        let mut prompt = format!(
            "OUTPUT FORMAT: You MUST respond with ONLY valid JSON. No text before or after.\n\n\
            VALID TOOL NAMES: {}\n\n\
            RESPONSE SCHEMA:\n\
            {{\n  \"name\": <tool_name>,\n  \"arguments\": {{...}}\n}}\n\n\
            TOOLS:\n\n",
            tool_names.join(" | ")
        );

        for tool in tools {
            prompt.push_str(&format!("{}:\n", tool.name));
            if let Some(desc) = &tool.description {
                // Truncate to first line for brevity
                let first_line = desc.lines().next().unwrap_or("");
                prompt.push_str(&format!("  {}\n", first_line));
            }
            prompt.push_str(&format!(
                "  Args: {}\n\n",
                serde_json::to_string(&tool.input_schema).unwrap_or_default()
            ));
        }

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_four_tools() {
        assert_eq!(GeneralTools::all().len(), 4);
        assert_eq!(UnixRawTools::all().len(), 4);
        assert_eq!(UnixEnhancedTools::all().len(), 4);
    }

    #[test]
    fn test_unix_raw_is_minimal() {
        let raw_tokens = estimate_tool_tokens(&UnixRawTools::all());
        let general_tokens = estimate_tool_tokens(&GeneralTools::all());
        let enhanced_tokens = estimate_tool_tokens(&UnixEnhancedTools::all());

        println!("Token estimates:");
        println!("  UnixRaw: {}", raw_tokens);
        println!("  UnixEnhanced: {}", enhanced_tokens);
        println!("  General: {}", general_tokens);

        // Raw should be smallest
        assert!(raw_tokens < enhanced_tokens);
        assert!(raw_tokens < general_tokens);
    }

    #[test]
    fn test_tool_names() {
        let general = GeneralTools::all();
        assert!(general.iter().any(|t| t.name == "Read"));
        assert!(general.iter().any(|t| t.name == "Search"));

        let unix = UnixRawTools::all();
        assert!(unix.iter().any(|t| t.name == "cat"));
        assert!(unix.iter().any(|t| t.name == "rg"));
    }

    #[test]
    fn test_enhanced_has_limits() {
        let enhanced = UnixEnhancedTools::all();

        // cat should have head/tail params
        let cat = enhanced.iter().find(|t| t.name == "cat").unwrap();
        let schema = &cat.input_schema;
        assert!(schema.get("properties").unwrap().get("head").is_some());

        // rg should have max_count
        let rg = enhanced.iter().find(|t| t.name == "rg").unwrap();
        let schema = &rg.input_schema;
        assert!(schema.get("properties").unwrap().get("max_count").is_some());
    }
}
