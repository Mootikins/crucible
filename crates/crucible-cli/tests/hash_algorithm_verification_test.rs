///! Verification test to ensure parser and scanner use the same hash algorithm
///!
///! This test confirms that both components produce identical BLAKE3 hashes,
///! which is critical for change detection to work correctly.

use anyhow::Result;
use tempfile::TempDir;

#[tokio::test]
async fn test_parser_scanner_use_same_blake3_algorithm() -> Result<()> {
    println!("\n=== Hash Algorithm Verification Test ===\n");

    // Create a temporary test vault
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();
    let test_file = vault_path.join("test.md");

    // Use the same content as e2e tests
    let test_content = "# Getting Started\n\nWelcome to the documentation.";
    std::fs::write(&test_file, test_content)?;
    println!("✓ Created test file with {} bytes", test_content.len());

    // 1. Compute hash via PARSER
    println!("\n1. Computing hash via PARSER (PulldownParser)...");
    let parsed_doc = crucible_surrealdb::parse_file_to_document(&test_file).await?;
    let parser_hash = parsed_doc.content_hash.clone();
    println!("   Parser hash:  {}", parser_hash);

    // 2. Compute hash via SCANNER
    println!("\n2. Computing hash via SCANNER (KilnScanner)...");
    let scan_config = crucible_surrealdb::kiln_scanner::KilnScannerConfig::default();
    let files = crucible_surrealdb::kiln_processor::scan_kiln_directory(
        &vault_path.to_path_buf(),
        &scan_config,
    ).await?;

    assert_eq!(files.len(), 1, "Should find exactly one file");
    let scanner_hash = files[0].content_hash_hex();
    println!("   Scanner hash: {}", scanner_hash);

    // 3. Verify both use BLAKE3 by computing reference hash directly
    println!("\n3. Computing reference BLAKE3 hash...");
    let file_bytes = std::fs::read(&test_file)?;
    let mut ref_hasher = blake3::Hasher::new();
    ref_hasher.update(&file_bytes);
    let reference_hash = ref_hasher.finalize().to_hex().to_string();
    println!("   Reference hash: {}", reference_hash);

    // 4. Compare all hashes
    println!("\n4. Verifying all hashes match...");
    println!("   Parser:    {}", parser_hash);
    println!("   Scanner:   {}", scanner_hash);
    println!("   Reference: {}", reference_hash);

    // All three should match
    assert_eq!(
        parser_hash, reference_hash,
        "Parser hash must match reference BLAKE3 hash"
    );

    assert_eq!(
        scanner_hash, reference_hash,
        "Scanner hash must match reference BLAKE3 hash"
    );

    assert_eq!(
        parser_hash, scanner_hash,
        "Parser and scanner must compute identical hashes"
    );

    println!("\n✅ SUCCESS: All hashes match!");
    println!("   Both parser and scanner correctly use BLAKE3 algorithm");

    Ok(())
}

#[tokio::test]
async fn test_hash_consistency_across_file_sizes() -> Result<()> {
    println!("\n=== Hash Consistency Test (Various File Sizes) ===\n");

    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Test with different file sizes to ensure streaming hash matches full-file hash
    let medium_content = format!("# Medium\n\n{}", "Lorem ipsum dolor sit amet.\n".repeat(100));
    let large_content = format!("# Large\n\n{}", "Lorem ipsum dolor sit amet.\n".repeat(1000));

    let test_cases = vec![
        ("small.md", "# Small\n\nTiny content.".to_string()),
        ("medium.md", medium_content),
        ("large.md", large_content),
    ];

    for (filename, content) in test_cases {
        println!("Testing {}...", filename);
        let test_file = vault_path.join(filename);
        std::fs::write(&test_file, &content)?;

        // Parser hash
        let parsed_doc = crucible_surrealdb::parse_file_to_document(&test_file).await?;
        let parser_hash = parsed_doc.content_hash;

        // Scanner hash
        let scan_config = crucible_surrealdb::kiln_scanner::KilnScannerConfig::default();
        let files = crucible_surrealdb::kiln_processor::scan_kiln_directory(
            &vault_path.to_path_buf(),
            &scan_config,
        ).await?;

        let scanner_hash = files.iter()
            .find(|f| f.path.file_name().unwrap().to_str().unwrap() == filename)
            .expect("File should be found")
            .content_hash_hex();

        assert_eq!(
            parser_hash, scanner_hash,
            "{}: Parser and scanner hashes must match",
            filename
        );

        println!("  ✓ {} - hashes match: {}...", filename, &parser_hash[..16]);

        // Clean up for next iteration
        std::fs::remove_file(&test_file)?;
    }

    println!("\n✅ All file sizes produce consistent hashes");

    Ok(())
}
