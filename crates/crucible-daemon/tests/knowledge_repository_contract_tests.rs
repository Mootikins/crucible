use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::MockKnowledgeRepository;

#[tokio::test]
async fn contract_get_note_by_name_returns_none_for_missing_note() {
    let repository = MockKnowledgeRepository;

    let note = repository
        .get_note_by_name("missing-note.md")
        .await
        .expect("lookup should not fail for unknown notes");

    assert!(
        note.is_none(),
        "repositories should represent missing notes as None"
    );
}

#[tokio::test]
async fn contract_list_notes_returns_empty_when_repository_has_no_notes() {
    let repository = MockKnowledgeRepository;

    let all_notes = repository
        .list_notes(None)
        .await
        .expect("listing all notes should succeed");
    let filtered_notes = repository
        .list_notes(Some("docs/"))
        .await
        .expect("listing by path filter should succeed");

    assert!(all_notes.is_empty(), "empty repositories should list no notes");
    assert!(
        filtered_notes.is_empty(),
        "path filters should not invent notes when none exist"
    );
}

#[tokio::test]
async fn contract_search_vectors_returns_empty_for_unmatched_embeddings() {
    let repository = MockKnowledgeRepository;

    let results = repository
        .search_vectors(vec![0.13, 0.42, 0.99])
        .await
        .expect("vector search should not fail for well-formed vectors");

    assert!(
        results.is_empty(),
        "vector search should return an empty set when nothing matches"
    );
}
