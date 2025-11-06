use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::sync::Arc;
use serde_json;
use tabled::{Table, Tabled, settings::Style};

use crate::config::CliConfig;
use crate::output;
use crucible_core::storage::builder::{ContentAddressedStorageBuilder, StorageBackendType, HasherConfig};
use crucible_core::storage::{ContentAddressedStorage, MerkleTree, HashedBlock, StorageResult, ContentHasher};
use crucible_core::parser::{StorageAwareParser, PulldownParser};

/// Output formats for parse command
#[derive(Debug, Clone)]
pub enum ParseOutputFormat {
    Plain,
    Json,
    Detailed,
}

impl From<String> for ParseOutputFormat {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "json" => ParseOutputFormat::Json,
            "detailed" => ParseOutputFormat::Detailed,
            _ => ParseOutputFormat::Plain,
        }
    }
}

/// Table-friendly block information
#[derive(Tabled)]
struct BlockRow {
    #[tabled(rename = "Index")]
    index: usize,
    #[tabled(rename = "Type")]
    block_type: String,
    #[tabled(rename = "Hash")]
    hash: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(rename = "Preview")]
    preview: String,
}

/// Table-friendly tree information
#[derive(Tabled)]
struct TreeRow {
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Blocks")]
    block_count: usize,
    #[tabled(rename = "Root Hash")]
    root_hash: String,
    #[tabled(rename = "Size")]
    size: String,
}

/// Parse result container
#[derive(Debug)]
struct ParseResult {
    path: String,
    tree: Option<MerkleTree>,
    blocks: Vec<HashedBlock>,
    parse_time: std::time::Duration,
    error: Option<String>,
}

/// Execute parse command
pub async fn execute(
    config: CliConfig,
    path: PathBuf,
    format: String,
    show_tree: bool,
    show_blocks: bool,
    max_depth: usize,
    continue_on_error: bool,
) -> Result<()> {
    let output_format: ParseOutputFormat = format.into();
    let start_time = Instant::now();

    // Validate input path
    if !path.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
    }

    output::info(&format!("Parsing: {}", path.display()));

    // Create storage backend
    let storage = create_storage_backend(&config)?;
    let storage = Arc::new(storage);

    // Create parser
    let block_parser = PulldownParser::new();
    let parser = StorageAwareParser::new(
        Box::new(block_parser),
    );

    // Process the path
    let results = if path.is_file() {
        vec![parse_file(&parser, &path).await]
    } else if path.is_dir() {
        parse_directory(&parser, &path, max_depth, continue_on_error).await?
    } else {
        return Err(anyhow::anyhow!("Path is neither file nor directory: {}", path.display()));
    };

    // Generate output
    match output_format {
        ParseOutputFormat::Plain => output_plain_format(&results, show_tree, show_blocks),
        ParseOutputFormat::Json => output_json_format(&results, show_tree, show_blocks)?,
        ParseOutputFormat::Detailed => output_detailed_format(&results, show_tree, show_blocks)?,
    }

    // Show summary
    let duration = start_time.elapsed();
    let total_files: usize = results.iter().filter(|r| r.error.is_none()).count();
    let total_blocks: usize = results.iter().map(|r| r.blocks.len()).sum();
    let total_errors: usize = results.iter().filter(|r| r.error.is_some()).count();

    output::success(&format!(
        "Parse completed in {:.2}s - {} files, {} blocks, {} errors",
        duration.as_secs_f32(),
        total_files,
        total_blocks,
        total_errors
    ));

    Ok(())
}

/// Create storage backend based on configuration
fn create_storage_backend(_config: &CliConfig) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
    let backend = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(crucible_core::hashing::blake3::Blake3Hasher::new()))
        .with_block_size(crucible_core::storage::BlockSize::Medium)
        .build()?;

    Ok(backend)
}

/// Parse a single file
async fn parse_file(
    parser: &StorageAwareParser,
    path: &Path,
) -> ParseResult {
    let parse_start = Instant::now();

    // Simple implementation - read file and create basic structure
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let hasher = crucible_core::hashing::blake3::Blake3Hasher::new();
            let hash = hasher.hash_block(content.as_bytes());

            let content_len = content.len();
            let block = HashedBlock {
                hash,
                data: content.into_bytes(),
                length: content_len,
                index: 0,
                offset: 0,
                is_last: true,
            };

            let tree = match MerkleTree::from_blocks(&[block.clone()], &hasher) {
                Ok(tree) => Some(tree),
                Err(_) => None,
            };

            ParseResult {
                path: path.to_string_lossy().to_string(),
                tree,
                blocks: vec![block],
                parse_time: parse_start.elapsed(),
                error: None,
            }
        },
        Err(e) => ParseResult {
            path: path.to_string_lossy().to_string(),
            tree: None,
            blocks: vec![],
            parse_time: parse_start.elapsed(),
            error: Some(e.to_string()),
        },
    }
}

/// Parse a directory recursively
async fn parse_directory(
    parser: &StorageAwareParser,
    path: &Path,
    max_depth: usize,
    continue_on_error: bool,
) -> Result<Vec<ParseResult>> {
    let mut results = Vec::new();
    let mut walk_stack = vec![(path.to_path_buf(), 0)];

    while let Some((current_path, depth)) = walk_stack.pop() {
        if depth > max_depth {
            continue;
        }

        let entries = match std::fs::read_dir(&current_path) {
            Ok(entries) => entries,
            Err(e) => {
                results.push(ParseResult {
                    path: current_path.to_string_lossy().to_string(),
                    tree: None,
                    blocks: vec![],
                    parse_time: std::time::Duration::from_secs(0),
                    error: Some(format!("Failed to read directory: {}", e)),
                });
                if !continue_on_error {
                    return Err(anyhow::anyhow!("Directory read error: {}", e));
                }
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    if continue_on_error {
                        continue;
                    } else {
                        return Err(anyhow::anyhow!("Directory entry error: {}", e));
                    }
                }
            };

            let entry_path = entry.path();

            if entry_path.is_file() {
                // Check if it's a markdown file
                if let Some(extension) = entry_path.extension() {
                    if extension == "md" || extension == "markdown" {
                        results.push(parse_file(parser, &entry_path).await);
                    }
                }
            } else if entry_path.is_dir() {
                walk_stack.push((entry_path, depth + 1));
            }
        }
    }

    Ok(results)
}

/// Output in plain text format
fn output_plain_format(results: &[ParseResult], show_tree: bool, show_blocks: bool) {
    for result in results {
        if let Some(error) = &result.error {
            output::error(&format!("Error parsing {}: {}", result.path, error));
            continue;
        }

        output::header(&result.path);

        if show_tree {
            if let Some(tree) = &result.tree {
                println!("Root Hash: {}", tree.root_hash);
                println!("Node Count: {}", tree.nodes.len());
                println!("Tree Depth: {}", tree.depth);
                println!("Block Count: {}", tree.block_count);
            } else {
                println!("No tree data available");
            }
        }

        if show_blocks {
            println!("Blocks: {}", result.blocks.len());
            for (i, block) in result.blocks.iter().enumerate() {
                println!("  {}: {} ({})", i, block.hash, format_bytes(block.data.len() as u64));
            }
        }

        if !show_tree && !show_blocks {
            println!("Successfully parsed with {} blocks", result.blocks.len());
        }
        println!();
    }
}

/// Output in JSON format
fn output_json_format(
    results: &[ParseResult],
    show_tree: bool,
    show_blocks: bool,
) -> Result<()> {
    let output = serde_json::json!({
        "metadata": {
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "total_files": results.len(),
            "total_blocks": results.iter().map(|r| r.blocks.len()).sum::<usize>(),
            "show_tree": show_tree,
            "show_blocks": show_blocks,
        },
        "results": results.iter().map(|result| {
            serde_json::json!({
                "path": result.path,
                "parse_time_ms": result.parse_time.as_millis(),
                "error": result.error,
                "tree": result.tree.as_ref().map(|tree| {
                    serde_json::json!({
                        "root_hash": tree.root_hash,
                        "node_count": tree.block_count,
                        "depth": tree.depth,
                    })
                }),
                "blocks": if show_blocks {
                    result.blocks.iter().enumerate().map(|(i, block)| {
                        serde_json::json!({
                            "index": i,
                            "hash": block.hash,
                            "size": block.data.len(),
                            "length": block.length,
                        })
                    }).collect::<Vec<_>>()
                } else {
                    vec![]  // Return empty Vec<Value> instead of Value::Array
                },
                "block_count": result.blocks.len(),
            })
        }).collect::<Vec<_>>()
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output in detailed table format
fn output_detailed_format(
    results: &[ParseResult],
    show_tree: bool,
    show_blocks: bool,
) -> Result<()> {
    // Summary table
    let summary_rows: Vec<TreeRow> = results.iter()
        .filter(|r| r.error.is_none())
        .map(|result| {
            let (root_hash, size) = if let Some(tree) = &result.tree {
                (tree.root_hash.clone(), format_bytes(result.blocks.iter().map(|b| b.data.len() as u64).sum()))
            } else {
                ("N/A".to_string(), format_bytes(result.blocks.iter().map(|b| b.data.len() as u64).sum()))
            };

            TreeRow {
                path: result.path.clone(),
                block_count: result.blocks.len(),
                root_hash,
                size,
            }
        })
        .collect();

    if !summary_rows.is_empty() {
        output::header("Parse Summary");
        let table = Table::new(&summary_rows)
            .with(Style::modern())
            .to_string();
        println!("{}", table);
        println!();
    }

    // Detailed tree information
    if show_tree {
        output::header("Merkle Tree Details");
        for result in results {
            if let Some(error) = &result.error {
                output::error(&format!("{}: {}", result.path, error));
                continue;
            }

            if let Some(tree) = &result.tree {
                println!("\n📁 {}", result.path);
                println!("   Root Hash: {}", tree.root_hash);
                println!("   Node Count: {}", tree.block_count);
                println!("   Tree Depth: {}", tree.depth);
                println!("   Parse Time: {:.2}ms", result.parse_time.as_millis());
            }
        }
        println!();
    }

    // Detailed block information
    if show_blocks {
        output::header("Block Details");
        for result in results {
            if let Some(error) = &result.error {
                output::error(&format!("{}: {}", result.path, error));
                continue;
            }

            if !result.blocks.is_empty() {
                println!("\n📄 {} ({} blocks)", result.path, result.blocks.len());

                let block_rows: Vec<BlockRow> = result.blocks.iter().enumerate().map(|(i, block)| {
                    BlockRow {
                        index: i,
                        block_type: "Content".to_string(), // TODO: Determine actual block type
                        hash: block.hash[..12].to_string(),
                        size: format_bytes(block.data.len() as u64),
                        preview: get_block_preview(&block.data),
                    }
                }).collect();

                let table = Table::new(&block_rows)
                    .with(Style::modern())
                    .to_string();
                println!("{}", table);
            }
        }
    }

    // Errors
    let errors: Vec<_> = results.iter().filter(|r| r.error.is_some()).collect();
    if !errors.is_empty() {
        output::header("Errors");
        for result in errors {
            output::error(&format!("{}: {}", result.path, result.error.as_ref().unwrap()));
        }
    }

    Ok(())
}

/// Get a preview of block content
fn get_block_preview(data: &[u8]) -> String {
    let preview = String::from_utf8_lossy(data);
    let lines: Vec<&str> = preview.lines().take(3).collect();
    let preview = lines.join(" ");

    if preview.len() > 50 {
        format!("{}...", &preview[..50])
    } else {
        preview.to_string()
    }
}

/// Format bytes into human readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}