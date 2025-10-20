//! Real Vault Validation Test
//!
//! This test scans and validates the user's actual vault, providing comprehensive
//! statistics and validation of the parsing infrastructure.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default vault path (~/Documents/crucible-testing)
//! CRUCIBLE_TEST_VAULT=1 cargo test -p crucible-daemon --test vault_validation -- --ignored --nocapture
//!
//! # Run with custom vault path
//! CRUCIBLE_TEST_VAULT=/path/to/vault cargo test -p crucible-daemon --test vault_validation -- --ignored --nocapture
//! ```
//!
//! ## Features
//!
//! - Scans all .md files recursively (READ-ONLY)
//! - Parses each file and collects comprehensive statistics
//! - Indexes to in-memory database
//! - Reports detailed statistics
//! - Shows sample parsed documents with previews
//! - Handles errors gracefully

use anyhow::{Context, Result};
use crucible_core::parser::{MarkdownParser, ParsedDocument, PulldownParser};
use crucible_surrealdb::SurrealEmbeddingDatabase;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;

// ============================================================================
// Configuration
// ============================================================================

/// Get vault path from environment variable
///
/// Returns None if CRUCIBLE_TEST_VAULT is not set, causing test to be skipped.
/// If set to "1", uses default path ~/Documents/crucible-testing
/// Otherwise uses the provided path.
fn get_vault_path() -> Option<PathBuf> {
    match std::env::var("CRUCIBLE_TEST_VAULT") {
        Ok(val) if val == "1" => {
            // Use default vault path
            dirs::home_dir().map(|home| home.join("Documents/crucible-testing"))
        }
        Ok(val) if !val.is_empty() => {
            // Use custom vault path
            Some(PathBuf::from(val))
        }
        _ => None,
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Statistics about a parsed file
#[derive(Debug, Clone)]
struct ParsedFile {
    path: PathBuf,
    doc: ParsedDocument,
    size_bytes: u64,
    parse_time_ms: u64,
}

/// Parse error information
#[derive(Debug, Clone)]
struct ParseError {
    path: PathBuf,
    error: String,
}

/// Comprehensive vault statistics
#[derive(Debug, Default)]
struct VaultStats {
    // File counts
    total_files: usize,
    successful: Vec<ParsedFile>,
    errors: Vec<ParseError>,

    // Content stats
    total_bytes: u64,
    total_words: usize,
    total_chars: usize,

    // File type stats
    with_frontmatter: usize,
    without_frontmatter: usize,

    // Tag stats
    total_tags: usize,
    unique_tags: HashSet<String>,
    inline_tags: usize,
    frontmatter_tags: usize,
    nested_tags: usize,

    // Wikilink stats
    total_wikilinks: usize,
    unique_targets: HashSet<String>,
    heading_links: usize,
    block_links: usize,
    embeds: usize,
    aliased_links: usize,

    // Structure stats
    total_headings: usize,
    total_code_blocks: usize,

    // Performance stats
    total_parse_time_ms: u64,
    min_parse_time_ms: u64,
    max_parse_time_ms: u64,
}

impl VaultStats {
    /// Add a successfully parsed file
    fn add_successful(&mut self, path: &Path, doc: ParsedDocument, parse_time: std::time::Duration) {
        let parse_time_ms = parse_time.as_millis() as u64;
        let size_bytes = doc.file_size;

        // Update performance stats
        self.total_parse_time_ms += parse_time_ms;
        if self.min_parse_time_ms == 0 || parse_time_ms < self.min_parse_time_ms {
            self.min_parse_time_ms = parse_time_ms;
        }
        if parse_time_ms > self.max_parse_time_ms {
            self.max_parse_time_ms = parse_time_ms;
        }

        // Update content stats
        self.total_bytes += size_bytes;
        self.total_words += doc.content.word_count;
        self.total_chars += doc.content.char_count;

        // Update frontmatter stats
        if doc.frontmatter.is_some() {
            self.with_frontmatter += 1;
        } else {
            self.without_frontmatter += 1;
        }

        // Update tag stats
        let inline_tag_count = doc.tags.len();
        self.inline_tags += inline_tag_count;
        self.total_tags += inline_tag_count;

        for tag in &doc.tags {
            self.unique_tags.insert(tag.name.clone());
            if tag.is_nested() {
                self.nested_tags += 1;
            }
        }

        // Count frontmatter tags
        if let Some(fm) = &doc.frontmatter {
            if let Some(fm_tags) = fm.get_array("tags") {
                self.frontmatter_tags += fm_tags.len();
                self.total_tags += fm_tags.len();
                for tag in fm_tags {
                    self.unique_tags.insert(tag);
                }
            }
        }

        // Update wikilink stats
        self.total_wikilinks += doc.wikilinks.len();
        for link in &doc.wikilinks {
            self.unique_targets.insert(link.target.clone());

            if link.heading_ref.is_some() {
                self.heading_links += 1;
            }
            if link.block_ref.is_some() {
                self.block_links += 1;
            }
            if link.is_embed {
                self.embeds += 1;
            }
            if link.alias.is_some() {
                self.aliased_links += 1;
            }
        }

        // Update structure stats
        self.total_headings += doc.content.headings.len();
        self.total_code_blocks += doc.content.code_blocks.len();

        self.successful.push(ParsedFile {
            path: path.to_path_buf(),
            doc,
            size_bytes,
            parse_time_ms,
        });
    }

    /// Add a parse error
    fn add_error(&mut self, path: &Path, error: String) {
        self.errors.push(ParseError {
            path: path.to_path_buf(),
            error,
        });
    }

    /// Get average parse time
    fn avg_parse_time_ms(&self) -> u64 {
        if self.successful.is_empty() {
            0
        } else {
            self.total_parse_time_ms / self.successful.len() as u64
        }
    }

    /// Get success rate as percentage
    fn success_rate(&self) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            (self.successful.len() as f64 / self.total_files as f64) * 100.0
        }
    }
}

// ============================================================================
// Core Functions
// ============================================================================

/// Recursively scan vault for all markdown files
async fn scan_markdown_files(vault_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut queue = vec![vault_path.to_path_buf()];

    while let Some(dir) = queue.pop() {
        let mut entries = fs::read_dir(&dir)
            .await
            .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name() {
                    if !name.to_string_lossy().starts_with('.') {
                        queue.push(path);
                    }
                }
            } else if metadata.is_file() {
                // Check for .md extension
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        files.push(path);
                    }
                }
            }
        }
    }

    // Sort files by path for deterministic output
    files.sort();
    Ok(files)
}

/// Parse all files and collect statistics
async fn parse_all_files(files: &[PathBuf]) -> Result<VaultStats> {
    let parser = PulldownParser::new();
    let mut stats = VaultStats::default();
    stats.total_files = files.len();

    for path in files {
        let start = Instant::now();

        match parser.parse_file(path).await {
            Ok(doc) => {
                stats.add_successful(path, doc, start.elapsed());
            }
            Err(e) => {
                stats.add_error(path, e.to_string());
            }
        }
    }

    Ok(stats)
}

/// Index files to in-memory database
async fn index_to_database(files: &[ParsedFile]) -> Result<DatabaseIndexStats> {
    let db = SurrealEmbeddingDatabase::new_memory();
    db.initialize().await?;

    let mut stats = DatabaseIndexStats::default();

    for file in files {
        // Create metadata from parsed document
        let metadata = create_metadata(&file.doc);

        // Create dummy embedding (384-dimensional zero vector)
        let embedding = vec![0.0f32; 384];

        // Store in database
        let path_str = file.path.to_string_lossy().to_string();
        db.store_embedding(&path_str, &file.doc.content.plain_text, &embedding, &metadata)
            .await
            .with_context(|| format!("Failed to store: {}", path_str))?;

        stats.files_indexed += 1;

        // Count wikilinks as relations
        stats.relations_created += file.doc.wikilinks.len();
    }

    Ok(stats)
}

/// Database indexing statistics
#[derive(Debug, Default)]
struct DatabaseIndexStats {
    files_indexed: usize,
    relations_created: usize,
}

/// Create metadata map from parsed document
fn create_metadata(doc: &ParsedDocument) -> crucible_surrealdb::EmbeddingMetadata {
    use chrono::Utc;

    let mut properties = HashMap::new();

    // Add wikilink targets
    let targets: Vec<String> = doc.wikilinks.iter().map(|w| w.target.clone()).collect();
    if !targets.is_empty() {
        properties.insert("links".to_string(), serde_json::json!(targets));
    }

    // Add counts
    properties.insert("word_count".to_string(), serde_json::json!(doc.content.word_count));
    properties.insert("heading_count".to_string(), serde_json::json!(doc.content.headings.len()));

    // Add frontmatter properties if present
    if let Some(fm) = &doc.frontmatter {
        for (key, value) in fm.properties() {
            properties.insert(key.clone(), value.clone());
        }
    }

    // Extract folder from path (parent directory)
    let folder = doc
        .path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();

    crucible_surrealdb::EmbeddingMetadata {
        file_path: doc.path.to_string_lossy().to_string(),
        title: Some(doc.title()),
        tags: doc.all_tags(),
        folder,
        properties,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

// ============================================================================
// Reporting
// ============================================================================

/// Print comprehensive validation report
fn print_validation_report(stats: &VaultStats, db_stats: &DatabaseIndexStats) {
    println!("\n═══════════════════════════════════════════════════");
    println!("  Crucible Vault Validation Report");
    println!("═══════════════════════════════════════════════════\n");

    // Files section
    println!("📁 FILES:");
    println!("  Total markdown files:    {}", stats.total_files);
    println!("  Successfully parsed:     {}", stats.successful.len());
    println!("  Parse errors:            {}", stats.errors.len());
    println!("  Success rate:            {:.1}%", stats.success_rate());
    println!("  Total size:              {:.2} MB", stats.total_bytes as f64 / 1_048_576.0);

    // Content section
    println!("\n📝 CONTENT:");
    println!("  Total words:             {}", stats.total_words);
    println!("  Total characters:        {}", stats.total_chars);
    println!("  With frontmatter:        {} ({:.1}%)",
             stats.with_frontmatter,
             (stats.with_frontmatter as f64 / stats.total_files as f64) * 100.0);
    println!("  Without frontmatter:     {} ({:.1}%)",
             stats.without_frontmatter,
             (stats.without_frontmatter as f64 / stats.total_files as f64) * 100.0);
    println!("  Total headings:          {}", stats.total_headings);
    println!("  Total code blocks:       {}", stats.total_code_blocks);

    // Tags section
    println!("\n🏷️  TAGS:");
    println!("  Total tag occurrences:   {}", stats.total_tags);
    println!("  Unique tags:             {}", stats.unique_tags.len());
    println!("  Inline tags:             {}", stats.inline_tags);
    println!("  Frontmatter tags:        {}", stats.frontmatter_tags);
    println!("  Nested tags:             {}", stats.nested_tags);

    // Show top tags
    if !stats.unique_tags.is_empty() {
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        for file in &stats.successful {
            for tag in file.doc.all_tags() {
                *tag_counts.entry(tag).or_insert(0) += 1;
            }
        }
        let mut sorted_tags: Vec<_> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

        println!("\n  Top 10 tags:");
        for (tag, count) in sorted_tags.iter().take(10) {
            println!("    {}: {}", tag, count);
        }
    }

    // Wikilinks section
    println!("\n🔗 WIKILINKS:");
    println!("  Total wikilinks:         {}", stats.total_wikilinks);
    println!("  Unique targets:          {}", stats.unique_targets.len());
    println!("  Heading references:      {}", stats.heading_links);
    println!("  Block references:        {}", stats.block_links);
    println!("  Embeds:                  {}", stats.embeds);
    println!("  Aliased links:           {}", stats.aliased_links);

    // Database section
    println!("\n💾 DATABASE:");
    println!("  Files indexed:           {}", db_stats.files_indexed);
    println!("  Relations created:       {}", db_stats.relations_created);

    // Performance section
    println!("\n⚡ PERFORMANCE:");
    println!("  Total parse time:        {} ms", stats.total_parse_time_ms);
    println!("  Average parse time:      {} ms", stats.avg_parse_time_ms());
    println!("  Min parse time:          {} ms", stats.min_parse_time_ms);
    println!("  Max parse time:          {} ms", stats.max_parse_time_ms);

    if stats.total_parse_time_ms > 0 {
        let throughput = (stats.successful.len() as f64 / (stats.total_parse_time_ms as f64 / 1000.0)) as usize;
        println!("  Throughput:              {} files/sec", throughput);
    }

    // Errors section
    if !stats.errors.is_empty() {
        println!("\n⚠️  ERRORS:");
        for error in &stats.errors {
            println!("  {} - {}", error.path.display(), error.error);
        }
    }

    println!("\n═══════════════════════════════════════════════════\n");
}

/// Print sample parsed documents
fn print_sample_documents(stats: &VaultStats, count: usize) {
    println!("📄 SAMPLE PARSED DOCUMENTS (showing {}):\n", count.min(stats.successful.len()));

    for (i, file) in stats.successful.iter().take(count).enumerate() {
        println!("{}. {}", i + 1, file.path.display());
        println!("   Size: {} bytes, Parse time: {} ms", file.size_bytes, file.parse_time_ms);

        // Show frontmatter info
        if let Some(fm) = &file.doc.frontmatter {
            let props = fm.properties();
            println!("   Frontmatter: {} fields", props.len());

            if let Some(title) = fm.get_string("title") {
                println!("   Title: {}", title);
            }

            // Show frontmatter properties
            let prop_keys: Vec<_> = props.keys().take(5).map(|s| s.as_str()).collect();
            if !prop_keys.is_empty() {
                println!("   Properties: {}", prop_keys.join(", "));
            }
        } else {
            println!("   Frontmatter: None");
        }

        // Show content stats
        println!("   Words: {}, Tags: {}, Wikilinks: {}",
                 file.doc.content.word_count,
                 file.doc.all_tags().len(),
                 file.doc.wikilinks.len());

        // Show headings
        if !file.doc.content.headings.is_empty() {
            let heading_count = file.doc.content.headings.len();
            let first_heading = &file.doc.content.headings[0];
            if heading_count == 1 {
                println!("   Headings: 1 (\"{}\")", first_heading.text);
            } else {
                println!("   Headings: {} (first: \"{}\")", heading_count, first_heading.text);
            }
        }

        // Show preview
        let preview_len = 100.min(file.doc.content.plain_text.len());
        let preview: String = file.doc.content.plain_text.chars().take(preview_len).collect();
        let ellipsis = if file.doc.content.plain_text.len() > 100 { "..." } else { "" };
        println!("   Preview: {}{}", preview.trim(), ellipsis);

        println!();
    }
}

// ============================================================================
// Test
// ============================================================================

#[tokio::test]
#[ignore] // Ignored by default, run with --ignored
async fn test_validate_real_vault() -> Result<()> {
    // Get vault path from environment
    let Some(vault_path) = get_vault_path() else {
        println!("\n⏭️  Test skipped");
        println!("   Set CRUCIBLE_TEST_VAULT=1 to run with default vault");
        println!("   Or set CRUCIBLE_TEST_VAULT=/path/to/vault for custom path\n");
        return Ok(());
    };

    // Verify vault exists
    if !vault_path.exists() {
        anyhow::bail!("Vault path does not exist: {}", vault_path.display());
    }

    println!("\n🔍 Scanning vault: {}\n", vault_path.display());

    // Step 1: Scan vault for all .md files
    let scan_start = Instant::now();
    let files = scan_markdown_files(&vault_path).await?;
    let scan_time = scan_start.elapsed();

    println!("✓ Found {} markdown files in {:.2}s\n", files.len(), scan_time.as_secs_f64());

    if files.is_empty() {
        println!("⚠️  No markdown files found in vault");
        return Ok(());
    }

    // Step 2: Parse all files and collect stats
    println!("📖 Parsing files...\n");
    let parse_start = Instant::now();
    let stats = parse_all_files(&files).await?;
    let parse_time = parse_start.elapsed();

    println!("✓ Parsed {} files in {:.2}s\n", stats.total_files, parse_time.as_secs_f64());

    // Step 3: Index to in-memory database
    println!("💾 Indexing to database...\n");
    let index_start = Instant::now();
    let db_stats = index_to_database(&stats.successful).await?;
    let index_time = index_start.elapsed();

    println!("✓ Indexed {} files in {:.2}s\n", db_stats.files_indexed, index_time.as_secs_f64());

    // Step 4: Print comprehensive report
    print_validation_report(&stats, &db_stats);

    // Step 5: Show sample documents
    print_sample_documents(&stats, 5);

    // Verify success
    assert!(
        stats.success_rate() > 50.0,
        "Success rate too low: {:.1}%",
        stats.success_rate()
    );

    Ok(())
}
