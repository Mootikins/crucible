//! What the compiler cannot check about the catalog.
//!
//! rustc proves that every `EmbeddingModel` variant has a row. It cannot prove
//! that the row is complete, that two rows claim different names, or that a
//! name parses back to the model it addresses. The walk therefore comes from
//! `TextEmbedding::list_supported_models()` — the crate's own registry — and
//! never from a list typed into this file.
//!
//! A field the catalog copies out of that same registry is not asserted here.
//! `dimensions` is `get_model_info(model).dim`, and the registry is where the
//! expectation would come from, so the assertion would compare a value with
//! itself. An independent oracle for it means writing 44 more literals, which
//! is the hand-kept list this module exists to avoid.

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

    // Every name a user may write, mapped back to the model it addresses. A
    // duplicate here means one of the two models is unreachable.
    let mut names: BTreeMap<String, &'static str> = BTreeMap::new();

    for info in &registered {
        let entry = entry(&info.model);

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

    seed(cache.path(), &model, None);

    assert!(
        is_downloaded(&model, cache.path()),
        "the probe missed a model that is already in the cache"
    );
}

/// An interrupted download is not a model.
///
/// hf-hub writes `refs/main` on the first file it fetches, and fastembed
/// fetches the ONNX file first. So the state below — the ref and the weights,
/// no tokenizer — is what a killed `download` leaves behind, and the probe
/// must not call it ready: `cru process` would then fail on a missing file
/// after the CLI said the model was in the cache.
#[test]
fn the_probe_refuses_a_download_that_stopped_after_the_weights() {
    let cache = tempfile::TempDir::new().expect("tempdir");
    let model = EmbeddingModel::BGESmallENV15;
    let info = TextEmbedding::get_model_info(&model).expect("fastembed knows the default model");

    for missing in std::iter::once(info.model_file.as_str())
        .chain(info.additional_files.iter().map(String::as_str))
        .chain(["tokenizer.json", "config.json"])
    {
        let cache = cache.path().join(missing.replace('/', "-"));
        seed(&cache, &model, Some(missing));
        assert!(
            !is_downloaded(&model, &cache),
            "the probe called a model ready with '{missing}' missing"
        );
    }
}

/// Write the HuggingFace cache layout for `model` under `cache`, leaving out
/// the one file `omit` names. Every file holds four bytes, so `disk_bytes`
/// over the seeded set is countable.
fn seed(cache: &std::path::Path, model: &EmbeddingModel, omit: Option<&str>) {
    let info = TextEmbedding::get_model_info(model).expect("fastembed knows the model");
    let repo = cache.join(format!("models--{}", info.model_code.replace('/', "--")));
    let snapshot = repo.join("snapshots").join("deadbeef");
    std::fs::create_dir_all(repo.join("refs")).expect("refs directory");
    std::fs::write(repo.join("refs").join("main"), "deadbeef\n").expect("ref file");

    let files = std::iter::once(info.model_file.as_str())
        .chain(info.additional_files.iter().map(String::as_str))
        .chain([
            "tokenizer.json",
            "config.json",
            "special_tokens_map.json",
            "tokenizer_config.json",
        ]);
    for file in files.filter(|file| Some(*file) != omit) {
        let path = snapshot.join(file);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("snapshot directory");
        std::fs::write(path, b"onnx").expect("cache file");
    }
}

/// The size a download reports counts this model's files, not its neighbour's.
///
/// `arctic-embed-xs` and `arctic-embed-xs-q` live in one HuggingFace
/// repository, so a walk over the snapshot directory would report each of them
/// as the size of both.
#[test]
fn the_size_of_a_model_excludes_a_sibling_in_the_same_repository() {
    let cache = tempfile::TempDir::new().expect("tempdir");
    let model = EmbeddingModel::SnowflakeArcticEmbedXS;
    let sibling = EmbeddingModel::SnowflakeArcticEmbedXSQ;
    let info = TextEmbedding::get_model_info(&model).expect("fastembed knows the model");
    let sibling_info = TextEmbedding::get_model_info(&sibling).expect("fastembed knows the model");
    assert_eq!(
        info.model_code, sibling_info.model_code,
        "the pair no longer shares a repository, so this test proves nothing"
    );

    seed(cache.path(), &model, None);
    let alone = disk_bytes(&model, cache.path()).expect("the model is in the cache");
    seed(cache.path(), &sibling, None);

    assert_eq!(
        disk_bytes(&model, cache.path()),
        Some(alone),
        "the sibling's weights were counted against this model"
    );
}
