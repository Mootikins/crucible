use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;
use serde_json;
use tabled::{Table, Tabled, settings::Style};

use crate::config::CliConfig;
use crate::output;
use crucible_core::storage::builder::{ContentAddressedStorageBuilder, StorageBackendType, HasherConfig};
use crucible_core::storage::{ContentAddressedStorage, ContentHasher, MerkleTree, StorageResult, EnhancedTreeChange};
use crucible_core::storage::diff::EnhancedChangeDetector;
use crucible_core::hashing::blake3::Blake3Hasher;
use crucible_core::storage::HashedBlock;
use std::sync::Arc;

/// Output formats for diff command
#[derive(Debug, Clone)]
pub enum DiffOutputFormat {
    Plain,
    Json,
    Detailed,
}

impl From<String> for DiffOutputFormat {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "json" => DiffOutputFormat::Json,
            "detailed" => DiffOutputFormat::Detailed,
            _ => DiffOutputFormat::Plain,
        }
    }
}

/// Table-friendly representation of diff changes
#[derive(Tabled)]
struct ChangeRow {
    #[tabled(rename = "Type")]
    change_type: String,
    #[tabled(rename = "Index")]
    index: usize,
    #[tabled(rename = "Hash")]
    hash: String,
    #[tabled(rename = "Similarity")]
    similarity: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(rename = "Timestamp")]
    timestamp: String,
}

/// Execute diff command
pub async fn execute(
    config: CliConfig,
    path1: PathBuf,
    path2: PathBuf,
    format: String,
    show_similarity: bool,
    show_unchanged: bool,
    max_depth: usize,
) -> Result<()> {
    let output_format: DiffOutputFormat = format.into();
    let start_time = Instant::now();

    // Validate input paths
    if !path1.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", path1.display()));
    }
    if !path2.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", path2.display()));
    }

    println!("📊 Comparing {} and {}",
        path1.display(), path2.display());

    // Create storage backend
    let storage = create_storage_backend(&config)?;
    let storage = Arc::new(storage);

    // Simple processing approach using the storage backend directly

    // Process first path
    println!("🔍 Analyzing: {}", path1.display());
    let tree1 = process_path(&storage, &path1, max_depth)
        .await
        .context("Failed to process first path")?;

    // Process second path
    println!("🔍 Analyzing: {}", path2.display());
    let tree2 = process_path(&storage, &path2, max_depth)
        .await
        .context("Failed to process second path")?;

    // Perform diff analysis
    output::info("Detecting changes...");
    let detector = EnhancedChangeDetector::new();
    let hasher = Blake3Hasher::new();
    let changes = detector.compare_trees(&tree1, &tree2, &hasher, crucible_core::storage::diff::ChangeSource::UserEdit)
        .context("Failed to detect changes")?;

    // Generate output
    match output_format {
        DiffOutputFormat::Plain => output_plain_format(&changes, show_unchanged),
        DiffOutputFormat::Json => output_json_format(&changes, &tree1, &tree2, show_similarity)?,
        DiffOutputFormat::Detailed => output_detailed_format(&changes, show_similarity, show_unchanged)?,
    }

    // Show summary
    let duration = start_time.elapsed();
    println!("✅ Diff completed in {:.2}s - {} changes detected",
        duration.as_secs_f32(),
        changes.len()
    );

    Ok(())
}

/// Create storage backend based on configuration
fn create_storage_backend(_config: &CliConfig) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
    let backend = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(crucible_core::storage::BlockSize::Medium)
        .build()?;

    Ok(backend)
}

/// Process a path (file or directory) and return a Merkle tree
async fn process_path(
    _storage: &Arc<dyn ContentAddressedStorage>,
    path: &Path,
    _max_depth: usize,
) -> Result<MerkleTree> {
    // Simple implementation - create a mock Merkle tree based on file content
    if path.is_file() {
        let content = std::fs::read_to_string(path)?;
        let hasher = Blake3Hasher::new();
        let hash = hasher.hash_block(content.as_bytes());

        // Create a simple hashed block for the file content
        let block = HashedBlock {
            hash,
            data: content.clone().into_bytes(),
            length: content.len(),
            index: 0,
            offset: 0,
            is_last: true,
        };

        // Create a Merkle tree from the block
        let tree = MerkleTree::from_blocks(&[block], &hasher)?;
        Ok(tree)
    } else if path.is_dir() {
        // For directories, create a hash based on directory structure
        let entries: Vec<_> = std::fs::read_dir(path)?
            .take(10) // Limit for now
            .collect();

        let dir_content = format!("directory_with_{}_entries", entries.len());
        let hasher = Blake3Hasher::new();
        let hash = hasher.hash_block(dir_content.as_bytes());

        let block = HashedBlock {
            hash,
            data: dir_content.clone().into_bytes(),
            length: dir_content.len(),
            index: 0,
            offset: 0,
            is_last: true,
        };

        let tree = MerkleTree::from_blocks(&[block], &hasher)?;
        Ok(tree)
    } else {
        Err(anyhow::anyhow!("Path is neither file nor directory: {}", path.display()))
    }
}

/// Output in plain text format
fn output_plain_format(changes: &[EnhancedTreeChange], show_unchanged: bool) {
    if changes.is_empty() {
        println!("✅ No changes detected");
        return;
    }

    output::header("Changes detected:");

    for change in changes {
        match change {
            EnhancedTreeChange::AddedBlock { index, hash, .. } => {
                output::success(&format!("+ Added block #{}: {}", index, hash[..8].to_string()));
            }
            EnhancedTreeChange::DeletedBlock { index, hash, .. } => {
                output::error(&format!("- Deleted block #{}: {}", index, hash[..8].to_string()));
            }
            EnhancedTreeChange::ModifiedBlock { index, old_hash, new_hash, similarity_score, .. } => {
                output::warning(&format!(
                    "~ Modified block #{}: {} -> {} ({}% similar)",
                    index,
                    &old_hash[..8],
                    &new_hash[..8],
                    (similarity_score * 100.0) as u32
                ));
            }
            EnhancedTreeChange::MovedBlock { old_index, new_index, hash, .. } => {
                output::info(&format!(
                    "↔ Moved block #{} → #{}: {}",
                    old_index,
                    new_index,
                    &hash[..8]
                ));
            }
            _ => {}
        }
    }
}

/// Output in JSON format
fn output_json_format(
    changes: &[EnhancedTreeChange],
    tree1: &MerkleTree,
    tree2: &MerkleTree,
    show_similarity: bool,
) -> Result<()> {
    let output = serde_json::json!({
        "metadata": {
            "tree1_hash": tree1.root_hash,
            "tree2_hash": tree2.root_hash,
            "total_changes": changes.len(),
            "timestamp": chrono::Utc::now().to_rfc3339()
        },
        "changes": changes.iter().map(|change| {
            match change {
                EnhancedTreeChange::AddedBlock { index, hash, metadata } => {
                    serde_json::json!({
                        "type": "added",
                        "index": index,
                        "hash": hash,
                        "size": 0, // Size not available in ChangeMetadata
                        "timestamp": metadata.timestamp
                    })
                }
                EnhancedTreeChange::DeletedBlock { index, hash, metadata } => {
                    serde_json::json!({
                        "type": "deleted",
                        "index": index,
                        "hash": hash,
                        "size": 0, // Size not available in ChangeMetadata
                        "timestamp": metadata.timestamp
                    })
                }
                EnhancedTreeChange::ModifiedBlock { index, old_hash, new_hash, similarity_score, metadata } => {
                    let mut obj = serde_json::json!({
                        "type": "modified",
                        "index": index,
                        "old_hash": old_hash,
                        "new_hash": new_hash,
                        "size": 0, // Size not available in ChangeMetadata
                        "timestamp": metadata.timestamp
                    });
                    if show_similarity {
                        obj["similarity_score"] = serde_json::json!(similarity_score);
                    }
                    obj
                }
                EnhancedTreeChange::MovedBlock { old_index, new_index, hash, metadata } => {
                    let obj = serde_json::json!({
                        "type": "moved",
                        "old_index": old_index,
                        "new_index": new_index,
                        "hash": hash,
                        "timestamp": metadata.timestamp
                    });
                    obj
                }
                _ => serde_json::json!({"type": "unknown"}),
            }
        }).collect::<Vec<_>>()
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output in detailed table format
fn output_detailed_format(
    changes: &[EnhancedTreeChange],
    show_similarity: bool,
    show_unchanged: bool,
) -> Result<()> {
    if changes.is_empty() {
        println!("✅ No changes detected");
        return Ok(());
    }

    let rows: Vec<ChangeRow> = changes.iter().map(|change| {
        match change {
            EnhancedTreeChange::AddedBlock { index, hash, metadata } => ChangeRow {
                change_type: "Added".to_string(),
                index: *index,
                hash: hash[..8].to_string(),
                similarity: "-".to_string(),
                size: "-".to_string(), // Size not available in ChangeMetadata
                timestamp: format_timestamp(metadata.timestamp),
            },
            EnhancedTreeChange::DeletedBlock { index, hash, metadata } => ChangeRow {
                change_type: "Deleted".to_string(),
                index: *index,
                hash: hash[..8].to_string(),
                similarity: "-".to_string(),
                size: "-".to_string(), // Size not available in ChangeMetadata
                timestamp: format_timestamp(metadata.timestamp),
            },
            EnhancedTreeChange::ModifiedBlock { index, old_hash, new_hash, similarity_score, metadata } => {
                let similarity_str = if show_similarity {
                    format!("{:.1}%", similarity_score * 100.0)
                } else {
                    "-".to_string()
                };

                ChangeRow {
                    change_type: "Modified".to_string(),
                    index: *index,
                    hash: format!("{} → {}", &old_hash[..8], &new_hash[..8]),
                    similarity: similarity_str,
                    size: "-".to_string(), // Size not available in ChangeMetadata
                    timestamp: format_timestamp(metadata.timestamp),
                }
            },
            EnhancedTreeChange::MovedBlock { old_index, new_index, hash, metadata } => {
                ChangeRow {
                    change_type: "Moved".to_string(),
                    index: *old_index,
                    hash: format!("{} → #{}", &hash[..8], new_index),
                    similarity: "-".to_string(),
                    size: "-".to_string(),
                    timestamp: format_timestamp(metadata.timestamp),
                }
            },
            _ => ChangeRow {
                change_type: "Unknown".to_string(),
                index: 0,
                hash: "-".to_string(),
                similarity: "-".to_string(),
                size: "-".to_string(),
                timestamp: "-".to_string(),
            },
        }
    }).collect();

    let table = Table::new(&rows)
        .with(Style::modern())
        .to_string();

    output::header("Detailed Changes:");
    println!("{}", table);

    // Show change summary
    let added = changes.iter().filter(|c| matches!(c, EnhancedTreeChange::AddedBlock { .. })).count();
    let deleted = changes.iter().filter(|c| matches!(c, EnhancedTreeChange::DeletedBlock { .. })).count();
    let modified = changes.iter().filter(|c| matches!(c, EnhancedTreeChange::ModifiedBlock { .. })).count();
    let moved = changes.iter().filter(|c| matches!(c, EnhancedTreeChange::MovedBlock { .. })).count();

    println!("\n📊 Summary:");
    println!("  Added: {} blocks", added);
    println!("  Deleted: {} blocks", deleted);
    println!("  Modified: {} blocks", modified);
    println!("  Moved: {} blocks", moved);
    println!("  Total: {} changes", changes.len());

    Ok(())
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

/// Format timestamp into human readable format
fn format_timestamp(timestamp: u64) -> String {
    if timestamp == 0 {
        "-".to_string()
    } else {
        let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
            .unwrap_or_default();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}