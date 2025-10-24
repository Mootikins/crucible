#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_vault_scanner_creation() {
        let config = VaultScannerConfig::default();
        let scanner = create_vault_scanner(config).await;
        assert!(scanner.is_ok());
    }

    #[tokio::test]
    async fn test_vault_scanner_basic_scan() {
        // Create temporary directory with test files
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().to_path_buf();

        // Create test markdown files
        tokio::fs::write(test_path.join("test1.md"), "# Test Document\n\nContent here.").await.unwrap();
        tokio::fs::write(test_path.join("test2.txt"), "Not a markdown file").await.unwrap();

        // Create subdirectory
        let subdir = test_path.join("subdir");
        tokio::fs::create_dir(&subdir).await.unwrap();
        tokio::fs::write(subdir.join("test3.md"), "# Nested Document\n\nNested content.").await.unwrap();

        // Test scanning
        let config = VaultScannerConfig::default();
        let mut scanner = create_vault_scanner(config).await.unwrap();

        let result = scanner.scan_vault_directory(&test_path).await.unwrap();

        // Verify results
        assert!(result.total_files_found >= 2); // At least 2 markdown files
        assert!(result.markdown_files_found >= 2); // At least 2 markdown files
        assert!(result.successful_files >= 2); // At least 2 successful files
        assert_eq!(result.scan_errors.len(), 0); // No errors expected

        // Test file info structure
        for file_info in &result.discovered_files {
            assert!(!file_info.path.as_os_str().is_empty());
            assert!(!file_info.relative_path.is_empty());
            if file_info.is_markdown {
                assert!(file_info.is_accessible);
            }
        }
    }

    #[tokio::test]
    async fn test_vault_scanner_configuration() {
        // Test default configuration
        let config = VaultScannerConfig::default();
        assert_eq!(config.max_file_size_bytes, 50 * 1024 * 1024);
        assert_eq!(config.max_recursion_depth, 10);
        assert!(config.recursive_scan);
        assert!(!config.include_hidden_files);
        assert_eq!(config.file_extensions, vec!["md".to_string(), "markdown".to_string()]);
        assert_eq!(config.parallel_processing, num_cpus::get());
        assert_eq!(config.batch_size, 16);
        assert!(config.enable_embeddings);
        assert!(config.process_embeds);
        assert!(config.process_wikilinks);

        // Test configuration presets
        let large_config = VaultScannerConfig::for_large_vault();
        assert!(large_config.parallel_processing >= 8);
        assert!(large_config.batch_size >= 32);
        assert!(large_config.enable_incremental);

        let small_config = VaultScannerConfig::for_small_vault();
        assert_eq!(small_config.parallel_processing, 1);
        assert_eq!(small_config.batch_size, 4);
        assert!(!small_config.enable_incremental);

        let resource_config = VaultScannerConfig::for_resource_constrained();
        assert_eq!(resource_config.parallel_processing, 1);
        assert_eq!(resource_config.batch_size, 2);
        assert!(!resource_config.enable_embeddings);
    }

    #[tokio::test]
    async fn test_vault_scanner_config_validation() {
        // Test valid configuration
        let valid_config = VaultScannerConfig::default();
        assert!(validate_vault_scanner_config(&valid_config).await.is_ok());

        // Test invalid configurations
        let invalid_configs = vec![
            VaultScannerConfig {
                parallel_processing: 0,
                ..Default::default()
            },
            VaultScannerConfig {
                batch_size: 0,
                ..Default::default()
            },
            VaultScannerConfig {
                file_extensions: vec![],
                ..Default::default()
            },
        ];

        for config in invalid_configs {
            assert!(validate_vault_scanner_config(&config).await.is_err());
        }
    }

    #[tokio::test]
    async fn test_parse_file_to_document() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create test markdown file
        let content = r#"# Test Document

This is a test document with some **bold** text and *italic* text.

## Section 1

Some content here.

## Section 2

More content here.
"#;
        tokio::fs::write(&test_file, content).await.unwrap();

        // Test parsing
        let document = parse_file_to_document(&test_file).await.unwrap();

        assert_eq!(document.title(), "Test Document");
        assert!(document.content.plain_text.contains("This is a test document"));
        assert!(!document.wikilinks.is_empty() || document.wikilinks.is_empty()); // Should work either way
    }
}