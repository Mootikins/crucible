//! TDD Tests for Binary File Detection and Memory Protection (Standalone)
//!
//! This test suite implements Test-Driven Development for critical safety features
//! in the search functionality. These tests are designed to FAIL initially (RED phase)
//! and drive the implementation of binary safety features.
//!
//! Critical Safety Gap Addressed:
//! The current search implementation lacks binary file detection, despite having
//! file size limits. This creates potential security and stability risks when
//! processing binary files that may masquerade as text files.

/// Test harness for binary safety TDD tests
pub struct BinarySafetyTestHarness {
    pub temp_dir: TempDir,
    pub vault_path: PathBuf,
}

impl BinarySafetyTestHarness {
    /// Create a new test harness with temporary directory
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let vault_path = temp_dir.path().join("vault");
        fs::create_dir_all(&vault_path)?;

        Ok(Self {
            temp_dir,
            vault_path,
        })
    }

    /// Create a test file with binary content
    pub fn create_binary_file(&self, relative_path: &str, content: &[u8]) -> Result<String> {
        let full_path = self.vault_path.join(relative_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&full_path, content)?;
        Ok(full_path.to_string_lossy().to_string())
    }

    /// Create a test file with text content
    pub fn create_text_file(&self, relative_path: &str, content: &str) -> Result<String> {
        let full_path = self.vault_path.join(relative_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&full_path, content)?;
        Ok(full_path.to_string_lossy().to_string())
    }

    /// Get the vault path for testing
    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }
}

// ============================================================================
// RED PHASE: Binary File Detection Tests (Expected to FAIL)
// ============================================================================

#[cfg(test)]
mod binary_detection_tests {
    use super::*;

    /// Test detection of PNG files with .md extensions (masquerading)
    #[test]
    fn test_detect_png_file_with_md_extension() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // PNG header bytes (89 50 4E 47 0D 0A 1A 0A) followed by .md extension
        let png_header = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk start
            b'#', b' ', b'T', b'e', b's', b't', b' ', b'I', b'm', b'a', b'g', b'e',
            b'\n', // Fake markdown content
        ];

        let file_path = harness.create_binary_file("test_image.md", &png_header)?;

        // This should FAIL because current implementation doesn't detect binary content
        let result = get_file_content(&file_path);

        // Expected behavior: Should detect binary and return an error
        // Current behavior: Will try to process as text and either fail or return garbage
        assert!(
            result.is_err(),
            "Should detect PNG binary content even with .md extension"
        );

        if let Err(e) = result {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("binary")
                    || error_msg.contains("invalid")
                    || error_msg.contains("utf-8"),
                "Error should mention binary/invalid content: {}",
                error_msg
            );
        }

        Ok(())
    }

    /// Test detection of JPEG files with .md extensions
    #[test]
    fn test_detect_jpeg_file_with_md_extension() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // JPEG header bytes (FF D8 FF E0)
        let jpeg_header = vec![
            0xFF, 0xD8, 0xFF, 0xE0, // JPEG signature
            0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, // JFIF identifier
            b'#', b' ', b'F', b'a', b'k', b'e', b' ', b'D', b'o', b'c',
            b'\n', // Fake markdown
        ];

        let file_path = harness.create_binary_file("document.md", &jpeg_header)?;

        let result = get_file_content(&file_path);
        assert!(
            result.is_err(),
            "Should detect JPEG binary content even with .md extension"
        );

        Ok(())
    }

    /// Test detection of executable files with text extensions
    #[test]
    fn test_detect_executable_with_text_extension() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // PE header (Windows executable)
        let pe_header = vec![
            b'M', b'Z', // DOS header
            0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, // More DOS header
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Zeros
            b'#', b' ', b'B', b'i', b'n', b'a', b'r', b'y', b' ', b'E', b'x', b'e',
            b'\n', // Fake text
        ];

        let file_path = harness.create_binary_file("readme.txt", &pe_header)?;

        let result = get_file_content(&file_path);
        assert!(
            result.is_err(),
            "Should detect executable binary content even with .txt extension"
        );

        Ok(())
    }

    /// Test detection of ELF executables (Linux)
    #[test]
    fn test_detect_elf_executable() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // ELF header (Linux executable)
        let elf_header = vec![
            0x7F, b'E', b'L', b'F', // ELF magic number
            0x02, 0x01, 0x01, 0x00, // 64-bit, little endian, current version
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Padding
            b'#', b' ', b'L', b'i', b'n', b'u', b'x', b' ', b'B', b'i', b'n',
            b'\n', // Fake markdown
        ];

        let file_path = harness.create_binary_file("linux_binary.md", &elf_header)?;

        let result = get_file_content(&file_path);
        assert!(result.is_err(), "Should detect ELF binary content");

        Ok(())
    }

    /// Test detection of files with null bytes (common binary indicator)
    #[test]
    fn test_detect_null_bytes_in_file() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Content with null bytes mixed with text
        let content_with_nulls = b"# Title\nSome text content\x00\x00\x00More text\x00End\n";

        let file_path = harness.create_binary_file("mixed_content.md", content_with_nulls)?;

        let result = get_file_content(&file_path);
        assert!(
            result.is_err(),
            "Should detect null bytes and reject as binary"
        );

        if let Err(e) = result {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("null")
                    || error_msg.contains("binary")
                    || error_msg.contains("invalid"),
                "Error should mention null bytes or binary content: {}",
                error_msg
            );
        }

        Ok(())
    }

    /// Test detection of ZIP archives with .md extensions
    #[test]
    fn test_detect_zip_file_with_md_extension() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // ZIP header (PK 03 04)
        let zip_header = vec![
            b'P', b'K', 0x03, 0x04, // Local file header signature
            0x14, 0x00, 0x00, 0x00, // Version
            0x08, 0x00, 0x00, 0x00, // Flags and compression
            b'#', b' ', b'Z', b'i', b'p', b' ', b'A', b'r', b'c', b'h', b'i', b'v', b'e',
            b'\n', // Fake content
        ];

        let file_path = harness.create_binary_file("archive.md", &zip_header)?;

        let result = get_file_content(&file_path);
        assert!(result.is_err(), "Should detect ZIP binary content");

        Ok(())
    }

    /// Test detection of PDF files with .md extensions
    #[test]
    fn test_detect_pdf_file_with_md_extension() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // PDF header (%PDF-)
        let pdf_header =
            b"%PDF-1.4\n1 0 obj\n<<\n/Type /Catalog\n>>\nendobj\n# Fake markdown content\n";

        let file_path = harness.create_binary_file("document.md", pdf_header)?;

        let result = get_file_content(&file_path);
        assert!(result.is_err(), "Should detect PDF binary content");

        Ok(())
    }

    /// Test detection of mixed binary and text content files
    #[test]
    fn test_detect_mixed_binary_text_content() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Start with text, then insert binary content
        let mixed_content = b"# Real Title\nThis is real markdown content.\n\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\nMore content here.\n";

        let file_path = harness.create_binary_file("mixed.md", mixed_content)?;

        let result = get_file_content(&file_path);
        assert!(
            result.is_err(),
            "Should detect binary content mixed with text"
        );

        Ok(())
    }

    /// Test search functionality properly skips binary files
    #[test]
    fn test_search_skips_binary_files() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create legitimate markdown file
        harness.create_text_file(
            "legitimate.md",
            "# Legitimate Document\nThis contains searchable content.",
        )?;

        // Create binary file with .md extension
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        harness.create_binary_file("binary.md", &png_header)?;

        // Search should find legitimate content but skip binary
        let results = search_files_in_kiln(harness.vault_path(), "searchable", 10, false)?;

        assert!(!results.is_empty(), "Should find legitimate content");

        // Verify all results are from legitimate text files
        for result in results {
            assert!(
                !result.id.contains("binary.md"),
                "Should not include binary files in results"
            );
        }

        Ok(())
    }
}

// ============================================================================
// RED PHASE: Memory Protection Tests (Expected to FAIL)
// ============================================================================

#[cfg(test)]
mod memory_protection_tests {
    use super::*;

    /// Test large file handling at 10MB boundary
    #[test]
    fn test_large_file_boundary_handling() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create file exactly at 10MB limit with valid UTF-8 content
        let mut content = String::new();
        let line = "# Large Document\nThis is line content. ".repeat(100); // ~2KB per line

        // Calculate how many lines to reach ~10MB
        let target_size = 10 * 1024 * 1024; // 10MB
        let lines_needed = target_size / line.len();

        for i in 0..lines_needed {
            content.push_str(&format!("Line {}: {}\n", i, line));
        }

        let file_path = harness.create_text_file("large_boundary.md", &content)?;

        let result = get_file_content(&file_path);

        // Current implementation might try to load this entirely into memory
        // Should either succeed with content truncation or fail gracefully
        match result {
            Ok(content) => {
                // If it succeeds, content should be truncated to memory limit
                assert!(
                    content.len() <= 1024 * 1024,
                    "Content should be truncated to 1MB memory limit"
                );
            }
            Err(e) => {
                // If it fails, error should be informative
                let error_msg = e.to_string().to_lowercase();
                assert!(
                    error_msg.contains("large")
                        || error_msg.contains("memory")
                        || error_msg.contains("limit"),
                    "Error should mention size/memory limit: {}",
                    error_msg
                );
            }
        }

        Ok(())
    }

    /// Test file slightly over 10MB limit
    #[test]
    fn test_file_over_size_limit() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create file slightly over 10MB limit
        let mut content = String::new();
        let line = "A".repeat(1000); // 1KB line

        // Create 11MB of content (over the 10MB limit)
        for _ in 0..(11 * 1024) {
            content.push_str(&line);
            content.push('\n');
        }

        let file_path = harness.create_text_file("oversized.md", &content)?;

        let result = get_file_content(&file_path);
        assert!(result.is_err(), "Should reject files over 10MB limit");

        if let Err(e) = result {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("large")
                    || error_msg.contains("size")
                    || error_msg.contains("limit"),
                "Error should mention file size limit: {}",
                error_msg
            );
        }

        Ok(())
    }

    /// Test memory usage when processing binary content
    #[test]
    fn test_memory_usage_with_binary_content() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create binary content that would be memory-intensive to process
        let large_binary_content = vec![0xFF; 5 * 1024 * 1024]; // 5MB of binary data

        let file_path = harness.create_binary_file("large_binary.md", &large_binary_content)?;

        let result = get_file_content(&file_path);
        assert!(
            result.is_err(),
            "Should reject large binary content to protect memory"
        );

        if let Err(e) = result {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("binary")
                    || error_msg.contains("invalid")
                    || error_msg.contains("utf-8"),
                "Error should mention binary/invalid content: {}",
                error_msg
            );
        }

        Ok(())
    }
}

// ============================================================================
// RED PHASE: File Size Boundary Tests (Expected to FAIL)
// ============================================================================

#[cfg(test)]
mod file_size_boundary_tests {
    use super::*;

    /// Test content truncation at 1MB processing limit
    #[test]
    fn test_content_truncation_boundary() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create file larger than 1MB but under 10MB
        let mut content = String::new();
        let target_size = 2 * 1024 * 1024; // 2MB

        while content.len() < target_size {
            content.push_str("# Large Document\nThis is content that should be truncated. ");
            content.push_str(&"Repeated text block. ".repeat(100));
            content.push('\n');
        }

        let file_path = harness.create_text_file("truncation_test.md", &content)?;

        let result = get_file_content(&file_path)?;

        // Content should be truncated to prevent memory issues
        assert!(
            result.len() <= 1024 * 1024,
            "Content should be truncated to 1MB or less"
        );
        assert!(
            !result.is_empty(),
            "Content should not be empty after truncation"
        );

        Ok(())
    }

    /// Test file exactly at 10MB limit
    #[test]
    fn test_file_exactly_at_limit() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create file exactly at 10MB
        let mut content = String::new();
        let target_size = 10 * 1024 * 1024;
        let chunk = "# Boundary Test\nContent chunk. ".repeat(1000);

        while content.len() + chunk.len() <= target_size {
            content.push_str(&chunk);
            content.push('\n');
        }

        let file_path = harness.create_text_file("exact_limit.md", &content)?;

        // Check file size
        let metadata = fs::metadata(&file_path)?;
        let file_size = metadata.len();
        assert!(
            file_size <= 10 * 1024 * 1024,
            "File should be at most 10MB, but was {} bytes",
            file_size
        );
        assert!(
            file_size > 9 * 1024 * 1024,
            "File should be close to 10MB, but was {} bytes",
            file_size
        );

        let result = get_file_content(&file_path);

        // Should handle the boundary case properly
        match result {
            Ok(_) => {
                // Should succeed with content truncation
            }
            Err(e) => {
                let error_msg = e.to_string().to_lowercase();
                assert!(
                    error_msg.contains("large") || error_msg.contains("memory"),
                    "Should handle boundary gracefully: {}",
                    error_msg
                );
            }
        }

        Ok(())
    }

    /// Test file one byte over limit
    #[test]
    fn test_file_one_byte_over_limit() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create file 1 byte over 10MB limit
        let base_size = 10 * 1024 * 1024;
        let content = "A".repeat(base_size + 1);

        let file_path = harness.create_text_file("one_byte_over.md", &content)?;

        let result = get_file_content(&file_path);
        assert!(result.is_err(), "Should reject file even 1 byte over limit");

        Ok(())
    }

    /// Test empty file handling
    #[test]
    fn test_empty_file_handling() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        let file_path = harness.create_text_file("empty.md", "")?;

        let result = get_file_content(&file_path)?;
        assert_eq!(result, "", "Empty file should return empty string");

        Ok(())
    }

    /// Test file with only whitespace
    #[test]
    fn test_whitespace_only_file() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        let content = "   \n\t\n   \n  \n";
        let file_path = harness.create_text_file("whitespace.md", content)?;

        let result = get_file_content(&file_path)?;
        assert_eq!(result, content, "Whitespace-only file should be preserved");

        Ok(())
    }
}

// ============================================================================
// RED PHASE: Integration Tests (Expected to FAIL)
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test search with mixed binary and text files
    #[test]
    fn test_search_mixed_binary_text_files() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create legitimate text files
        harness.create_text_file("doc1.md", "# Document 1\nContent with search term alpha")?;
        harness.create_text_file("doc2.md", "# Document 2\nContent with search term beta")?;
        harness.create_text_file("doc3.md", "# Document 3\nMore alpha content here")?;

        // Create binary files with .md extensions
        harness.create_binary_file("binary1.md", &[0x89, 0x50, 0x4E, 0x47])?;
        harness.create_binary_file("binary2.md", &[0xFF, 0xD8, 0xFF, 0xE0])?;
        harness.create_binary_file("binary3.md", &[0x00, 0x00, 0x00, 0x00])?;

        // Search should find text files but skip binary files
        let alpha_results = search_files_in_kiln(harness.vault_path(), "alpha", 10, false)?;
        let beta_results = search_files_in_kiln(harness.vault_path(), "beta", 10, false)?;

        assert_eq!(alpha_results.len(), 2, "Should find 2 files with 'alpha'");
        assert_eq!(beta_results.len(), 1, "Should find 1 file with 'beta'");

        // Verify no binary files in results
        let all_paths: Vec<String> = alpha_results
            .iter()
            .chain(beta_results.iter())
            .map(|r| r.id.clone())
            .collect();

        for path in all_paths {
            assert!(
                !path.contains("binary"),
                "Results should not include binary files: {}",
                path
            );
        }

        Ok(())
    }

    /// Test search continues after encountering binary files
    #[test]
    fn test_search_continues_after_binary_files() -> Result<()> {
        let harness = BinarySafetyTestHarness::new()?;

        // Create binary file first
        harness.create_binary_file("first_binary.md", &[0x89, 0x50, 0x4E, 0x47])?;

        // Create text files after
        harness.create_text_file("after_binary1.md", "# After Binary 1\nSearchable content")?;
        harness.create_text_file(
            "after_binary2.md",
            "# After Binary 2\nMore searchable content",
        )?;

        // Another binary file
        harness.create_binary_file("second_binary.md", &[0xFF, 0xD8, 0xFF, 0xE0])?;

        // More text files
        harness.create_text_file("final_text.md", "# Final Text\nLast searchable content")?;

        // Search should process all text files despite binary files
        let results = search_files_in_kiln(harness.vault_path(), "searchable", 10, false)?;

        assert_eq!(
            results.len(),
            3,
            "Should find all 3 text files with searchable content"
        );

        Ok(())
    }
}
use anyhow::Result;
use crucible_cli::commands::search::{get_file_content, search_files_in_kiln};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
