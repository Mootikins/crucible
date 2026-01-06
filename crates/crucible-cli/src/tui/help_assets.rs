//! Embedded documentation assets for the help system.
//!
//! Uses rust-embed to bundle user-facing documentation from `docs/` at compile time.
//! Internal directories (Meta, plans, research, etc.) are excluded.

use rust_embed::Embed;

/// Embedded documentation files from the docs/ directory.
///
/// Excludes internal directories:
/// - Meta/ (contributor docs)
/// - plans/ (implementation plans)
/// - reference-prompts/ (internal prompts)
/// - research/ (research notes)
/// - plugins/ (internal plugin files)
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../docs/"]
#[exclude = "Meta/*"]
#[exclude = "plans/*"]
#[exclude = "reference-prompts/*"]
#[exclude = "research/*"]
#[exclude = "plugins/*"]
#[exclude = "*.toml"]
pub struct EmbeddedDocs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_docs_not_empty() {
        let count = EmbeddedDocs::iter().count();
        assert!(count > 0, "Expected embedded docs, found none");
    }

    #[test]
    fn excludes_meta_directory() {
        for file in EmbeddedDocs::iter() {
            assert!(
                !file.starts_with("Meta/"),
                "Meta/ should be excluded: {}",
                file
            );
        }
    }

    #[test]
    fn excludes_plans_directory() {
        for file in EmbeddedDocs::iter() {
            assert!(
                !file.starts_with("plans/"),
                "plans/ should be excluded: {}",
                file
            );
        }
    }

    #[test]
    fn includes_help_directory() {
        let has_help = EmbeddedDocs::iter().any(|f| f.starts_with("Help/"));
        assert!(has_help, "Expected Help/ directory in embedded docs");
    }

    #[test]
    fn can_read_file_content() {
        // Find any markdown file and verify we can read it
        if let Some(file) = EmbeddedDocs::iter().find(|f| f.ends_with(".md")) {
            let content = EmbeddedDocs::get(&file);
            assert!(content.is_some(), "Should be able to read {}", file);
            let data = content.unwrap();
            assert!(!data.data.is_empty(), "File {} should not be empty", file);
        }
    }
}
