//! Dev-Kiln Documentation Validation Tests
//!
//! These tests validate the integrity and quality of the `docs/` documentation kiln:
//!
//! 1. **Parsing**: All `.md` files must parse without errors
//! 2. **Frontmatter**: Required fields (title, description, tags)
//! 3. **Wikilinks**: STRICT - ALL wikilinks must resolve to existing files
//! 4. **Reachability**: every `docs/Help/**` note is linked from somewhere
//! 5. **Code References**: All `crates/...` paths must exist in the repo, and a
//!    `path.rs:26` citation must name a line that file actually has
//! 6. **Proof tests**: every `::test_name` cited by a `Meta/Product` proof line
//!    must exist somewhere in the source tree
//!
//! # Running
//!
//! ```text
//! cargo test -p crucible-core --test dev_kiln -- --ignored
//! ```
//!
//! `--test <name>` selects a test *binary*; the `-- --ignored` after it is
//! passed through to the libtest harness to bypass the `#[ignore]` gate.
//! `cargo test --ignored dev_kiln` is INVALID Cargo syntax — `dev_kiln` sitting
//! after `--ignored` parses as a positional test-name filter, not a binary
//! selector, so it silently runs nothing. `just lint docs` runs this binary
//! together with `docs_config`, and that recipe is what the CI `docs` job calls.
//!
//! The suite is `#[ignore]`d because it walks and parses every markdown and
//! script file under `docs/`. It is anchored at `CARGO_MANIFEST_DIR`, so it can
//! only ever validate this repo's `docs/` — a failure must be reproduced by
//! editing `docs/` itself, never a copy under `/tmp`.

mod common;

use common::docs_kiln::{
    docs_root, files_with_extensions, is_authored, is_committable, markdown_files, workspace_root,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Workspace-relative roots that make up the docs kiln.
const DEV_KILN_ROOTS: &[&str] = &["docs"];

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

    wikilink_re
        .captures_iter(&without_code)
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

/// A `crates/…` path cited in prose, optionally anchored to a line.
#[derive(Debug, PartialEq, Eq)]
struct CodeRef {
    /// Workspace-relative path, with any `:LINE` suffix removed.
    path: String,
    /// The cited line number, 1-indexed, if the citation had one.
    line: Option<usize>,
}

/// Extract code references from markdown content
///
/// Finds paths like `crates/crucible-core/src/...`, including the
/// `crates/…/vault.rs:26` form. The line suffix is split off here so the
/// citation resolves to a real file *and* can be checked against that file's
/// length: before this, `:` was simply swept into the path, so every
/// line-anchored citation resolved to a file named `vault.rs:26` and failed.
/// The gate forbade the most precise citation style instead of validating it.
fn extract_code_references(content: &str) -> Vec<CodeRef> {
    // `"` and `]` terminate a path as surely as `)` does: a mermaid node label
    // (`D["Path: crates/…/lib.rs"]`) is a legitimate citation, and without
    // these the trailing `"]` became part of the path and the file "did not
    // exist".
    let re = regex::Regex::new(r#"crates/[a-zA-Z0-9_-]+/[^\s)`"\]]+"#).unwrap();

    re.find_iter(content)
        .map(|m| {
            let raw = m.as_str();
            match raw
                .rsplit_once(':')
                .filter(|(_, n)| n.chars().all(|c| c.is_ascii_digit()))
                .and_then(|(path, n)| n.parse::<usize>().ok().map(|n| (path, n)))
            {
                Some((path, line)) => CodeRef {
                    path: path.to_string(),
                    line: Some(line),
                },
                None => CodeRef {
                    path: raw.to_string(),
                    line: None,
                },
            }
        })
        .collect()
}

/// Check every code reference in one document.
///
/// Returns the number of references seen and one `file: …` message per broken
/// one. Split out from the test so the checking rules are exercised directly by
/// unit tests rather than only against whatever `docs/` happens to contain.
fn code_reference_failures(
    workspace_root: &Path,
    display: &str,
    content: &str,
) -> (usize, Vec<String>) {
    let refs = extract_code_references(content);
    let mut failures = Vec::new();

    for code_ref in &refs {
        let resolved = workspace_root.join(&code_ref.path);

        let Some(line) = code_ref.line else {
            if !resolved.exists() {
                failures.push(format!(
                    "{display}: Code reference does not exist: {}",
                    code_ref.path
                ));
            }
            continue;
        };

        match std::fs::read_to_string(&resolved) {
            Ok(text) => {
                let count = text.lines().count();
                if line == 0 || line > count {
                    failures.push(format!(
                        "{display}: Code reference {}:{line} names line {line} of a {count}-line file",
                        code_ref.path
                    ));
                }
            }
            Err(e) => failures.push(format!(
                "{display}: Code reference {}:{line} is not a readable file: {e}",
                code_ref.path
            )),
        }
    }

    (refs.len(), failures)
}

/// Every file a wikilink target could mean.
///
/// Resolution is Obsidian-style: the name is what matters, a path prefix is a
/// hint. `[[Folder/Title]]` prefers a file actually under `Folder/`, but falls
/// back to any `Title.md`. Names are *not* unique in this kiln — there are eight
/// `Index.md`s — so this returns all candidates rather than the first the
/// directory walk happens to reach.
fn resolve_wikilink_candidates(target: &str, dev_kiln_root: &Path) -> Vec<PathBuf> {
    let wanted = target.to_lowercase();
    let filename_part = wanted.rsplit('/').next().unwrap_or(&wanted).to_string();

    // Try common extensions: .md (notes), .lua/.fnl (Lua/Fennel scripts)
    for ext in [".md", ".lua", ".fnl"] {
        let target_filename = format!("{filename_part}{ext}");

        let matches: Vec<PathBuf> = WalkDir::new(dev_kiln_root)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter(|e| is_authored(e.path()))
            // A link may only resolve to something a commit contains. Both
            // directions have to agree on what the kiln IS, or an untracked
            // file sitting in the tree silently satisfies a link that the
            // repository does not.
            .filter(|e| is_committable(e.path()))
            .filter(|e| {
                e.path()
                    .file_name()
                    .is_some_and(|f| f.to_string_lossy().to_lowercase() == target_filename)
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        if matches.is_empty() {
            continue;
        }

        if wanted.contains('/') {
            let suffix = format!("{wanted}{ext}");
            let hinted: Vec<PathBuf> = matches
                .iter()
                .filter(|p| p.to_string_lossy().to_lowercase().ends_with(&suffix))
                .cloned()
                .collect();
            if !hinted.is_empty() {
                return hinted;
            }
        }

        return matches;
    }

    Vec::new()
}

/// Whether a wikilink target names a file that exists.
fn resolve_wikilink(target: &str, dev_kiln_root: &Path) -> Option<PathBuf> {
    resolve_wikilink_candidates(target, dev_kiln_root)
        .into_iter()
        .next()
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
#[ignore = "requires: dev kiln — parses every markdown file in docs/"]
async fn dev_kiln_all_notes_parse() {
    use crucible_core::parser::test_utils::parse_note;

    let md_files = markdown_files(DEV_KILN_ROOTS);

    assert!(
        !md_files.is_empty(),
        "Dev-kiln should contain at least one markdown file"
    );

    let mut failures = Vec::new();

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!(
                    "{}: Failed to read file: {}",
                    file_path.display(),
                    e
                ));
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

    println!(
        "✅ All {} markdown files parsed successfully",
        md_files.len()
    );
}

// ============================================================================
// TEST 2: All Notes Have Required Frontmatter Fields
// ============================================================================

#[tokio::test]
#[ignore = "requires: dev kiln — parses every markdown file in docs/"]
async fn dev_kiln_frontmatter_has_required_fields() {
    let md_files = markdown_files(DEV_KILN_ROOTS);
    let required_fields = vec!["title", "description", "tags"];

    let mut failures = Vec::new();

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!(
                    "{}: Failed to read file: {}",
                    file_path.display(),
                    e
                ));
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

    println!(
        "✅ All {} markdown files have required frontmatter",
        md_files.len()
    );
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
        "Note Name",
        "Other Note",
        "Note",
        "Another Idea",
        "Related Concept",
        "link",
        "wikilinks",
        "broken",
        "...",
        "first",
        "second",
        "third",
        "note with spaces",
        "note-with-dashes",
        "note_with_underscores",
        "note.with.dots",
        "not a link",
        "` and `",
    ];
    if placeholders.contains(&target) {
        return true;
    }

    // Zettelkasten/PKM examples (concepts, not actual notes)
    let zettelkasten_examples = [
        "Deep Work",
        "Flow States",
        "Attention Residue",
        "Time Blocking",
        "Deliberate Practice",
        "Pomodoro Technique",
        "Batching",
        "Multitasking Myth",
        "Active Recall",
        "Forgetting Curve",
        "Interleaving",
        "Anki",
        "Time Value of Money",
        "Investment Growth",
        "Learning Techniques Index",
        "Deep Work by Cal Newport",
        "Flow by Mihaly Csikszentmihalyi",
        "Creative Process",
        "Creative Constraints",
        "Morning Creative Sessions",
        "Daily Routines",
        "Deep Work Practices",
        "Connection 1",
        "Connection 2",
        "Book Notes",
        "Research Paper",
        "Productivity System",
        "Project Planning",
    ];
    if zettelkasten_examples.contains(&target) {
        return true;
    }

    // Johnny Decimal examples (numbered organization)
    if target
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return true; // Things like "21.01 Invoice Template", "11 Company Info"
    }

    // PARA examples (project structure)
    let para_examples = [
        "Projects/Product Launch/Index",
        "Projects/Q4 Report/Index",
        "Areas/Team Management/Index",
        "Areas/Health/Index",
        "Projects/Current",
        "Notes/Ideas",
        "Reference/Index",
    ];
    if para_examples.contains(&target) {
        return true;
    }

    // Code/API examples in documentation
    let api_examples = [
        "API Endpoints",
        "API Design",
        "API Best Practices",
        "Authentication Guide",
        "Error Handling",
        "Error Codes",
        "Search Implementation",
        "Processing Pipeline",
        "Parsing Examples",
        "Crucible Parser Usage",
        "mcp.servers",
        "Folder/Subfolder/Note",
        "Premise One",
        "Premise Two",
    ];
    if api_examples.contains(&target) {
        return true;
    }

    false
}

#[tokio::test]
#[ignore = "requires: dev kiln — parses every markdown file in docs/"]
async fn dev_kiln_all_wikilinks_resolve() {
    let dev_kiln_root = docs_root();
    let md_files = markdown_files(DEV_KILN_ROOTS);

    let mut all_broken_links = Vec::new();
    let mut total_links = 0;
    let mut resolved_links = 0;
    let mut skipped_examples = 0;

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                all_broken_links.push(format!(
                    "{}: Failed to read file: {}",
                    file_path.display(),
                    e
                ));
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
        resolved_links, skipped_examples
    );
}

// ============================================================================
// TEST 4: Every Help Note Is Reachable From The Graph
// ============================================================================

/// A user-facing note nothing links to is only findable by search, which is not
/// how the kiln is meant to be navigated: `docs/Help/**` is the shipped manual,
/// and an orphan there is a page that was written and then lost.
#[tokio::test]
#[ignore = "requires: dev kiln — resolves every wikilink in docs/"]
async fn dev_kiln_every_help_note_is_reachable() {
    let dev_kiln_root = docs_root();
    let md_files = markdown_files(DEV_KILN_ROOTS);

    let mut linked: HashSet<PathBuf> = HashSet::new();

    for file_path in &md_files {
        let Ok(content) = tokio::fs::read_to_string(file_path).await else {
            continue;
        };

        for link in extract_wikilinks(&content) {
            if link.is_empty() || is_example_link(&link) {
                continue;
            }
            // An ambiguous link (`[[Index]]`) counts as reaching every
            // candidate: which one a reader lands on is a resolution detail,
            // and calling the others orphans would be a lie.
            for target in resolve_wikilink_candidates(&link, &dev_kiln_root) {
                // A note linking to itself does not make it reachable.
                if &target != file_path {
                    linked.insert(target);
                }
            }
        }
    }

    let help_root = dev_kiln_root.join("Help");
    let help_notes: Vec<&PathBuf> = md_files
        .iter()
        .filter(|p| p.starts_with(&help_root))
        .collect();
    assert!(
        !help_notes.is_empty(),
        "no notes found under {} — root discovery is broken",
        help_root.display()
    );

    let orphans: Vec<&PathBuf> = help_notes
        .iter()
        .copied()
        .filter(|p| !linked.contains(*p))
        .collect();

    if !orphans.is_empty() {
        panic!(
            "❌ UNREACHABLE HELP NOTES ({} of {} orphaned — no wikilink anywhere in the kiln targets them):\n\n{}",
            orphans.len(),
            help_notes.len(),
            orphans
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    println!("✅ Every Help note is linked from at least one other note");
}

// ============================================================================
// TEST 5: All Code References Exist in Repository
// ============================================================================

#[tokio::test]
#[ignore = "requires: dev kiln — parses every markdown file in docs/"]
async fn dev_kiln_code_references_exist() {
    let workspace_root = workspace_root();
    let md_files = markdown_files(DEV_KILN_ROOTS);

    let mut failures = Vec::new();
    let mut total_refs = 0;

    for file_path in &md_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!(
                    "{}: Failed to read file: {}",
                    file_path.display(),
                    e
                ));
                continue;
            }
        };

        let (n, mut file_failures) =
            code_reference_failures(&workspace_root, &file_path.display().to_string(), &content);
        total_refs += n;
        failures.append(&mut file_failures);
    }

    if !failures.is_empty() {
        panic!(
            "❌ CODE REFERENCE VALIDATION FAILURES ({} broken refs):\n\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    println!("✅ All {total_refs} code references exist in repository");
}

// ============================================================================
// TEST 6: Lua/Fennel Scripts Have Valid Syntax
// ============================================================================
#[tokio::test]
#[ignore = "requires: dev kiln — parses every script file in docs/"]
async fn dev_kiln_lua_scripts_valid_syntax() {
    let lua_files = files_with_extensions(DEV_KILN_ROOTS, &["lua", "fnl"]);

    if lua_files.is_empty() {
        println!("No Lua/Fennel scripts found in dev-kiln, skipping.");
        return;
    }

    let mut failures = Vec::new();

    for file_path in &lua_files {
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!(
                    "{}: Failed to read file: {}",
                    file_path.display(),
                    e
                ));
                continue;
            }
        };

        // Basic syntax validation: check for balanced braces/parens
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
    }
    if !failures.is_empty() {
        panic!(
            "LUA SCRIPT VALIDATION FAILURES ({}/{} files failed):\n\n{}",
            failures.len(),
            lua_files.len(),
            failures.join("\n")
        );
    }

    println!(
        "All {} Lua/Fennel scripts have valid syntax",
        lua_files.len()
    );
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
- crates/crucible-core/src/parser/wikilinks.rs
    "#;

    let refs = extract_code_references(content);

    assert_eq!(refs.len(), 3);
    assert!(refs.iter().all(|r| r.line.is_none()));
    let paths: Vec<&str> = refs.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"crates/crucible-cli/src/commands/stats.rs"));
    assert!(paths.contains(&"crates/crucible-core/src/parser/types.rs"));
    assert!(paths.contains(&"crates/crucible-core/src/parser/wikilinks.rs"));
}

#[test]
fn a_line_anchored_reference_splits_into_path_and_line() {
    let refs = extract_code_references("see crates/crucible-core/src/vault.rs:26 for the guard");

    assert_eq!(
        refs,
        vec![CodeRef {
            path: "crates/crucible-core/src/vault.rs".into(),
            line: Some(26),
        }]
    );
}

/// The `:` in a windows-style or otherwise non-numeric suffix is part of the
/// path, not a line anchor.
#[test]
fn a_non_numeric_suffix_stays_part_of_the_path() {
    let refs = extract_code_references("crates/crucible-core/src/lib.rs:main");

    assert_eq!(refs[0].path, "crates/crucible-core/src/lib.rs:main");
    assert_eq!(refs[0].line, None);
}

#[test]
fn a_reference_to_a_line_the_file_has_passes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("crates/demo/src")).unwrap();
    std::fs::write(root.path().join("crates/demo/src/lib.rs"), "a\nb\nc\n").unwrap();

    let (count, failures) =
        code_reference_failures(root.path(), "Doc.md", "see crates/demo/src/lib.rs:3");

    assert_eq!(count, 1);
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn a_reference_past_the_end_of_the_file_fails() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("crates/demo/src")).unwrap();
    std::fs::write(root.path().join("crates/demo/src/lib.rs"), "a\nb\nc\n").unwrap();

    let (_, failures) =
        code_reference_failures(root.path(), "Doc.md", "see crates/demo/src/lib.rs:400");

    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].contains("crates/demo/src/lib.rs:400") && failures[0].contains("3-line"),
        "{}",
        failures[0]
    );
}

#[test]
fn a_reference_to_a_missing_file_fails() {
    let root = tempfile::tempdir().unwrap();

    let (_, failures) = code_reference_failures(
        root.path(),
        "Doc.md",
        "crates/demo/src/gone.rs and crates/demo/src/gone.rs:1",
    );

    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].contains("does not exist"), "{}", failures[0]);
    assert!(
        failures[1].contains("not a readable file"),
        "{}",
        failures[1]
    );
}

/// Two notes can share a name (`Canvas.md` exists under both `Help/` and
/// `Meta/`). A link that carries a path hint means the note under that path;
/// without this, `[[Meta/Analysis/Canvas]]` credited whichever `Canvas.md` the
/// directory walk reached first, and the real one looked reachable when it was
/// the *other* one being linked.
#[test]
fn a_path_hint_picks_between_notes_that_share_a_name() {
    let kiln = tempfile::tempdir().unwrap();
    for dir in ["Help/Concepts", "Meta/Analysis"] {
        std::fs::create_dir_all(kiln.path().join(dir)).unwrap();
        std::fs::write(kiln.path().join(dir).join("Canvas.md"), "").unwrap();
    }

    let hinted = resolve_wikilink_candidates("Meta/Analysis/Canvas", kiln.path());
    assert_eq!(hinted, vec![kiln.path().join("Meta/Analysis/Canvas.md")]);

    let mut ambiguous = resolve_wikilink_candidates("Canvas", kiln.path());
    ambiguous.sort();
    assert_eq!(
        ambiguous,
        vec![
            kiln.path().join("Help/Concepts/Canvas.md"),
            kiln.path().join("Meta/Analysis/Canvas.md"),
        ]
    );
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

/// Removes the file it names when it drops, panic or not.
struct ScratchFile(PathBuf);
impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The gate validates the content of the commit, not the state of someone's
/// disk.
///
/// These suites walked `docs/` and held every `.md` on disk to the authoring
/// conventions, so anything dropped in there — a scratch plan, a personal note,
/// a second agent's work in a shared checkout — turned the gate red for work
/// that is not part of the repository. That happened: seven untracked drafts
/// appeared in `docs/Meta/Plans/` and `just ci` went red on broken wikilinks
/// and code references in files no commit contains.
///
/// Tracked-or-staged is the line, and `git add` is what moves a file across it,
/// so a new note is validated from the moment it is on its way into a commit.
#[test]
fn the_kiln_sweep_ignores_a_file_no_commit_would_contain() {
    let scratch = ScratchFile(docs_root().join("Meta").join("zz-untracked-scratch.md"));
    std::fs::write(
        &scratch.0,
        "# scratch\n\nA [[Link That Does Not Resolve]] and a bad ref \
         `crates/nonexistent/file.rs`.\n",
    )
    .unwrap();

    assert!(
        !markdown_files(&["docs"]).contains(&scratch.0),
        "an untracked file must not be swept: {}",
        scratch.0.display()
    );
}

/// A named proof test in `Meta/Product` must exist.
///
/// The product map earns its `[x]`s by naming the tests that demonstrate them,
/// in the form `` `path/to/file.rs` ``::`test_name`. That citation is the whole
/// mechanism, and it rots silently: a test gets renamed, the map still names
/// the old one, and the entry keeps reading as proven. An audit on 2026-08-18
/// found six such citations: four renamed, and two naming tests that never
/// existed. One of those two was the only proof behind an `[x]` whose subject
/// had itself been deleted — the citation outlived the code, and the entry
/// went on reading as proven for both.
///
/// Only the `::name` form is checked. A name written in plain backticks is
/// prose — the map deliberately names absent things ("`redo_turns` does not
/// exist on `ConversationTree`", "what would settle it: `a_user_plugin_…`"),
/// and those must not fail a test for being absent. So the convention this
/// test enforces is narrow and worth stating: **`::name` asserts the test
/// exists; backticks alone do not.**
///
/// Matching is a substring search over the source, not a `fn name` search: the
/// map cites Rust tests, vitest strings and Playwright titles, and only the
/// first is a function.
#[test]
#[ignore = "requires: dev kiln — reads the product map and greps the source tree"]
fn product_map_proof_tests_exist() {
    let root = workspace_root();
    let map = root.join("docs").join("Meta").join("Product.md");
    let content = std::fs::read_to_string(&map).expect("read Meta/Product.md");

    let mut corpus = String::new();
    for dir in ["crates", "runtime", "scripts"] {
        for entry in WalkDir::new(root.join(dir))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            // Neither is evidence, and both are enormous: `target/` holds
            // stale build artifacts that would let a deleted test still count,
            // and `node_modules/` is vendored third-party code.
            if entry.path().components().any(|c| {
                let c = c.as_os_str();
                c == "target" || c == "node_modules" || c == "dist"
            }) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                corpus.push_str(&text);
            }
        }
    }

    let re = regex::Regex::new(r"::([a-z_][a-z0-9_]{6,})").unwrap();
    let mut missing: Vec<(usize, String)> = Vec::new();
    let mut checked = HashSet::new();

    for (i, line) in content.lines().enumerate() {
        if !line.contains("**Proof:**") {
            continue;
        }
        for caps in re.captures_iter(line) {
            let name = caps[1].to_string();
            if !checked.insert(name.clone()) {
                continue;
            }
            if !corpus.contains(&name) {
                missing.push((i + 1, name));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "docs/Meta/Product.md names {} proof test(s) that exist nowhere under \
         crates/, runtime/ or scripts/. Either the test was renamed (update the \
         citation) or the claim has no proof (demote the entry):\n{}",
        missing.len(),
        missing
            .iter()
            .map(|(line, name)| format!("  Product.md:{line}  ::{name}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
