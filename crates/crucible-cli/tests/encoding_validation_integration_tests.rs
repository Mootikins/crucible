//! Encoding Validation Integration Tests
//!
//! Test various encoding scenarios to ensure robust UTF-8 handling:
//! - Valid UTF-8 with Unicode characters (emoji, accented characters)
//! - Invalid UTF-8 sequences
//! - Mixed encoding content
//! - UTF-8 BOM handling
//! - Partial UTF-8 sequences at file boundaries
//! - Binary files masquerading as text

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use unicode_normalization::UnicodeNormalization;
use crucible_cli::commands::search::is_binary_content;
use crucible_cli::commands::secure_filesystem::{PathValidator, SecureFileReader, SecureFileSystemConfig};

/// Test helper to create files with specific byte patterns
fn create_test_file_with_bytes(temp_dir: &Path, filename: &str, bytes: &[u8]) -> Result<PathBuf> {
    let file_path = temp_dir.join(filename);
    fs::write(&file_path, bytes)?;
    Ok(file_path)
}

/// Test helper to create UTF-8 files with various content
fn create_test_file_with_content(temp_dir: &Path, filename: &str, content: &str) -> Result<PathBuf> {
    let file_path = temp_dir.join(format!("{}.md", filename));
    fs::write(&file_path, content.as_bytes())?;
    Ok(file_path)
}

#[cfg(test)]
mod encoding_validation_tests {
    use super::*;

    #[test]
    fn test_valid_utf8_with_unicode() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test various Unicode characters that should work correctly
        let test_content = "# Unicode Test\n\n\
            Emoji: 🚀🔥⭐🎯\n\
            Accented: café naïve résumé\n\
            Mathematical: ∑∏∫∆∇∂\n\
            Currency: $€£¥₹\n\
            Symbols: ♥♪♫☀☁☂\n\
            Mixed: Hello 世界 🌍 café";

        let file_path = create_test_file_with_content(temp_dir.path(), "unicode-test", test_content)?;

        // Verify file can be read as valid UTF-8
        let read_content = fs::read_to_string(&file_path)?;
        assert_eq!(read_content, test_content);

        // Test that it's not detected as binary
        let file_bytes = fs::read(&file_path)?;
        assert!(!is_binary_content(&file_bytes));

        // Test with secure file reader
        let config = SecureFileSystemConfig::default();
        let validator = PathValidator::new(temp_dir.path());
        let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
        let file_path_str = relative_path.to_str().unwrap();

        let validated_path = validator.validate_path(file_path_str)?;
        let secure_reader = SecureFileReader::new(temp_dir.path(), config);
        let secure_content = secure_reader.read_file_content(file_path_str)?;

        assert_eq!(secure_content, test_content);

        println!("✅ Valid UTF-8 Unicode test passed");
        Ok(())
    }

    #[test]
    fn test_utf8_bom_handling() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Content with UTF-8 BOM
        let content_with_bom = "# BOM Test\n\nContent with Byte Order Mark";
        let mut bytes_with_bom = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        bytes_with_bom.extend_from_slice(content_with_bom.as_bytes());

        let file_path = create_test_file_with_bytes(temp_dir.path(), "bom-test.md", &bytes_with_bom)?;

        // Read with standard FS (should include BOM)
        let raw_content = fs::read(&file_path)?;
        assert!(raw_content.starts_with(&[0xEF, 0xBB, 0xBF]));

        // Test binary detection (should NOT be detected as binary)
        assert!(!is_binary_content(&raw_content));

        // Test with secure file reader
        let config = SecureFileSystemConfig::default();
        let validator = PathValidator::new(temp_dir.path());
        let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
        let file_path_str = relative_path.to_str().unwrap();

        let validated_path = validator.validate_path(file_path_str)?;
        let secure_reader = SecureFileReader::new(temp_dir.path(), config);
        let secure_content = secure_reader.read_file_content(file_path_str)?;

        // Content should be readable but may include BOM characters
        println!("Secure content length: {}", secure_content.len());
        println!("Content starts with BOM: {}", secure_content.chars().next().unwrap() as u32);

        // Verify content is readable (BOM might appear as replacement character or be stripped)
        assert!(secure_content.contains("BOM Test"));
        assert!(secure_content.contains("Content with Byte Order Mark"));

        println!("✅ UTF-8 BOM handling test passed");
        Ok(())
    }

    #[test]
    fn test_invalid_utf8_sequences() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test various invalid UTF-8 sequences
        let invalid_utf8_cases = vec![
            // Invalid 2-byte sequence
            vec![0xC0, 0x00], // Second byte invalid
            vec![0xC0, 0x80], // Overlong encoding
            // Invalid 3-byte sequences
            vec![0xE0, 0x00, 0x80], // Second byte invalid
            vec![0xE0, 0x80, 0x00], // Third byte invalid
            vec![0xED, 0xA0, 0x80], // Surrogate area (U+D800)
            // Invalid 4-byte sequences
            vec![0xF0, 0x00, 0x80, 0x80], // Second byte invalid
            vec![0xF0, 0x80, 0x00, 0x80], // Third byte invalid
            vec![0xF0, 0x80, 0x80, 0x00], // Fourth byte invalid
            vec![0xF4, 0x90, 0x80, 0x80], // Beyond Unicode range
            // Incomplete sequences
            vec![0xC0], // Incomplete 2-byte
            vec![0xE0, 0x80], // Incomplete 3-byte
            vec![0xF0, 0x80, 0x80], // Incomplete 4-byte
        ];

        for (i, invalid_bytes) in invalid_utf8_cases.iter().enumerate() {
            let filename = format!("invalid-utf8-{}.md", i);
            let file_path = create_test_file_with_bytes(temp_dir.path(), &filename, invalid_bytes)?;

            // Test binary detection - should be detected as binary due to invalid UTF-8
            assert!(is_binary_content(invalid_bytes),
                "Case {} should be detected as binary due to invalid UTF-8", i);

            // Test that standard string reading fails
            let string_result = std::str::from_utf8(invalid_bytes);
            assert!(string_result.is_err(),
                "Case {} should fail UTF-8 validation", i);

            // Test with secure file reader - should reject file
            let config = SecureFileSystemConfig::default();
            let validator = PathValidator::new(temp_dir.path());
            let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
            let file_path_str = relative_path.to_str().unwrap();

            let validated_path = validator.validate_path(file_path_str)?;
            let secure_reader = SecureFileReader::new(temp_dir.path(), config);

            match secure_reader.read_file_content(file_path_str) {
                Ok(_) => panic!("Case {} should have failed due to invalid UTF-8", i),
                Err(e) => {
                    assert!(e.to_string().contains("Invalid UTF-8") ||
                           e.to_string().contains("Binary file detected"),
                           "Case {} should fail with UTF-8 or binary error, got: {}", i, e);
                }
            }
        }

        println!("✅ Invalid UTF-8 sequences test passed ({} cases)", invalid_utf8_cases.len());
        Ok(())
    }

    #[test]
    fn test_mixed_encoding_content() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create content that mixes valid UTF-8 with invalid bytes
        let mut mixed_content = b"# Mixed Encoding Test\n\n".to_vec();
        mixed_content.extend_from_slice("Valid UTF-8: café 🚀\n".as_bytes());
        mixed_content.extend_from_slice(&[0xC0, 0x80]); // Invalid overlong encoding
        mixed_content.extend_from_slice(b"\nMore valid content\n");
        mixed_content.push(0x80); // Invalid continuation byte without start
        mixed_content.extend_from_slice("End".as_bytes());

        let file_path = create_test_file_with_bytes(temp_dir.path(), "mixed-encoding.md", &mixed_content)?;

        // Test binary detection
        assert!(is_binary_content(&mixed_content));

        // Test with secure file reader
        let config = SecureFileSystemConfig::default();
        let validator = PathValidator::new(temp_dir.path());
        let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
        let file_path_str = relative_path.to_str().unwrap();

        let validated_path = validator.validate_path(file_path_str)?;
        let secure_reader = SecureFileReader::new(temp_dir.path(), config);

        match secure_reader.read_file_content(file_path_str) {
            Ok(_) => panic!("Mixed encoding should be rejected as binary"),
            Err(e) => {
                assert!(e.to_string().contains("Binary file detected"));
            }
        }

        println!("✅ Mixed encoding content test passed");
        Ok(())
    }

    #[test]
    fn test_high_unicode_code_points() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test very high Unicode code points that might cause issues
        let high_unicode_content = "# High Unicode Test\n\n\
            Mathematical symbols: ∮∰∳∴∵∶∷∸\n\
            Miscellaneous: ⏚⏛⏜⏝⏞⏟⏠⏡\n\
            Emoji: 🦄🦅🦆🦇🦈🦉\n\
            CJK: 中文测试 日本語 한국어\n\
            Combining: e\u{0301} (e + combining acute)";

        let file_path = create_test_file_with_content(temp_dir.path(), "high-unicode", high_unicode_content)?;

        // Test that it reads correctly
        let read_content = fs::read_to_string(&file_path)?;
        assert_eq!(read_content, high_unicode_content);

        // Test that it's not detected as binary
        let file_bytes = fs::read(&file_path)?;
        assert!(!is_binary_content(&file_bytes));

        // Test Unicode normalization
        let normalized = high_unicode_content.nfc().collect::<String>();
        assert_ne!(normalized, high_unicode_content); // Some characters should be normalized

        println!("✅ High Unicode code points test passed");
        Ok(())
    }

    #[test]
    fn test_unicode_normalization_scenarios() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test content that requires Unicode normalization
        let normalization_test_cases = vec![
            // Cafe in different forms
            ("café", vec!["cafe\u{0301}"]), // NFD form
            // Other accented characters
            ("naïve", vec!["nai\u{0308}ve"]),
            ("résumé", vec!["re\u{0301}sume\u{0301}"]),
            // Multiple combining marks
            ("e\u{0301}\u{0302}", vec!["\u{0301}\u{0302}e"]), // Multiple accents
        ];

        for (original, _variants) in normalization_test_cases {
            let content = format!("# Normalization Test\n\nOriginal: {}", original);
            let file_path = create_test_file_with_content(temp_dir.path(), "normalization-test", &content)?;

            // Test that content is readable and not binary
            let file_bytes = fs::read(&file_path)?;
            assert!(!is_binary_content(&file_bytes));

            let read_content = fs::read_to_string(&file_path)?;
            assert!(read_content.contains(original));

            // Test normalization functions
            let nfc_form = content.nfc().collect::<String>();
            let nfd_form = content.nfd().collect::<String>();

            // Should be different for content with combining marks
            if content.contains('\u{0300}') || content.contains('\u{0301}') || content.contains('\u{0302}') {
                assert_ne!(nfc_form, nfd_form, "NFC and NFD forms should differ for: {}", original);
            }
        }

        println!("✅ Unicode normalization scenarios test passed");
        Ok(())
    }

    #[test]
    fn test_partial_utf8_at_boundaries() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test UTF-8 sequences that cross buffer boundaries
        // Create a string that will be split across typical 4KB or 8KB boundaries
        let base_text = "x".repeat(4090); // Get close to buffer boundary
        let unicode_char = "🚀"; // 4-byte UTF-8 sequence
        let more_text = "y".repeat(100);

        let boundary_content = format!("# Boundary Test\n\n{}{}{}", base_text, unicode_char, more_text);

        let file_path = create_test_file_with_content(temp_dir.path(), "boundary-test", &boundary_content)?;

        // Test that it reads correctly
        let read_content = fs::read_to_string(&file_path)?;
        assert_eq!(read_content, boundary_content);

        // Test that it's not detected as binary
        let file_bytes = fs::read(&file_path)?;
        assert!(!is_binary_content(&file_bytes));

        // Test with secure file reader (which uses buffered reading)
        let config = SecureFileSystemConfig::default();
        let validator = PathValidator::new(temp_dir.path());
        let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
        let file_path_str = relative_path.to_str().unwrap();

        let validated_path = validator.validate_path(file_path_str)?;
        let secure_reader = SecureFileReader::new(temp_dir.path(), config);
        let secure_content = secure_reader.read_file_content(file_path_str)?;

        assert_eq!(secure_content, boundary_content);
        assert!(secure_content.contains("🚀"));

        println!("✅ Partial UTF-8 at boundaries test passed");
        Ok(())
    }

    #[test]
    fn test_binary_files_with_text_like_content() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test files that are binary but might contain some text-like content
        let binary_with_text_cases = vec![
            // PNG with text chunk (common in metadata)
            (vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D],
             "PNG header"),
            // ZIP with text entries
            (vec![0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00],
             "ZIP header"),
            // Executable with text strings
            (vec![0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00],
             "ELF header"),
        ];

        for (binary_header, description) in &binary_with_text_cases {
            let mut content = binary_header.clone();
            // Add some text-like content after the binary header
            content.extend_from_slice(b"This looks like text but is in a binary file\n");
            content.extend_from_slice(b"More text content here\n");

            let filename = format!("binary-text-{}.bin", description.replace(' ', "_"));
            let file_path = create_test_file_with_bytes(temp_dir.path(), &filename, &content)?;

            // Should be detected as binary due to headers
            assert!(is_binary_content(&content),
                "{} should be detected as binary", description);

            // Test with secure file reader
            let config = SecureFileSystemConfig::default();
            let validator = PathValidator::new(temp_dir.path());
            let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
            let file_path_str = relative_path.to_str().unwrap();

            let validated_path = validator.validate_path(file_path_str)?;
            let secure_reader = SecureFileReader::new(temp_dir.path(), config);

            match secure_reader.read_file_content(file_path_str) {
                Ok(_) => panic!("{} should be rejected as binary", description),
                Err(e) => {
                    assert!(e.to_string().contains("Binary file detected"));
                }
            }
        }

        println!("✅ Binary files with text-like content test passed ({} cases)", binary_with_text_cases.len());
        Ok(())
    }

    #[test]
    fn test_edge_case_empty_and_minimal_files() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Test edge cases with very small or empty files
        let edge_cases = vec![
            (vec![], "empty file"),
            (vec![0x09], "tab only"),
            (vec![0x0A], "newline only"),
            (vec![0x0D], "carriage return only"),
            (vec![0x20], "space only"),
            (vec![0xEF, 0xBB, 0xBF], "BOM only"),
            (vec![0xC2, 0xA9], "© symbol"),
            (vec![0xE2, 0x98, 0x83], "☃ snowman"),
        ];

        for (bytes, description) in &edge_cases {
            let filename = format!("edge-{}.md", description.replace(' ', "_"));
            let file_path = create_test_file_with_bytes(temp_dir.path(), &filename, &bytes)?;

            // None of these should be detected as binary (they're all valid UTF-8 or empty)
            assert!(!is_binary_content(&bytes),
                "{} should not be detected as binary", description);

            // Test with secure file reader
            let config = SecureFileSystemConfig::default();
            let validator = PathValidator::new(temp_dir.path());
            let relative_path = file_path.strip_prefix(temp_dir.path()).unwrap();
            let file_path_str = relative_path.to_str().unwrap();

            let validated_path = validator.validate_path(file_path_str)?;
            let secure_reader = SecureFileReader::new(temp_dir.path(), config);

            match secure_reader.read_file_content(file_path_str) {
                Ok(content) => {
                    // Content should be readable as UTF-8
                    let string_result = std::str::from_utf8(&bytes);
                    assert!(string_result.is_ok() || bytes.is_empty(),
                           "{} should be valid UTF-8 or empty", description);
                }
                Err(e) => {
                    // Only acceptable error for non-empty files would be unexpected
                    if !bytes.is_empty() {
                        panic!("{} should be readable: {}", description, e);
                    }
                }
            }
        }

        println!("✅ Edge case empty and minimal files test passed ({} cases)", edge_cases.len());
        Ok(())
    }
}