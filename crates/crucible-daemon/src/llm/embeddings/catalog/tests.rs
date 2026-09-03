//! What the compiler cannot check about the catalog.
//!
//! rustc proves that every `EmbeddingModel` variant has a row. It cannot prove
//! that the row is complete, that two rows do not claim the same name, or that
//! the dimension matches the model. The expectation therefore comes from
//! `TextEmbedding::list_supported_models()` — the crate's own registry — and
//! never from a list typed into this file.

use super::*;
use std::collections::BTreeMap;

/// One walk over fastembed's registry, asserting the catalog answers for each
/// model it holds.
///
/// A model the crate adds arrives here with no edit to this test: the walk
/// comes from the running system, and `facts` fails to compile until someone
/// writes its row.
#[test]
fn the_catalog_answers_for_every_model_fastembed_exposes() {
    let registered = TextEmbedding::list_supported_models();
    assert!(!registered.is_empty(), "fastembed registers no text model");

    let entries = all();
    assert_eq!(
        entries.len(),
        registered.len(),
        "the catalog and fastembed's registry hold a different number of models"
    );

    // Every name a user may write, mapped back to the model it addresses. A
    // duplicate here means one of the two models is unreachable.
    let mut names: BTreeMap<String, &'static str> = BTreeMap::new();

    for info in &registered {
        let entry = entry(&info.model);

        assert_eq!(
            entry.dimensions, info.dim,
            "'{}' reports a dimension fastembed does not agree with",
            entry.canonical_name
        );
        assert!(
            !entry.note.is_empty(),
            "'{}' has no note, so a user reading the list learns nothing about it",
            entry.canonical_name
        );
        assert!(
            entry.parameter_millions > 0 && entry.max_input_tokens > 0,
            "'{}' reports no size or no context length",
            entry.canonical_name
        );
        if entry.recommended {
            assert!(
                entry.retrieval_score.is_some(),
                "'{}' is recommended but publishes no retrieval score, so the \
                 recommendation rests on nothing",
                entry.canonical_name
            );
        }

        assert_eq!(
            parse_model_name(entry.canonical_name).ok().as_ref(),
            Some(&info.model),
            "'{}' does not parse back to its own model",
            entry.canonical_name
        );

        for name in std::iter::once(entry.canonical_name).chain(entry.aliases.iter().copied()) {
            let owned = name.to_ascii_lowercase();
            if let Some(taken) = names.insert(owned, entry.canonical_name) {
                panic!(
                    "'{name}' addresses both '{taken}' and '{}'",
                    entry.canonical_name
                );
            }
        }
    }
}

/// The two forms a user copies — the short name from our own list, and the
/// HuggingFace name from a model card — reach one model.
///
/// Case is irrelevant in both, because a config file holds whatever the user
/// pasted.
#[test]
fn a_hugging_face_name_and_a_short_name_reach_one_model() {
    let short = parse_model_name("bge-small-en-v1.5").expect("the short name parses");
    let hugging_face = parse_model_name("BAAI/bge-small-en-v1.5").expect("the long name parses");
    let shouted = parse_model_name("  BGE-Small-EN-V1.5  ").expect("case and space do not matter");

    assert_eq!(short, EmbeddingModel::BGESmallENV15);
    assert_eq!(short, hugging_face);
    assert_eq!(short, shouted);

    let refused = parse_model_name("bge-small").expect_err("a partial name is not a model");
    let message = refused.to_string();
    assert!(
        message.contains("bge-small-en-v1.5"),
        "the error must name the closest catalog entries, but it said: {message}"
    );
}

/// The disk probe answers from the HuggingFace cache layout, and it answers
/// `false` for an empty directory rather than starting a download.
#[test]
fn the_probe_finds_a_model_that_is_already_in_the_cache() {
    let cache = tempfile::TempDir::new().expect("tempdir");
    let model = EmbeddingModel::BGESmallENV15;

    assert!(
        !is_downloaded(&model, cache.path()),
        "an empty cache directory holds no model"
    );

    let info = TextEmbedding::get_model_info(&model).expect("fastembed knows the default model");
    let repo = cache
        .path()
        .join(format!("models--{}", info.model_code.replace('/', "--")));
    let snapshot = repo.join("snapshots").join("deadbeef");
    std::fs::create_dir_all(snapshot.join(info.model_file.rsplit_once('/').map_or("", |p| p.0)))
        .expect("snapshot directory");
    std::fs::create_dir_all(repo.join("refs")).expect("refs directory");
    std::fs::write(repo.join("refs").join("main"), "deadbeef\n").expect("ref file");
    std::fs::write(snapshot.join(&info.model_file), b"onnx").expect("model file");

    assert!(
        is_downloaded(&model, cache.path()),
        "the probe missed a model that is already in the cache"
    );
}
