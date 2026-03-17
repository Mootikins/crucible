use crate::config::CliConfig;
use crate::formatting::OutputFormat;
use anyhow::{anyhow, Result};
use crucible_core::EXCLUDED_DIRS;
use serde::Serialize;
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

/// Output format for stats (JSON serializable)
#[derive(Debug, Serialize)]
pub struct StatsOutput {
    pub file_count: u64,
    pub markdown_count: u64,
    pub size_bytes: u64,
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
                // Skip excluded directories
                let dir_name = entry_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if EXCLUDED_DIRS.contains(&dir_name) {
                    continue;
                }

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

pub async fn execute(config: CliConfig, format: &str) -> Result<()> {
    let service: Arc<dyn KilnStatsService> = Arc::new(FileSystemKilnStatsService);
    execute_with_service(service, config, format).await
}

pub async fn execute_with_service(
    service: Arc<dyn KilnStatsService>,
    config: CliConfig,
    format: &str,
) -> Result<()> {
    let kiln_path = &config.kiln_path;

    if !kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", kiln_path.display());
        eprintln!("Please configure kiln.path in your config file (see: cru config show)");
        return Err(anyhow!("kiln path does not exist"));
    }

    let stats = service.collect(kiln_path)?;
    let output_format = OutputFormat::from(format);

    match output_format {
        OutputFormat::Json => {
            let output = StatsOutput {
                file_count: stats.total_files,
                markdown_count: stats.markdown_files,
                size_bytes: stats.total_size_bytes,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            println!("📊 Kiln Statistics\n");
            println!("📁 Total files: {}", stats.total_files);
            println!("📝 Markdown files: {}", stats.markdown_files);
            println!("💾 Total size: {} KB", stats.total_size_bytes / 1024);
            println!("🗂️  Kiln path: {}", kiln_path.display());
            println!("\n✅ Kiln scan completed successfully.");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct MockStatsService {
        stats: KilnStats,
    }

    impl KilnStatsService for MockStatsService {
        fn collect(&self, _kiln_path: &Path) -> Result<KilnStats> {
            Ok(self.stats.clone())
        }
    }

    struct ErrorStatsService;

    impl KilnStatsService for ErrorStatsService {
        fn collect(&self, _kiln_path: &Path) -> Result<KilnStats> {
            Err(anyhow!("Mock error"))
        }
    }

    #[test]
    fn test_kiln_stats_default() {
        let stats = KilnStats::default();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.markdown_files, 0);
        assert_eq!(stats.total_size_bytes, 0);
    }

    #[test]
    fn test_kiln_stats_equality() {
        let stats1 = KilnStats {
            total_files: 10,
            markdown_files: 5,
            total_size_bytes: 1024,
        };
        let stats2 = KilnStats {
            total_files: 10,
            markdown_files: 5,
            total_size_bytes: 1024,
        };
        assert_eq!(stats1, stats2);
    }

    #[test]
    fn test_filesystem_service_empty_dir() {
        let temp = TempDir::new().unwrap();
        let service = FileSystemKilnStatsService;
        let stats = service.collect(temp.path()).unwrap();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.markdown_files, 0);
    }

    #[test]
    fn test_filesystem_service_with_markdown_files() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("note1.md"), "# Note 1").unwrap();
        std::fs::write(temp.path().join("note2.md"), "# Note 2").unwrap();
        std::fs::write(temp.path().join("readme.txt"), "readme").unwrap();

        let service = FileSystemKilnStatsService;
        let stats = service.collect(temp.path()).unwrap();

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.markdown_files, 2);
        assert!(stats.total_size_bytes > 0);
    }

    #[test]
    fn test_filesystem_service_recursive() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        std::fs::write(temp.path().join("root.md"), "# Root").unwrap();
        std::fs::write(subdir.join("nested.md"), "# Nested").unwrap();

        let service = FileSystemKilnStatsService;
        let stats = service.collect(temp.path()).unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.markdown_files, 2);
    }

    #[test]
    fn test_filesystem_service_excludes_directories() {
        let temp = TempDir::new().unwrap();

        // Create excluded directories with markdown files
        std::fs::create_dir(temp.path().join(".crucible")).unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        std::fs::create_dir(temp.path().join("node_modules")).unwrap();

        // Create markdown files in excluded directories (should be ignored)
        std::fs::write(temp.path().join(".crucible/note.md"), "# Note").unwrap();
        std::fs::write(temp.path().join(".git/config.md"), "# Config").unwrap();
        std::fs::write(temp.path().join("node_modules/pkg.md"), "# Package").unwrap();

        // Create markdown file in root (should be counted)
        std::fs::write(temp.path().join("root.md"), "# Root").unwrap();

        let service = FileSystemKilnStatsService;
        let stats = service.collect(temp.path()).unwrap();

        // Should only count the root.md file, not the ones in excluded directories
        assert_eq!(
            stats.total_files, 1,
            "Expected 1 file (root.md), but got {}",
            stats.total_files
        );
        assert_eq!(
            stats.markdown_files, 1,
            "Expected 1 markdown file, but got {}",
            stats.markdown_files
        );
    }

    #[test]
    fn test_filesystem_service_nonexistent_path() {
        let service = FileSystemKilnStatsService;
        let stats = service.collect(Path::new("/nonexistent/path")).unwrap();
        assert_eq!(stats.total_files, 0);
    }

    #[tokio::test]
    async fn test_execute_with_mock_service() {
        let temp = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: temp.path().to_path_buf(),
            ..Default::default()
        };

        let mock = MockStatsService {
            stats: KilnStats {
                total_files: 100,
                markdown_files: 50,
                total_size_bytes: 1024 * 1024,
            },
        };

        let result = execute_with_service(Arc::new(mock), config, "table").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_kiln_path() {
        let config = CliConfig {
            kiln_path: PathBuf::from("/nonexistent/kiln/path"),
            ..Default::default()
        };

        let mock = MockStatsService {
            stats: KilnStats::default(),
        };

        let result = execute_with_service(Arc::new(mock), config, "table").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_service_error() {
        let temp = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: temp.path().to_path_buf(),
            ..Default::default()
        };

        let result = execute_with_service(Arc::new(ErrorStatsService), config, "table").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_stats_output_json_serialization() {
        let output = StatsOutput {
            file_count: 42,
            markdown_count: 10,
            size_bytes: 1024 * 100,
        };

        let json_str = serde_json::to_string(&output).expect("Failed to serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("Failed to parse JSON");

        assert_eq!(parsed["file_count"], 42);
        assert_eq!(parsed["markdown_count"], 10);
        assert_eq!(parsed["size_bytes"], 1024 * 100);
    }
}
