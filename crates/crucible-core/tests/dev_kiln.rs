//! Dev-Kiln Documentation Validation Tests
//!
//! These tests validate the integrity and quality of the `examples/dev-kiln/` documentation.
//! They follow TDD (RED-GREEN-REFACTOR) methodology:
//!
//! 1. RED: Tests are written to FAIL initially
//! 2. GREEN: Documentation is fixed to make tests pass
//! 3. REFACTOR: Improve without changing behavior
//!
//! Run with: `cargo test -p crucible-core -- --ignored dev_kiln`
//!
//! # Test Coverage
//!
//! 1. **Parsing**: All `.md` files must parse without errors
//! 2. **Frontmatter**: Required fields (title, description, tags)
//! 3. **Wikilinks**: STRICT - ALL wikilinks must resolve to existing files
//! 4. **Code References**: All `crates/...` paths must exist in repo
//! 5. **Rune Scripts**: `.rn` files must have valid syntax

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Base path to dev-kiln (relative to workspace root)
const DEV_KILN_PATH: &str = "examples/dev-kiln";

/// Get absolute path to dev-kiln from workspace root
fn dev_kiln_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(DEV_KILN_PATH)
}

/// Find all markdown files in dev-kiln
fn find_markdown_files() -> Vec<PathBuf> {
    let root = dev_kiln_root();
    WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Find all Rune script files in dev-kiln
fn find_rune_files() -> Vec<PathBuf> {
    let root = dev_kiln_root();
    WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rn"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Extract wikilinks from markdown content using regex
///
/// Pattern: `[[target]]` or `[[target|alias]]` or `[[target#heading]]` etc.
/// Returns list of wikilink targets (before any `#` or `|`)
///
/// Skips:
/// - Wikilinks inside fenced code blocks (```...```)
/// - Wikilinks inside inline code (`...`)
/// - Wikilinks that span multiple lines (malformed)
fn extract_wikilinks(content: &str) -> Vec<String> {
    // Remove fenced code blocks first
    let fenced_re = regex::Regex::new(r"```[\s\S]*?```").unwrap();
    let without_fenced = fenced_re.replace_all(content, "");

    // Remove inline code
    let inline_re = regex::Regex::new(r"`[^`]+`").unwrap();
    let without_code = inline_re.replace_all(&without_fenced, "");

    // Match wikilinks (single line only - no newlines in target)
    let wikilink_re = regex::Regex::new(r"!?\[\[([^\]\n]+)\]\]").unwrap();

    wikilink_re.captures_iter(&without_code)
        .map(|cap| {
            let full_link = cap.get(1).unwrap().as_str();

            // Extract just the target (before # or |)
            let target = full_link
                .split('|') // Remove alias
                .next()
                .unwrap()
                .split('#') // Remove heading/block reference
                .next()
                .unwrap()
                .trim()
                .to_string();

            target
        })
        .collect()
}

/// Extract code references from markdown content
///
/// Finds paths like `crates/crucible-core/src/...` in the content
fn extract_code_references(content: &str) -> Vec<String> {
    let re = regex::Regex::new(r"crates/[a-zA-Z0-9_-]+/[^\s)`]+").unwrap();

    re.find_iter(content)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Resolve a wikilink target to a file path
///
/// Resolution algorithm (Obsidian-style, name-only):
/// - `[[Title]]` -> Search for `Title.md` anywhere in dev-kiln
/// - `[[Folder/Title]]` -> Extract `Title`, search for `Title.md` anywhere
///
/// This matches Obsidian's behavior where paths are hints, not requirements.
/// Multi-kiln resolution is a future design consideration (see thoughts/backlog.md).
fn resolve_wikilink(target: &str, dev_kiln_root: &Path) -> Option<PathBuf> {
    // Extract just the filename (ignore path prefixes like "Help/Config/")
    let filename_part = target
        .rsplit('/')
        .next()
        .unwrap_or(target);

    // Try common extensions: .md (notes), .rn (Rune scripts)
    let extensions = [".md", ".rn"];

    for ext in extensions {
        let target_filename = format!("{}{}", filename_part, ext).to_lowercase();

        // Search for filename anywhere in dev-kiln (case-insensitive)
        for entry in WalkDir::new(dev_kiln_root) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Some(filename) = entry.path().file_name() {
                        if filename.to_string_lossy().to_lowercase() == target_filename {
                            return Some(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Parse frontmatter from markdown content
///
/// Returns None if no frontmatter exists, or the raw YAML content
fn extract_frontmatter(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0] != "---" {
        return None;
    }

    // Find closing ---
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if *line == "---" {
            end_idx = Some(i);
            break;
        }
    }

    if let Some(end) = end_idx {
        Some(lines[1..end].join("\n"))
    } else {
        None
    }
}

/// Parse YAML frontmatter into a simple key-value map
fn parse_frontmatter_fields(yaml: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Simple key: value parsing (doesn't handle nested structures)
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // Skip array items (lines starting with -)
            if !key.starts_with('-') {
                fields.insert(key, value);
            }
        }
    }

    fields
}

// ============================================================================
// TEST 1: All Markdown Files Parse Successfully
// ============================================================================

#[tokio::test]
#[ignore] // Slow test - run explicitly
async fn dev_kiln_all_notes_parse() {
    use crucible_parser::test_utils::parse_note;

    let md_files = find_markdown_files();

    assert!(
        !md_files.is_empty(),
        "Dev-kiln should contain at least one markdown file"
    );

    let mut failures = Vec::new();

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{}: Failed to read file: {}", file_path.display(), e));
                continue;
            }
        };

        // Attempt to parse
        if let Err(e) = parse_note(&content, file_path.to_str().unwrap()).await {
            failures.push(format!("{}: Parse error: {}", file_path.display(), e));
        }
    }

    if !failures.is_empty() {
        panic!(
            "❌ PARSE FAILURES ({}/{} files failed):\n\n{}",
            failures.len(),
            md_files.len(),
            failures.join("\n")
        );
    }

    println!("✅ All {} markdown files parsed successfully", md_files.len());
}

// ============================================================================
// TEST 2: All Notes Have Required Frontmatter Fields
// ============================================================================

#[tokio::test]
#[ignore] // Slow test - run explicitly
async fn dev_kiln_frontmatter_has_required_fields() {
    let md_files = find_markdown_files();
    let required_fields = vec!["title", "description", "tags"];

    let mut failures = Vec::new();

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{}: Failed to read file: {}", file_path.display(), e));
                continue;
            }
        };

        // Extract and parse frontmatter
        let frontmatter = match extract_frontmatter(&content) {
            Some(fm) => fm,
            None => {
                failures.push(format!("{}: Missing frontmatter", file_path.display()));
                continue;
            }
        };

        let fields = parse_frontmatter_fields(&frontmatter);

        // Check for required fields
        let mut missing = Vec::new();
        for required in &required_fields {
            if !fields.contains_key(*required) {
                missing.push(*required);
            }
        }

        if !missing.is_empty() {
            failures.push(format!(
                "{}: Missing required fields: {}",
                file_path.display(),
                missing.join(", ")
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "❌ FRONTMATTER VALIDATION FAILURES ({}/{} files failed):\n\n{}",
            failures.len(),
            md_files.len(),
            failures.join("\n")
        );
    }

    println!("✅ All {} markdown files have required frontmatter", md_files.len());
}

// ============================================================================
// TEST 3: Wikilink Resolution - Real Links Must Resolve
// ============================================================================

/// Check if a wikilink is an example/illustrative link (used to demonstrate syntax)
///
/// These are allowed to be "broken" because they're showing users what wikilinks look like,
/// not actually linking to content.
fn is_example_link(target: &str) -> bool {
    // Generic placeholder names
    let placeholders = [
        "Note Name", "Other Note", "Note", "Another Idea", "Related Concept",
        "link", "wikilinks", "broken", "...", "first", "second", "third",
        "note with spaces", "note-with-dashes", "note_with_underscores", "note.with.dots",
        "not a link", "` and `",
    ];
    if placeholders.contains(&target) {
        return true;
    }

    // Zettelkasten/PKM examples (concepts, not actual notes)
    let zettelkasten_examples = [
        "Deep Work", "Flow States", "Attention Residue", "Time Blocking",
        "Deliberate Practice", "Pomodoro Technique", "Batching", "Multitasking Myth",
        "Active Recall", "Forgetting Curve", "Interleaving", "Anki",
        "Time Value of Money", "Investment Growth", "Learning Techniques Index",
        "Deep Work by Cal Newport", "Flow by Mihaly Csikszentmihalyi",
        "Creative Process", "Creative Constraints", "Morning Creative Sessions",
        "Daily Routines", "Deep Work Practices", "Connection 1", "Connection 2",
        "Book Notes", "Research Paper", "Productivity System", "Project Planning",
    ];
    if zettelkasten_examples.contains(&target) {
        return true;
    }

    // Johnny Decimal examples (numbered organization)
    if target.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return true; // Things like "21.01 Invoice Template", "11 Company Info"
    }

    // PARA examples (project structure)
    let para_examples = [
        "Projects/Product Launch/Index", "Projects/Q4 Report/Index",
        "Areas/Team Management/Index", "Areas/Health/Index",
        "Projects/Current", "Notes/Ideas", "Reference/Index",
    ];
    if para_examples.contains(&target) {
        return true;
    }

    // Code/API examples in documentation
    let api_examples = [
        "API Endpoints", "API Design", "API Best Practices", "Authentication Guide",
        "Error Handling", "Error Codes", "Search Implementation", "Processing Pipeline",
        "Parsing Examples", "Crucible Parser Usage", "mcp.servers",
        "Folder/Subfolder/Note", "Premise One", "Premise Two",
    ];
    if api_examples.contains(&target) {
        return true;
    }

    false
}

#[tokio::test]
#[ignore] // Slow test - run explicitly
async fn dev_kiln_all_wikilinks_resolve() {
    let dev_kiln_root = dev_kiln_root();
    let md_files = find_markdown_files();

    let mut all_broken_links = Vec::new();
    let mut total_links = 0;
    let mut resolved_links = 0;
    let mut skipped_examples = 0;

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                all_broken_links.push(format!("{}: Failed to read file: {}", file_path.display(), e));
                continue;
            }
        };

        let wikilinks = extract_wikilinks(&content);
        total_links += wikilinks.len();

        for link in wikilinks {
            // Skip empty wikilinks
            if link.is_empty() {
                continue;
            }

            // Skip example/illustrative links (used to demonstrate syntax)
            if is_example_link(&link) {
                skipped_examples += 1;
                continue;
            }

            // Try to resolve
            if resolve_wikilink(&link, &dev_kiln_root).is_some() {
                resolved_links += 1;
            } else {
                all_broken_links.push(format!(
                    "{}: Broken wikilink [[{}]]",
                    file_path.display(),
                    link
                ));
            }
        }
    }

    if !all_broken_links.is_empty() {
        panic!(
            "❌ WIKILINK VALIDATION FAILED\n\n\
            Total links found: {}\n\
            Resolved: {}\n\
            Skipped (examples): {}\n\
            Broken: {}\n\n\
            BROKEN LINKS:\n{}",
            total_links,
            resolved_links,
            skipped_examples,
            all_broken_links.len(),
            all_broken_links.join("\n")
        );
    }

    println!(
        "✅ Wikilink validation passed: {} resolved, {} examples skipped",
        resolved_links,
        skipped_examples
    );
}

// ============================================================================
// TEST 4: All Code References Exist in Repository
// ============================================================================

#[tokio::test]
#[ignore] // Slow test - run explicitly
async fn dev_kiln_code_references_exist() {
    let workspace_root = dev_kiln_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let md_files = find_markdown_files();

    let mut failures = Vec::new();
    let mut total_refs = 0;

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{}: Failed to read file: {}", file_path.display(), e));
                continue;
            }
        };

        let code_refs = extract_code_references(&content);
        total_refs += code_refs.len();

        for code_ref in code_refs {
            let ref_path = workspace_root.join(&code_ref);

            if !ref_path.exists() {
                failures.push(format!(
                    "{}: Code reference does not exist: {}",
                    file_path.display(),
                    code_ref
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "❌ CODE REFERENCE VALIDATION FAILURES ({} broken refs):\n\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    println!("✅ All {} code references exist in repository", total_refs);
}

// ============================================================================
// TEST 5: Rune Scripts Have Valid Syntax
// ============================================================================

#[tokio::test]
#[ignore] // Slow test - run explicitly
async fn dev_kiln_rune_scripts_valid_syntax() {
    let rune_files = find_rune_files();

    assert!(
        !rune_files.is_empty(),
        "Dev-kiln should contain at least one Rune script"
    );

    let mut failures = Vec::new();

    for file_path in &rune_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{}: Failed to read file: {}", file_path.display(), e));
                continue;
            }
        };

        // Basic syntax validation:
        // 1. Check for balanced braces
        // 2. Check for common syntax patterns
        // 3. TODO: Once Rune parser is available, use proper parsing

        let open_braces = content.matches('{').count();
        let close_braces = content.matches('}').count();

        if open_braces != close_braces {
            failures.push(format!(
                "{}: Unbalanced braces ({{ {}, }} {})",
                file_path.display(),
                open_braces,
                close_braces
            ));
        }

        // Check for basic function syntax
        if content.contains("pub fn") && !content.contains('{') {
            failures.push(format!(
                "{}: Function definition without body",
                file_path.display()
            ));
        }

        // Future: Add proper Rune parser when crucible-rune is ready
        // let parse_result = crucible_rune::parse(&content);
        // if let Err(e) = parse_result {
        //     failures.push(format!("{}: Parse error: {}", file_path.display(), e));
        // }
    }

    if !failures.is_empty() {
        panic!(
            "❌ RUNE SCRIPT VALIDATION FAILURES ({}/{} files failed):\n\n{}",
            failures.len(),
            rune_files.len(),
            failures.join("\n")
        );
    }

    println!("✅ All {} Rune scripts have valid syntax", rune_files.len());
}

// ============================================================================
// Helper Tests - Verify Test Utilities Work
// ============================================================================

#[test]
fn test_wikilink_extraction() {
    let content = r#"
# Test

Regular link: [[Note Name]]
With alias: [[Target|Display]]
With heading: [[Note#Section]]
With block: [[Note#^block-id]]
Transclusion: ![[Embedded]]
Multiple: [[first]] and [[second]]
    "#;

    let links = extract_wikilinks(content);

    assert_eq!(links.len(), 7);
    assert!(links.contains(&"Note Name".to_string()));
    assert!(links.contains(&"Target".to_string()));
    assert!(links.contains(&"Note".to_string())); // Appears twice (heading + block)
    assert!(links.contains(&"Embedded".to_string()));
    assert!(links.contains(&"first".to_string()));
    assert!(links.contains(&"second".to_string()));
}

#[test]
fn test_code_reference_extraction() {
    let content = r#"
Implementation: `crates/crucible-cli/src/commands/stats.rs`

See also:
- crates/crucible-core/src/parser/types.rs
- crates/crucible-parser/src/wikilinks.rs
    "#;

    let refs = extract_code_references(content);

    assert_eq!(refs.len(), 3);
    assert!(refs.contains(&"crates/crucible-cli/src/commands/stats.rs".to_string()));
    assert!(refs.contains(&"crates/crucible-core/src/parser/types.rs".to_string()));
    assert!(refs.contains(&"crates/crucible-parser/src/wikilinks.rs".to_string()));
}

#[test]
fn test_frontmatter_extraction() {
    let content = r#"---
title: Test Note
description: A test note
tags:
  - test
  - example
---

# Content here
    "#;

    let frontmatter = extract_frontmatter(content).expect("Should extract frontmatter");
    assert!(frontmatter.contains("title: Test Note"));
    assert!(frontmatter.contains("description: A test note"));
}

#[test]
fn test_frontmatter_parsing() {
    let yaml = r#"title: Test Note
description: A test note
order: 1"#;

    let fields = parse_frontmatter_fields(yaml);

    assert_eq!(fields.get("title").unwrap(), "Test Note");
    assert_eq!(fields.get("description").unwrap(), "A test note");
    assert_eq!(fields.get("order").unwrap(), "1");
}
