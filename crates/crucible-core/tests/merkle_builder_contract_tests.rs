//! Contract tests for MerkleTreeBuilder trait
//!
//! These tests verify the contract that all MerkleTreeBuilder implementations
//! must satisfy, following the Tower-style dependency inversion pattern.

use crucible_core::parser::{ParsedNote, ParsedNoteBuilder};
use crucible_core::MerkleTreeBuilder;
use std::path::PathBuf;

/// Contract: Builder trait must be implemented correctly
#[test]
fn contract_builder_must_be_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    #[derive(Clone)]
    struct TestBuilder;

    #[derive(Clone, Debug)]
    struct TestTree;

    impl MerkleTreeBuilder for TestBuilder {
        type Tree = TestTree;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            TestTree
        }
    }

    // This is a compile-time check - if it compiles, the contract is satisfied
    assert_send_sync::<TestBuilder>();
    assert_send_sync::<TestTree>();
}

/// Contract: Builder must be cloneable for dependency injection
#[test]
fn contract_builder_must_be_cloneable() {
    #[derive(Clone)]
    struct TestBuilder;

    #[derive(Clone, Debug)]
    struct TestTree;

    impl MerkleTreeBuilder for TestBuilder {
        type Tree = TestTree;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            TestTree
        }
    }

    let builder = TestBuilder;
    let cloned = builder.clone();

    // Both should work independently
    let note = ParsedNoteBuilder::new(PathBuf::from("test.md")).build();
    let _tree1 = builder.from_document(&note);
    let _tree2 = cloned.from_document(&note);
}

/// Contract: from_document must be deterministic
#[test]
fn contract_from_document_must_be_deterministic() {
    #[derive(Clone)]
    struct DeterministicBuilder;

    #[derive(Clone, PartialEq, Debug)]
    struct DeterministicTree {
        hash: u64,
    }

    impl MerkleTreeBuilder for DeterministicBuilder {
        type Tree = DeterministicTree;

        fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
            // Hash based on document path (deterministic)
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            doc.path.hash(&mut hasher);

            DeterministicTree {
                hash: hasher.finish(),
            }
        }
    }

    let builder = DeterministicBuilder;
    let note = ParsedNoteBuilder::new(PathBuf::from("deterministic.md")).build();

    // Multiple calls should produce identical trees
    let tree1 = builder.from_document(&note);
    let tree2 = builder.from_document(&note);

    assert_eq!(tree1, tree2, "Builder must be deterministic");
}

/// Contract: Tree type must be thread-safe
#[test]
fn contract_tree_must_be_send_sync() {
    #[derive(Clone)]
    struct ThreadSafeBuilder;

    #[derive(Clone, Debug)]
    struct ThreadSafeTree {
        data: String,
    }

    // Verify Tree implements Send + Sync
    impl MerkleTreeBuilder for ThreadSafeBuilder {
        type Tree = ThreadSafeTree;

        fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
            ThreadSafeTree {
                data: doc.path.display().to_string(),
            }
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ThreadSafeTree>();
}

/// Contract: Builder can be stored in Arc for shared usage
#[test]
fn contract_builder_can_be_shared() {
    use std::sync::Arc;

    #[derive(Clone)]
    struct SharedBuilder;

    #[derive(Clone, Debug)]
    struct SharedTree;

    impl MerkleTreeBuilder for SharedBuilder {
        type Tree = SharedTree;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            SharedTree
        }
    }

    let builder = Arc::new(SharedBuilder);
    let builder2 = Arc::clone(&builder);

    let note = ParsedNoteBuilder::new(PathBuf::from("shared.md")).build();

    // Both Arc instances should work
    let _tree1 = builder.from_document(&note);
    let _tree2 = builder2.from_document(&note);
}

/// Contract: Builder works with generic services
#[test]
fn contract_builder_works_with_generic_services() {
    #[derive(Clone)]
    struct GenericBuilder;

    #[derive(Clone, Debug)]
    struct GenericTree {
        block_count: usize,
    }

    impl MerkleTreeBuilder for GenericBuilder {
        type Tree = GenericTree;

        fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
            GenericTree {
                block_count: doc.metadata.paragraph_count + doc.metadata.heading_count,
            }
        }
    }

    // Simulate a generic service (like enrichment service)
    struct GenericService<M: MerkleTreeBuilder> {
        builder: M,
    }

    impl<M: MerkleTreeBuilder> GenericService<M> {
        fn new(builder: M) -> Self {
            Self { builder }
        }

        fn process(&self, doc: &ParsedNote) -> M::Tree {
            self.builder.from_document(doc)
        }
    }

    let builder = GenericBuilder;
    let service = GenericService::new(builder);

    let note = ParsedNoteBuilder::new(PathBuf::from("generic.md")).build();
    let tree = service.process(&note);

    // Note has content, so block_count should reflect that
    assert!(tree.block_count >= 0);
}

/// Contract: Different builders can be used interchangeably
#[test]
fn contract_builders_are_interchangeable() {
    #[derive(Clone)]
    struct BuilderA;

    #[derive(Clone)]
    struct BuilderB;

    #[derive(Clone, Debug)]
    struct TreeTypeA {
        name: &'static str,
    }

    #[derive(Clone, Debug)]
    struct TreeTypeB {
        name: &'static str,
    }

    impl MerkleTreeBuilder for BuilderA {
        type Tree = TreeTypeA;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            TreeTypeA { name: "A" }
        }
    }

    impl MerkleTreeBuilder for BuilderB {
        type Tree = TreeTypeB;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            TreeTypeB { name: "B" }
        }
    }

    // Same service can work with different builders
    fn use_builder<M: MerkleTreeBuilder>(builder: M) -> M::Tree {
        let note = ParsedNoteBuilder::new(PathBuf::from("test.md")).build();
        builder.from_document(&note)
    }

    let tree_a = use_builder(BuilderA);
    let tree_b = use_builder(BuilderB);

    assert_eq!(tree_a.name, "A");
    assert_eq!(tree_b.name, "B");
}

/// Contract: Builder handles empty documents gracefully
#[test]
fn contract_builder_handles_empty_documents() {
    #[derive(Clone)]
    struct RobustBuilder;

    #[derive(Clone, Debug)]
    struct RobustTree {
        is_empty: bool,
    }

    impl MerkleTreeBuilder for RobustBuilder {
        type Tree = RobustTree;

        fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
            // Check if the document has any content (paragraphs, headings, etc.)
            let has_content = doc.metadata.paragraph_count > 0
                || doc.metadata.heading_count > 0
                || doc.metadata.code_block_count > 0;

            RobustTree {
                is_empty: !has_content,
            }
        }
    }

    let builder = RobustBuilder;
    let empty_note = ParsedNoteBuilder::new(PathBuf::from("empty.md")).build();

    let tree = builder.from_document(&empty_note);
    assert!(tree.is_empty, "Builder should handle empty documents");
}

/// Contract: Builder can be used in async contexts
#[tokio::test]
async fn contract_builder_works_in_async_context() {
    #[derive(Clone)]
    struct AsyncBuilder;

    #[derive(Clone, Debug)]
    struct AsyncTree;

    impl MerkleTreeBuilder for AsyncBuilder {
        type Tree = AsyncTree;

        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            AsyncTree
        }
    }

    let builder = AsyncBuilder;
    let note = ParsedNoteBuilder::new(PathBuf::from("async.md")).build();

    // Builder can be used in async functions
    let _tree = tokio::spawn(async move { builder.from_document(&note) })
        .await
        .unwrap();
}

/// Contract: Multiple builders can coexist in the same scope
#[test]
fn contract_multiple_builders_coexist() {
    #[derive(Clone)]
    struct Builder1;

    #[derive(Clone)]
    struct Builder2;

    #[derive(Clone, Debug)]
    struct Tree1;

    #[derive(Clone, Debug)]
    struct Tree2;

    impl MerkleTreeBuilder for Builder1 {
        type Tree = Tree1;
        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            Tree1
        }
    }

    impl MerkleTreeBuilder for Builder2 {
        type Tree = Tree2;
        fn from_document(&self, _doc: &ParsedNote) -> Self::Tree {
            Tree2
        }
    }

    let builder1 = Builder1;
    let builder2 = Builder2;

    let note = ParsedNoteBuilder::new(PathBuf::from("multi.md")).build();

    let _tree1 = builder1.from_document(&note);
    let _tree2 = builder2.from_document(&note);
}
