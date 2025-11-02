use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Summary statistics for a kiln directory.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct KilnStats {
    pub total_files: u64,
    pub markdown_files: u64,
    pub total_size_bytes: u64,
}

/// Abstraction over the source of kiln statistics so tests can stub results.
pub trait KilnStatsService: Send + Sync {
    fn collect(&self, kiln_path: &Path) -> Result<KilnStats>;
}

/// Filesystem-backed implementation that mirrors the previous behaviour.
#[derive(Default)]
pub struct FileSystemKilnStatsService;

impl FileSystemKilnStatsService {
    /// Recursively collect statistics from a directory and all subdirectories
    fn collect_recursive(&self, path: &Path, stats: &mut KilnStats) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(path)?;

        for entry in entries.flatten() {
            let entry_path = entry.path();

            if entry_path.is_file() {
                // Count the file
                stats.total_files += 1;

                // Add file size
                if let Ok(metadata) = entry.metadata() {
                    stats.total_size_bytes = stats.total_size_bytes.saturating_add(metadata.len());
                }

                // Check if it's a markdown file
                if entry_path
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
                {
                    stats.markdown_files += 1;
                }
            } else if entry_path.is_dir() {
                // Recursively process subdirectory
                self.collect_recursive(&entry_path, stats)?;
            }
        }

        Ok(())
    }
}

impl KilnStatsService for FileSystemKilnStatsService {
    fn collect(&self, kiln_path: &Path) -> Result<KilnStats> {
        let mut totals = KilnStats::default();

        if !kiln_path.is_dir() {
            return Ok(totals);
        }

        // Use recursive helper to walk all subdirectories
        self.collect_recursive(kiln_path, &mut totals)?;

        Ok(totals)
    }
}

pub async fn execute(config: CliConfig) -> Result<()> {
    let service: Arc<dyn KilnStatsService> = Arc::new(FileSystemKilnStatsService);
    execute_with_service(service, config).await
}

pub async fn execute_with_service(
    service: Arc<dyn KilnStatsService>,
    config: CliConfig,
) -> Result<()> {
    let kiln_path = &config.kiln.path;

    if !kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", kiln_path.display());
        eprintln!("Please configure kiln.path in your config file (see: cru config show)");
        return Err(anyhow!("kiln path does not exist"));
    }

    let stats = service.collect(kiln_path)?;

    println!("📊 Kiln Statistics\n");
    println!("📁 Total files: {}", stats.total_files);
    println!("📝 Markdown files: {}", stats.markdown_files);
    println!("💾 Total size: {} KB", stats.total_size_bytes / 1024);
    println!("🗂️  Kiln path: {}", kiln_path.display());
    println!("\n✅ Kiln scan completed successfully.");

    Ok(())
}
