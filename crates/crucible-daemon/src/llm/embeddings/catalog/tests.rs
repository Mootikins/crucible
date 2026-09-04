//! What the compiler cannot check about the catalog.
//!
//! The curated set is small and hand-written, so these tests prove the three
//! things a reader cannot see by eye: every curated row resolves by every name
//! it claims, a model outside the set still resolves through the backend's own
//! registry, and an unknown name is refused with the curated names in the
//! message.
//!
//! A field the catalog copies out of that registry is not asserted here.
//! `dimensions` is `get_model_info(model).dim`, so the expectation would come
//! from the same call and would compare a value with itself.

use super::*;
use std::collections::BTreeSet;

/// Every curated row resolves by its canonical name and by each alias, and
/// carries the numbers a user chooses on. Two rows never share a name.
#[test]
fn every_curated_row_resolves_by_every_name_it_claims() {
    let mut seen = BTreeSet::new();
    for curated in CURATED {
        let mut names = vec![curated.canonical_name];
        names.extend(curated.aliases.iter().copied());
        for name in names {
            let lowered = name.to_lowercase();
            assert!(
                seen.insert(lowered.clone()),
                "two curated rows answer to {name}"
            );
            let model = parse_model_name(name)
                .unwrap_or_else(|e| panic!("the curated name {name} does not resolve: {e}"));
            assert_eq!(model, curated.model, "{name} reaches another model");
            let entry = entry(&model);
            assert!(entry.curated, "{name} must report itself as curated");
            assert_eq!(entry.canonical_name, curated.canonical_name);
            assert!(entry.retrieval_score.is_some(), "{name} needs a score");
            assert!(entry.parameter_millions.is_some(), "{name} needs a size");
            assert!(entry.max_input_tokens.is_some(), "{name} needs a context");
            assert!(!entry.note.is_empty(), "{name} needs a note");
            assert!(entry.dimensions > 0, "{name} needs a width");
        }
    }
    assert_eq!(all().len(), CURATED.len(), "`all` lists the curated rows");
}

/// A model Crucible does not curate still runs: the backend's registry
/// resolves it by repository name, and the row carries no invented metadata.
#[test]
fn a_model_outside_the_curated_set_resolves_through_the_registry() {
    // The registry hosts this model under a mirror, so the name a user knows
    // resolves through the leaf rather than the repository owner.
    let model = parse_model_name("BAAI/bge-large-en-v1.5")
        .expect("the backend knows bge-large by the name a user writes");
    assert_eq!(model, EmbeddingModel::BGELargeENV15);

    let entry = entry(&model);
    assert!(!entry.curated, "bge-large is not one of the curated four");
    assert_eq!(entry.canonical_name, "Xenova/bge-large-en-v1.5");
    assert_eq!(entry.dimensions, 1024, "the width comes from the registry");
    assert_eq!(entry.retrieval_score, None, "no score is invented");
    assert_eq!(entry.parameter_millions, None);
    assert_eq!(entry.max_input_tokens, None);
    assert!(entry.note.is_empty());

    // The Rust variant name resolves too, which is what fastembed's own
    // `FromStr` accepts.
    assert_eq!(parse_model_name("BGELargeENV15").unwrap(), model);
    assert!(
        all().iter().all(|row| row.model != model),
        "an uncurated model is not offered for download"
    );
}

/// A name no backend model answers to is refused, and the message names the
/// models Crucible can fetch.
#[test]
fn an_unknown_name_is_refused_and_names_the_curated_models() {
    let error = parse_model_name("bge-enormous").expect_err("no such model");
    let message = error.to_string();
    for curated in CURATED {
        assert!(
            message.contains(curated.canonical_name),
            "the refusal must name {}: {message}",
            curated.canonical_name
        );
    }
}

/// A curated name matches whatever case and spacing the config file holds.
#[test]
fn a_curated_name_ignores_case_and_surrounding_space() {
    let model = EmbeddingModel::BGESmallENV15;
    for name in [
        "bge-small-en-v1.5",
        "  BGE-Small-EN-V1.5  ",
        "BAAI/bge-small-en-v1.5",
    ] {
        assert_eq!(parse_model_name(name).unwrap(), model, "{name}");
    }
}

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
