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

impl KilnStatsService for FileSystemKilnStatsService {
    fn collect(&self, kiln_path: &Path) -> Result<KilnStats> {
        let mut totals = KilnStats::default();

        if !kiln_path.is_dir() {
            return Ok(totals);
        }

        if let Ok(entries) = fs::read_dir(kiln_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    totals.total_files += 1;

                    if let Ok(metadata) = entry.metadata() {
                        totals.total_size_bytes =
                            totals.total_size_bytes.saturating_add(metadata.len());
                    }

                    if path
                        .extension()
                        .map(|ext| ext.eq_ignore_ascii_case("md"))
                        .unwrap_or(false)
                    {
                        totals.markdown_files += 1;
                    }
                }
            }
        }

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
