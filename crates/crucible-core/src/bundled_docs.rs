//! Crucible's own help corpus, compiled into the binary.
//!
//! # Why it ships, and why only part of it
//!
//! Same packaging reality as [`crate::runtime_roots`]: `cargo-dist`'s generated
//! shell installer moves only binaries and libraries out of the unpacked
//! archive and deletes the rest, and `cargo install` has no data path at all.
//! A `<prefix>/share/crucible/docs` would be correct and empty. So the corpus
//! travels inside the binary or it does not reach anyone who installed rather
//! than cloned.
//!
//! Only `Help/` and `Guides/` — 704K of the repo's 2.1M `docs/`. The rest is
//! `Meta/`: the roadmap, the decision log, planning notes. Excluding it is not
//! only a size decision. An agent answering "how do I configure X" should not
//! be retrieving the maintainer's roadmap, and a corpus that contains both
//! competes with itself on exactly the queries help is for.
//!
//! # Why it is a kiln and not a bespoke index
//!
//! `docs/` already *is* a kiln — valid frontmatter, wikilinks, parsed by the
//! integration tests, published as the website. Making the shipped copy
//! anything else would mean a second source of documented truth that has to be
//! kept in step with the first. The extracted tree is registered as a kiln and
//! read with the same tools as any other.
//!
//! It is never auto-mounted. Nothing joins a session's retrieval unless the
//! user connects it — the same rule everything else in Crucible follows.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::runtime_roots::{embedded_stamp, write_embedded_tree, STAMP_FILE};

/// The help corpus as shipped.
///
/// `debug-embed` is inherited from the crate's feature selection for the same
/// reason it is on for the runtime tree: without it rust-embed reads the build
/// machine's source path in debug builds, so the embedded route would only ever
/// execute in release.
#[derive(rust_embed::Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../docs"]
#[include = "Help/**"]
#[include = "Guides/**"]
struct BundledDocs;

/// Where the extracted corpus lands.
///
/// A path, not a promise that anything is there. Version-stamped so two
/// installed Crucibles do not fight over one directory.
pub fn bundled_docs_dir() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("crucible")
            .join(concat!("docs-", env!("CARGO_PKG_VERSION"))),
    )
}

/// Materialise the help corpus, returning where it landed.
///
/// Memoised: repeated calls in one process are a no-op. Returns `None` when
/// there is no data directory or the write fails — a missing help corpus is a
/// degraded experience, never a startup failure.
pub fn ensure_bundled_docs() -> Option<PathBuf> {
    static MATERIALISED: OnceLock<Option<PathBuf>> = OnceLock::new();
    MATERIALISED
        .get_or_init(|| {
            let target = bundled_docs_dir()?;
            match sync_bundled_docs(&target) {
                Ok(()) => Some(target),
                Err(err) => {
                    tracing::warn!(
                        target = %target.display(),
                        error = %err,
                        "could not write the bundled help corpus"
                    );
                    None
                }
            }
        })
        .clone()
}

/// Write the corpus to `target`, skipping if this build already did.
///
/// The stamp is a claim about which build wrote the tree, not a checksum of
/// what is there now — someone who annotates their extracted copy keeps those
/// notes until the next release.
pub fn sync_bundled_docs(target: &std::path::Path) -> std::io::Result<()> {
    let stamp = embedded_stamp::<BundledDocs>();
    if std::fs::read_to_string(target.join(STAMP_FILE)).is_ok_and(|w| w == stamp) {
        return Ok(());
    }
    write_embedded_tree::<BundledDocs>(target, &stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus carries user-facing help and nothing else.
    ///
    /// Both halves matter. Shipping `Meta/` would put the roadmap and decision
    /// log into a retrieval index whose whole job is answering "how do I use
    /// this", where they compete with the answer. Shipping *no* `Help/` would
    /// be a silent 704K of nothing.
    #[test]
    fn the_corpus_is_help_and_guides_only() {
        let paths: Vec<String> = BundledDocs::iter().map(|p| p.to_string()).collect();
        assert!(!paths.is_empty(), "the help corpus must not be empty");

        for path in &paths {
            assert!(
                path.starts_with("Help/") || path.starts_with("Guides/"),
                "only Help/ and Guides/ ship; found {path}"
            );
        }

        for required in [
            "Help/Extending/Creating Plugins.md",
            "Help/Concepts/Kilns.md",
            "Guides/Your First Kiln.md",
        ] {
            assert!(
                paths.iter().any(|p| p == required),
                "the include globs dropped '{required}'"
            );
        }
    }

    /// Extraction round-trips, and the stamp suppresses a second write.
    #[test]
    fn extraction_writes_the_corpus_once() {
        let tmp = tempfile::tempdir().expect("tempdir");
        sync_bundled_docs(tmp.path()).expect("first extract");

        let note = tmp.path().join("Guides").join("Your First Kiln.md");
        assert!(note.is_file(), "a guide should be on disk");

        // A hand edit survives a second sync from the same build: the stamp is
        // a claim about which build wrote the tree, not a checksum of it.
        std::fs::write(&note, "-- annotated").expect("edit");
        sync_bundled_docs(tmp.path()).expect("second extract");
        assert_eq!(
            std::fs::read_to_string(&note).expect("read"),
            "-- annotated"
        );

        // Dropping the stamp makes the next sync restore it.
        std::fs::remove_file(tmp.path().join(STAMP_FILE)).expect("drop stamp");
        sync_bundled_docs(tmp.path()).expect("third extract");
        assert!(std::fs::read_to_string(&note).expect("read").len() > 20);
    }
}
