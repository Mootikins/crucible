//! Test fixtures for Crucible tests
//!
//! Provides unified fixture-based API for creating test kilns.

use anyhow::{Context, Result};
use tempfile::TempDir;

/// Fixture types for different test scenarios
pub enum KilnFixture<'a> {
    /// 5 general markdown documents
    Basic,

    /// MoCs with linked content, tags, wikilinks
    Clustering {
        moc_count: usize,
        content_count: usize,
        links_per_moc: usize,
    },

    /// Multi-level hierarchy with cross-references
    Complex { domains: usize, depth: usize },

    /// Custom files for specific test scenarios
    Custom { files: Vec<(&'a str, &'a str)> },
}

impl KilnFixture<'_> {
    /// Get a human-readable name for this fixture
    pub fn name(&self) -> &str {
        match self {
            KilnFixture::Basic => "basic",
            KilnFixture::Clustering { .. } => "clustering",
            KilnFixture::Complex { .. } => "complex",
            KilnFixture::Custom { .. } => "custom",
        }
    }

    /// Estimate the number of files this fixture will create
    pub fn estimated_file_count(&self) -> usize {
        match self {
            KilnFixture::Basic => 5,
            KilnFixture::Clustering {
                moc_count,
                content_count,
                ..
            } => moc_count + content_count,
            KilnFixture::Complex { domains, depth } => {
                let mut total = 0;
                let mut level_count = *domains;
                for _ in 0..*depth {
                    total += level_count;
                    level_count *= 3;
                }
                total.min(100)
            }
            KilnFixture::Custom { files } => files.len(),
        }
    }
}

/// Create a test kiln using fixture configuration
pub fn create_kiln(fixture: KilnFixture) -> Result<TempDir> {
    match fixture {
        KilnFixture::Basic => create_basic_kiln(),
        KilnFixture::Clustering {
            moc_count,
            content_count,
            links_per_moc,
        } => create_clustering_kiln(moc_count, content_count, links_per_moc),
        KilnFixture::Complex { domains, depth } => create_complex_kiln(domains, depth),
        KilnFixture::Custom { files } => create_kiln_with_files(&files),
    }
}

fn create_basic_kiln() -> Result<TempDir> {
    create_kiln_with_files(&[
        (
            "Getting Started.md",
            "# Getting Started\n\nThis is a getting started guide for the kiln.",
        ),
        (
            "Project Architecture.md",
            "# Project Architecture\n\nThis note describes the architecture.",
        ),
        ("Testing Notes.md", "# Testing\n\nSome testing notes here."),
        ("README.md", "# README\n\nThis is the main README file."),
        (
            "Development.md",
            "# Development\n\nDevelopment documentation.",
        ),
    ])
}

fn create_clustering_kiln(
    moc_count: usize,
    content_count: usize,
    links_per_moc: usize,
) -> Result<TempDir> {
    let mut files: Vec<(String, String)> = Vec::new();

    for i in 0..moc_count {
        let moc_name = format!("MoC {}", i);
        let mut moc_content = format!("# {}\n\n## Content\n", moc_name);

        for j in 0..links_per_moc.min(content_count) {
            moc_content.push_str(&format!("- [[Content {}]]\n", j));
        }

        files.push((format!("moc_{}.md", i), moc_content));
    }

    for i in 0..content_count {
        let moc_index = i % moc_count.max(1);
        let category = i % 3;
        let content = format!(
            r#"---
tags: [content, category_{}]
---

# Content {}

This is content document number {}.

Linked from [[moc_{}.md]]
"#,
            category, i, i, moc_index
        );
        files.push((format!("content_{}.md", i), content));
    }

    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    create_kiln_with_files(&files)
}

fn create_complex_kiln(domains: usize, _depth: usize) -> Result<TempDir> {
    let mut files: Vec<(String, String)> = Vec::new();

    for d in 0..domains {
        let domain_name = format!("Domain {}", d);
        let domain_dir = format!("Domain_{}", d);
        let mut domain_content = format!("# {}\n\n## Areas\n", domain_name);

        let areas_per_domain = 3_usize;
        for a in 0..areas_per_domain {
            let area_name = format!("{} Area {}", domain_name, a);
            domain_content.push_str(&format!("- [[{}]]\n", area_name));

            let topics_per_area = 2_usize;
            for t in 0..topics_per_area {
                let topic_name = format!("{} Topic {}", area_name, t);
                let topic_file = format!("{}/{}.md", domain_dir, topic_name.replace(' ', "_"));
                let topic_content = format!("# {}\n\nTopic content.", topic_name);
                files.push((topic_file, topic_content));
            }

            let area_file = format!("{}/{}.md", domain_dir, area_name.replace(' ', "_"));
            let area_content = format!(
                "# {}\n\n## Topics\n{}",
                area_name,
                (0..topics_per_area)
                    .map(|t| format!("- [[{} Topic {}]]", area_name, t))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            files.push((area_file, area_content));
        }

        let domain_file = format!("{}.md", domain_dir);
        files.push((domain_file, domain_content));
    }

    let mut hub_content = String::from("# Knowledge Hub\n\n## Domains\n");
    for d in 0..domains {
        hub_content.push_str(&format!("- [[Domain {}]]\n", d));
    }
    files.push(("Hub.md".to_string(), hub_content));

    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    create_kiln_with_files(&files)
}

fn create_kiln_with_files(files: &[(&str, &str)]) -> Result<TempDir> {
    let temp_dir = TempDir::new().context("failed to create temporary kiln directory")?;
    let kiln_path = temp_dir.path();

    for (relative_path, contents) in files {
        let file_path = kiln_path.join(relative_path);

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create kiln subdirectory {:?}", parent.display())
            })?;
        }

        std::fs::write(&file_path, contents)
            .with_context(|| format!("failed to write kiln file {:?}", file_path.display()))?;
    }

    Ok(temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_basic_has_correct_name() {
        assert_eq!(KilnFixture::Basic.name(), "basic");
        assert_eq!(
            KilnFixture::Clustering {
                moc_count: 2,
                content_count: 5,
                links_per_moc: 3
            }
            .name(),
            "clustering"
        );
    }

    #[test]
    fn fixture_basic_estimates_file_count() {
        assert_eq!(KilnFixture::Basic.estimated_file_count(), 5);
        assert_eq!(
            KilnFixture::Clustering {
                moc_count: 2,
                content_count: 5,
                links_per_moc: 3
            }
            .estimated_file_count(),
            7
        );
        assert_eq!(
            KilnFixture::Custom {
                files: vec![("a.md", ""), ("b.md", "")]
            }
            .estimated_file_count(),
            2
        );
    }

    #[test]
    fn create_basic_kiln_creates_five_files() {
        let kiln = create_kiln(KilnFixture::Basic).unwrap();
        let entries: Vec<_> = kiln
            .path()
            .read_dir()
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn create_clustering_kiln_creates_expected_files() {
        let kiln = create_kiln(KilnFixture::Clustering {
            moc_count: 2,
            content_count: 3,
            links_per_moc: 2,
        })
        .unwrap();

        let entries: Vec<_> = kiln.path().read_dir().unwrap().collect();
        assert_eq!(entries.len(), 5); // 2 MoCs + 3 content

        // Verify MoC files exist
        assert!(kiln.path().join("moc_0.md").exists());
        assert!(kiln.path().join("moc_1.md").exists());

        // Verify content files exist
        assert!(kiln.path().join("content_0.md").exists());
        assert!(kiln.path().join("content_1.md").exists());
        assert!(kiln.path().join("content_2.md").exists());
    }

    #[test]
    fn create_custom_kiln_creates_directories() {
        let kiln = create_kiln(KilnFixture::Custom {
            files: vec![
                ("root.md", "# Root"),
                ("nested/file.md", "# Nested"),
                ("deeply/nested/path.md", "# Deep"),
            ],
        })
        .unwrap();

        assert!(kiln.path().join("root.md").exists());
        assert!(kiln.path().join("nested/file.md").exists());
        assert!(kiln.path().join("deeply/nested/path.md").exists());
    }

    #[test]
    fn create_complex_kiln_creates_hierarchy() {
        let kiln = create_kiln(KilnFixture::Complex {
            domains: 2,
            depth: 2,
        })
        .unwrap();

        // Verify hub exists
        assert!(kiln.path().join("Hub.md").exists());

        // Verify domain structure
        assert!(kiln.path().join("Domain_0.md").exists());
        assert!(kiln.path().join("Domain_0/Domain_0_Area_0.md").exists());
    }
}
