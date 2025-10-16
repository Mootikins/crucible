/// Enhanced tool discovery for flexible organization patterns
///
/// This module implements Phase 1 of the enhanced architecture:
/// - Support for simple direct tools (current approach)
/// - Support for organized tools in modules
/// - Flexible naming conventions
/// - Consumer awareness without restrictions

use super::{RuneTool, ToolMetadata, RuneAstAnalyzer, DiscoveredModule, AsyncFunctionInfo};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Discovery result containing all tools found in a file
#[derive(Debug, Clone)]
pub struct DiscoveredTools {
    pub file_path: PathBuf,
    pub tools: Vec<DiscoveredTool>,
    pub metadata: FileMetadata,
}

/// Individual discovered tool
#[derive(Debug, Clone)]
pub struct DiscoveredTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub entry_point: ToolEntryPoint,
    pub metadata: ToolMetadata,
    pub consumer_info: ConsumerInfo,
}

/// How to invoke the tool
#[derive(Debug, Clone)]
pub enum ToolEntryPoint {
    /// Direct function call (simple tools)
    Direct { function_name: String },
    /// Module-based function call (organized tools)
    Module { module_path: Vec<String>, function_name: String },
}

/// File-level metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub naming_convention: Option<String>,
    pub file_info: HashMap<String, Value>,
}

/// Consumer information for tool
#[derive(Debug, Clone)]
pub struct ConsumerInfo {
    pub primary_consumers: Vec<String>,
    pub secondary_consumers: Vec<String>,
    pub ui_hints: HashMap<String, Value>,
    pub agent_hints: HashMap<String, Value>,
}

/// Enhanced tool discovery engine
pub struct ToolDiscovery {
    context: Arc<rune::Context>,
}

impl ToolDiscovery {
    /// Create a new discovery engine
    pub fn new(context: Arc<rune::Context>) -> Self {
        Self { context }
    }

    /// Discover tools in a directory
    pub async fn discover_in_directory(&self, tool_dir: &Path) -> Result<Vec<DiscoveredTools>> {
        info!("Starting tool discovery in directory: {:?}", tool_dir);

        let mut all_discoveries = Vec::new();

        if !tool_dir.exists() {
            warn!("Tool directory does not exist: {:?}", tool_dir);
            return Ok(all_discoveries);
        }

        let mut entries = std::fs::read_dir(tool_dir)
            .with_context(|| format!("Failed to read tool directory: {:?}", tool_dir))?;

        while let Some(entry) = entries.next() {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("rn") {
                match self.discover_in_file(&path).await {
                    Ok(discovery) => {
                        info!("Discovered {} tools in {:?}", discovery.tools.len(), path);
                        all_discoveries.push(discovery);
                    }
                    Err(e) => {
                        warn!("Failed to discover tools in {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Discovery complete. Found {} files with tools", all_discoveries.len());
        Ok(all_discoveries)
    }

    /// Discover tools in a single file
    pub async fn discover_in_file(&self, file_path: &Path) -> Result<DiscoveredTools> {
        debug!("Discovering tools in file: {:?}", file_path);

        let source_code = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        // Compile the file to analyze its structure
        let unit = self.compile_file(&source_code, file_path)?;

        // Discover tools using both patterns
        let mut tools = Vec::new();

        // 1. Discover simple direct tools (backwards compatibility)
        tools.extend(self.discover_direct_tools(unit.clone(), file_path)?);

        // 2. Discover organized tools in modules
        tools.extend(self.discover_module_tools(unit.clone(), file_path)?);

        // 3. Extract file metadata
        let metadata = self.extract_file_metadata(&unit, &source_code)?;

        Ok(DiscoveredTools {
            file_path: file_path.to_path_buf(),
            tools,
            metadata,
        })
    }

    /// Compile a Rune file for analysis
    fn compile_file(&self, source_code: &str, file_path: &Path) -> Result<Arc<rune::Unit>> {
        let source = rune::Source::memory(source_code)?;

        let mut sources = rune::Sources::new();
        sources.insert(source)?;

        let mut diagnostics = rune::Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        // Report any warnings/errors but don't fail on warnings
        if !diagnostics.is_empty() {
            let mut writer = rune::termcolor::StandardStream::stderr(rune::termcolor::ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result.with_context(|| "Failed to compile Rune unit")?;
        Ok(Arc::new(unit))
    }

    /// Discover simple direct tools (backwards compatibility)
    fn discover_direct_tools(&self, unit: Arc<rune::Unit>, file_path: &Path) -> Result<Vec<DiscoveredTool>> {
        let mut tools = Vec::new();

        // For now, we'll use a simple approach: try to call the expected functions
        // This is the same approach used in RuneTool::from_source

        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit);

        // Try to extract NAME
        let tool_name = match vm.call(["NAME"], ()) {
            Ok(value) => {
                let name: String = rune::from_value(value)
                    .with_context(|| "Failed to convert NAME to String")?;
                debug!("Found tool name: {}", name);
                name
            }
            Err(_) => {
                debug!("No NAME function found, using default naming");
                // Use filename as tool name
                file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown_tool")
                    .to_string()
            }
        };

        // Try to extract DESCRIPTION
        let description = match vm.call(["DESCRIPTION"], ()) {
            Ok(value) => {
                let desc: String = rune::from_value(value)
                    .with_context(|| "Failed to convert DESCRIPTION to String")?;
                desc
            }
            Err(_) => format!("Tool from {:?}", file_path),
        };

        // Try to extract INPUT_SCHEMA
        let input_schema = match vm.call(["INPUT_SCHEMA"], ()) {
            Ok(value) => {
                super::tool::rune_value_to_json(&value)
                    .with_context(|| "Failed to convert INPUT_SCHEMA to JSON")?
            }
            Err(_) => {
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                })
            }
        };

        // Create discovered tool
        let tool = DiscoveredTool {
            name: tool_name.clone(),
            description: description.clone(),
            input_schema: input_schema.clone(),
            entry_point: ToolEntryPoint::Direct {
                function_name: "call".to_string(),
            },
            metadata: ToolMetadata {
                name: tool_name.clone(),
                description: description.clone(),
                input_schema: input_schema.clone(),
                output_schema: None,
            },
            consumer_info: ConsumerInfo::default(),
        };

        tools.push(tool);
        Ok(tools)
    }

    /// Discover organized tools in modules using AST analysis
    fn discover_module_tools(&self, unit: Arc<rune::Unit>, _file_path: &Path) -> Result<Vec<DiscoveredTool>> {
        let mut tools = Vec::new();

        // Use the AST analyzer to discover modules and their functions
        let analyzer = RuneAstAnalyzer::new();
        let discovered_modules = analyzer.analyze_modules(&unit)?;

        for module in discovered_modules {
            for function in module.functions {
                // Convert discovered module function to DiscoveredTool
                let tool = DiscoveredTool {
                    name: format!("{}.{}", module.name, function.name),
                    description: analyzer.extract_function_description(
                        &unit,
                        &module.path,
                        &function.name
                    ).unwrap_or_else(|_| format!("{} function in {} module", function.name, module.name)),

                    input_schema: analyzer.analyze_function_parameters(
                        &unit,
                        &module.path,
                        &function.name
                    ).unwrap_or_else(|_| serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    })),

                    entry_point: ToolEntryPoint::Module {
                        module_path: module.path.clone(),
                        function_name: function.name.clone(),
                    },

                    metadata: ToolMetadata {
                        name: format!("{}.{}", module.name, function.name),
                        description: format!("{} function from {} module", function.name, module.name),
                        input_schema: serde_json::json!({
                            "type": "object",
                            "properties": {},
                            "required": []
                        }),
                        output_schema: None,
                    },

                    consumer_info: analyzer.extract_consumer_info(
                        &unit,
                        &module.path,
                        &function.name
                    ).unwrap_or_default(),
                };

                tools.push(tool);
            }
        }

        debug!("Discovered {} module-based tools", tools.len());
        Ok(tools)
    }

    /// Extract file metadata from the compiled unit
    fn extract_file_metadata(&self, _unit: &rune::Unit, source_code: &str) -> Result<FileMetadata> {
        let mut metadata = FileMetadata {
            naming_convention: None,
            file_info: HashMap::new(),
        };

        // Look for naming convention constant in source code
        if source_code.contains("TOOL_NAMING_CONVENTION") {
            // Simple string parsing for Phase 1
            if source_code.contains("\"semantic\"") {
                metadata.naming_convention = Some("semantic".to_string());
            } else if source_code.contains("\"topic_module_function\"") {
                metadata.naming_convention = Some("topic_module_function".to_string());
            }
        }

        // Look for metadata function in source code
        if source_code.contains("get_tool_metadata") {
            metadata.file_info.insert("has_metadata".to_string(), Value::Bool(true));
        }

        Ok(metadata)
    }
}

impl Default for ConsumerInfo {
    fn default() -> Self {
        Self {
            primary_consumers: vec!["agents".to_string(), "ui".to_string()],
            secondary_consumers: vec![],
            ui_hints: HashMap::new(),
            agent_hints: HashMap::new(),
        }
    }
}

/// Convert discovered tools to RuneTool instances
pub fn convert_to_rune_tools(discoveries: Vec<DiscoveredTools>, context: &rune::Context) -> Result<Vec<RuneTool>> {
    let mut rune_tools = Vec::new();

    for discovery in discoveries {
        for discovered_tool in discovery.tools {
            // For Phase 1, we'll use the existing RuneTool::from_source method
            // This maintains backwards compatibility while we collect enhanced metadata

            debug!("Converting discovered tool: {}", discovered_tool.name);

            // Read the source file to create the RuneTool
            let source_code = std::fs::read_to_string(&discovery.file_path)
                .with_context(|| format!("Failed to read source file: {:?}", discovery.file_path))?;

            let rune_tool = RuneTool::from_source(&source_code, context)?;
            rune_tools.push(rune_tool);
        }
    }

    Ok(rune_tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_enhanced_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        // Create a test tool file with both simple and organized tools
        let tool_source = r#"
            // Simple direct tool
            pub async fn search_files(args) {
                #{ success: true, results: ["file1.md", "file2.md"] }
            }

            // Organized tools
            pub mod file {
                pub async fn list_by_pattern(args) {
                    #{ success: true, results: ["pattern1", "pattern2"] }
                }
            }

            pub mod ui_helpers {
                pub async fn get_suggestions(args) {
                    #{ success: true, suggestions: ["suggestion1", "suggestion2"] }
                }
            }
        "#;

        let tool_path = tool_dir.join("test.rn");
        fs::write(&tool_path, tool_source).unwrap();

        let context = Arc::new(rune::Context::with_default_modules().unwrap());
        let discovery = ToolDiscovery::new(context);

        let discoveries = discovery.discover_in_directory(&tool_dir).await.unwrap();

        assert_eq!(discoveries.len(), 1);
        let discovered = &discoveries[0];
        assert!(discovered.tools.len() > 0);
    }

    #[test]
    fn test_consumer_info_default() {
        let consumer_info = ConsumerInfo::default();
        assert!(consumer_info.primary_consumers.contains(&"agents".to_string()));
        assert!(consumer_info.primary_consumers.contains(&"ui".to_string()));
    }
}