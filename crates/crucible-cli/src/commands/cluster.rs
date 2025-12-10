use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use crucible_tools::ClusteringTools;
use serde_json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

/// Clustering configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub kiln_path: PathBuf,
    pub algorithm: String,
    pub min_similarity: f64,
    pub min_cluster_size: usize,
    pub min_moc_score: f64,
    pub output_format: OutputFormat,
    pub output_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Json,
    Table,
    Summary,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            kiln_path: PathBuf::from("."),
            algorithm: "heuristic".to_string(),
            min_similarity: 0.2,
            min_cluster_size: 2,
            min_moc_score: 0.5,
            output_format: OutputFormat::Summary,
            output_file: None,
        }
    }
}

/// Service abstraction for clustering operations
#[async_trait]
pub trait ClusteringService: Send + Sync {
    async fn detect_mocs(&self, config: &ClusterConfig) -> Result<serde_json::Value>;
    async fn cluster_documents(&self, config: &ClusterConfig) -> Result<serde_json::Value>;
    async fn get_statistics(&self, config: &ClusterConfig) -> Result<serde_json::Value>;
}

/// Default implementation using ClusteringTools
pub struct DefaultClusteringService {
    tools: ClusteringTools,
}

impl DefaultClusteringService {
    pub fn new(kiln_path: PathBuf) -> Self {
        Self {
            tools: ClusteringTools::new(kiln_path),
        }
    }
}

#[async_trait]
impl ClusteringService for DefaultClusteringService {
    async fn detect_mocs(&self, config: &ClusterConfig) -> Result<serde_json::Value> {
        let mocs = self.tools.detect_mocs(Some(config.min_moc_score)).await?;
        let json = serde_json::to_value(&mocs)?;
        Ok(json)
    }

    async fn cluster_documents(&self, config: &ClusterConfig) -> Result<serde_json::Value> {
        let clusters = self.tools
            .cluster_documents(
                Some(config.min_similarity),
                Some(config.min_cluster_size),
                Some(0.6), // link_weight
                Some(0.3), // tag_weight
                Some(0.1), // title_weight
            )
            .await?;
        let json = serde_json::to_value(&clusters)?;
        Ok(json)
    }

    async fn get_statistics(&self, config: &ClusterConfig) -> Result<serde_json::Value> {
        let stats = self.tools.get_document_stats().await?;
        let json = serde_json::to_value(&stats)?;
        Ok(json)
    }
}

/// Execute clustering command
pub async fn execute(
    action: ClusterAction,
    algorithm: String,
    min_similarity: f64,
    min_cluster_size: usize,
    min_moc_score: f64,
    output_format: String,
    output_file: Option<PathBuf>,
    config: CliConfig,
) -> Result<()> {
    // Build clustering configuration
    let cluster_config = ClusterConfig {
        kiln_path: config.kiln_path.clone(),
        algorithm,
        min_similarity,
        min_cluster_size,
        min_moc_score,
        output_format: match output_format.as_str() {
            "json" => OutputFormat::Json,
            "table" => OutputFormat::Table,
            _ => OutputFormat::Summary,
        },
        output_file,
    };

    // Validate kiln path exists
    if !cluster_config.kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", cluster_config.kiln_path.display());
        return Err(anyhow!("kiln path does not exist"));
    }

    // Create clustering service
    let service: Arc<dyn ClusteringService> = Arc::new(DefaultClusteringService::new(
        cluster_config.kiln_path.clone(),
    ));

    // Execute action
    match action {
        ClusterAction::Mocs => {
            let result = service.detect_mocs(&cluster_config).await?;
            format_output(&result, &cluster_config.output_format, None);
            if let Some(output_path) = &cluster_config.output_file {
                let json = serde_json::to_string_pretty(&result)?;
                fs::write(output_path, json).await?;
                println!("✅ Results saved to: {}", output_path.display());
            }
        }
        ClusterAction::Documents => {
            let result = service.cluster_documents(&cluster_config).await?;
            format_output(&result, &cluster_config.output_format, None);
            if let Some(output_path) = &cluster_config.output_file {
                let json = serde_json::to_string_pretty(&result)?;
                fs::write(output_path, json).await?;
                println!("✅ Results saved to: {}", output_path.display());
            }
        }
        ClusterAction::Statistics => {
            let result = service.get_statistics(&cluster_config).await?;
            format_output(&result, &cluster_config.output_format, None);
            if let Some(output_path) = &cluster_config.output_file {
                let json = serde_json::to_string_pretty(&result)?;
                fs::write(output_path, json).await?;
                println!("✅ Results saved to: {}", output_path.display());
            }
        }
        ClusterAction::All => {
            // Run all clustering operations
            println!("🔍 Detecting Maps of Content...\n");
            let mocs = service.detect_mocs(&cluster_config).await?;
            format_output(&mocs, &cluster_config.output_format, Some("Maps of Content"));

            println!("\n📊 Clustering documents...\n");
            let clusters = service.cluster_documents(&cluster_config).await?;
            format_output(&clusters, &cluster_config.output_format, Some("Document Clusters"));

            println!("\n📈 Gathering statistics...\n");
            let stats = service.get_statistics(&cluster_config).await?;
            format_output(&stats, &cluster_config.output_format, Some("Knowledge Base Statistics"));
        }
    }

    Ok(())
}


/// Format output based on format type
pub fn format_output(
    data: &serde_json::Value,
    format: &OutputFormat,
    title: Option<&str>,
) {
    if let Some(t) = title {
        println!("## {}\n", t);
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
        }
        OutputFormat::Table => {
            format_as_table(data);
        }
        OutputFormat::Summary => {
            format_as_summary(data);
        }
    }
}

/// Format data as a readable table
fn format_as_table(data: &serde_json::Value) {
    if let Some(array) = data.as_array() {
        for (i, item) in array.iter().enumerate() {
            println!("{}. {}", i + 1, format_item_summary(item));
        }
    } else {
        println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
    }
}

/// Format data as a summary
fn format_as_summary(data: &serde_json::Value) {
    if let Some(array) = data.as_array() {
        println!("Found {} items\n", array.len());

        // Show first few items in detail
        for (i, item) in array.iter().take(5).enumerate() {
            println!("{}. {}", i + 1, format_item_summary(item));
        }

        if array.len() > 5 {
            println!("... and {} more items", array.len() - 5);
        }
    } else if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            println!("{}: {}", key, format_value_summary(value));
        }
    }
}

/// Format a single item as a summary line
fn format_item_summary(item: &serde_json::Value) -> String {
    if let Some(obj) = item.as_object() {
        let title = obj.get("title")
            .and_then(|v| v.as_str())
            .or_else(|| obj.get("path").and_then(|v| v.as_str()))
            .unwrap_or("<unnamed>");

        let score = obj.get("score")
            .and_then(|v| v.as_f64())
            .map(|s| format!(" (score: {:.2})", s))
            .unwrap_or_default();

        format!("{}{}", title, score)
    } else {
        serde_json::to_string(item).unwrap_or_default()
    }
}

/// Format a value for summary display
fn format_value_summary(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Array(a) => format!("array[{}]", a.len()),
        serde_json::Value::Object(o) => format!("object{{{}}}", o.len()),
        _ => "<null>".to_string(),
    }
}

#[derive(Debug, Clone)]
pub enum ClusterAction {
    Mocs,
    Documents,
    Statistics,
    All,
}