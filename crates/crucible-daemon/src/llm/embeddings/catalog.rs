//! The catalog of local embedding models.
//!
//! Crucible curates a few models and fetches them for the user. Everything
//! else is open: a name the config file holds goes to the backend, and the
//! backend decides whether it knows it.
//!
//! **The curated set is the set we can download.** [`CURATED`] holds one row
//! per model that `cru models embeddings download` offers, with the numbers a
//! user needs to choose between them. A model outside that set still runs; it
//! arrives without editorial metadata, because Crucible has not measured it
//! and will not invent a number for it.
//!
//! **A number that nobody published is `None`.** `retrieval_score` holds the
//! MTEB v1 English retrieval score (nDCG@10) that the model authors report. A
//! blank column is honest; an invented number sends a user to a worse model.
//!
//! **Context is the model's limit, not Crucible's.** fastembed truncates the
//! input at 512 tokens unless the caller raises `max_length`, and Crucible
//! does not raise it. So `max_input_tokens` says what the model accepts, and a
//! model that accepts 8192 tokens still sees 512 today.

use fastembed::{EmbeddingModel, TextEmbedding};
use std::path::{Path, PathBuf};

use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{ModelFamily, ModelInfo, ParameterSize};

/// One curated model: the numbers a user needs to choose it.
struct Curated {
    model: EmbeddingModel,
    canonical_name: &'static str,
    aliases: &'static [&'static str],
    parameter_millions: u32,
    max_input_tokens: u32,
    retrieval_score: f32,
    note: &'static str,
}

/// The models Crucible recommends and can fetch.
///
/// A row earns its place by having a published retrieval score and a reason to
/// pick it over its neighbours. Adding a row means adding a model we are
/// willing to recommend, not merely one the backend supports.
const CURATED: &[Curated] = &[
    Curated {
        model: EmbeddingModel::BGESmallENV15,
        canonical_name: "bge-small-en-v1.5",
        aliases: &["BAAI/bge-small-en-v1.5"],
        parameter_millions: 33,
        max_input_tokens: 512,
        retrieval_score: 51.68,
        note: "The default. The smallest model we recommend.",
    },
    Curated {
        model: EmbeddingModel::BGEBaseENV15,
        canonical_name: "bge-base-en-v1.5",
        aliases: &["BAAI/bge-base-en-v1.5"],
        parameter_millions: 109,
        max_input_tokens: 512,
        retrieval_score: 53.25,
        note: "It retrieves better than bge-small, and it costs more CPU time.",
    },
    Curated {
        model: EmbeddingModel::SnowflakeArcticEmbedM,
        canonical_name: "arctic-embed-m",
        aliases: &[
            "Snowflake/snowflake-arctic-embed-m",
            "snowflake-arctic-embed-m",
        ],
        parameter_millions: 109,
        max_input_tokens: 512,
        retrieval_score: 54.90,
        note: "The best published retrieval score of the models we fetch.",
    },
    Curated {
        model: EmbeddingModel::GTEBaseENV15,
        canonical_name: "gte-base-en-v1.5",
        aliases: &["Alibaba-NLP/gte-base-en-v1.5"],
        parameter_millions: 137,
        max_input_tokens: 8192,
        retrieval_score: 54.09,
        note: "A strong score, and the model itself accepts 8192 tokens.",
    },
];

/// What Crucible knows about one embedding model.
///
/// `dimensions` comes from the backend's own registry. The editorial fields
/// are filled for a curated model and empty for any other, because Crucible
/// has an opinion about the first group and none about the second.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// The fastembed model this row describes.
    pub model: EmbeddingModel,

    /// The name a user writes in the config file.
    pub canonical_name: String,

    /// The width of the vector this model produces.
    pub dimensions: usize,

    /// The parameter count in millions, or `None` outside the curated set.
    pub parameter_millions: Option<u32>,

    /// The longest input the model accepts, or `None` outside the curated set.
    ///
    /// fastembed truncates at 512 tokens, so a larger number here is a
    /// property of the model and not yet of Crucible.
    pub max_input_tokens: Option<u32>,

    /// The MTEB v1 English retrieval score (nDCG@10), or `None` when the
    /// authors publish no score. Never a guess.
    pub retrieval_score: Option<f32>,

    /// Whether Crucible curates this model and can fetch it.
    pub curated: bool,

    /// One sentence that tells a user why to pick this model. Empty outside
    /// the curated set.
    pub note: &'static str,
}

/// Whether `name` addresses `curated`, ignoring case and surrounding space.
fn curated_matches(curated: &Curated, name: &str) -> bool {
    let wanted = name.trim();
    curated.canonical_name.eq_ignore_ascii_case(wanted)
        || curated
            .aliases
            .iter()
            .any(|a| a.eq_ignore_ascii_case(wanted))
}

/// The curated rows, in the order [`CURATED`] declares them.
///
/// The order is editorial: smallest first, then the ones that retrieve better.
/// It is what `cru models embeddings` prints, so it must not shuffle.
#[must_use]
pub fn all() -> Vec<CatalogEntry> {
    CURATED.iter().map(curated_entry).collect()
}

fn curated_entry(curated: &Curated) -> CatalogEntry {
    CatalogEntry {
        model: curated.model.clone(),
        canonical_name: curated.canonical_name.to_string(),
        dimensions: dimensions(&curated.model),
        parameter_millions: Some(curated.parameter_millions),
        max_input_tokens: Some(curated.max_input_tokens),
        retrieval_score: Some(curated.retrieval_score),
        curated: true,
        note: curated.note,
    }
}

/// The row for one model: curated when Crucible knows it, otherwise the
/// registry's own name and dimension and nothing else.
#[must_use]
pub fn entry(model: &EmbeddingModel) -> CatalogEntry {
    if let Some(curated) = CURATED.iter().find(|c| &c.model == model) {
        return curated_entry(curated);
    }
    CatalogEntry {
        model: model.clone(),
        canonical_name: registry_name(model),
        dimensions: dimensions(model),
        parameter_millions: None,
        max_input_tokens: None,
        retrieval_score: None,
        curated: false,
        note: "",
    }
}

/// The part of a repository name after the last `/`, which is the model name
/// a user knows.
fn leaf(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// The name the backend's registry gives a model, which is its HuggingFace
/// repository. It is the name a user writes for a model Crucible does not
/// curate.
fn registry_name(model: &EmbeddingModel) -> String {
    TextEmbedding::get_model_info(model)
        .map(|info| info.model_code.clone())
        .unwrap_or_else(|_| format!("{model:?}"))
}

/// The row `name` addresses, or `None` when no backend model answers to it.
#[must_use]
pub fn find(name: &str) -> Option<CatalogEntry> {
    parse_model_name(name).ok().map(|model| entry(&model))
}

/// Resolve a configured model name to the backend model.
///
/// A curated name resolves first. Any other name goes to the backend's own
/// registry, matched on its repository name or its Rust variant name, so a
/// user may name any model the backend supports without Crucible holding a row
/// for it. Only a name the backend does not know is an error.
pub fn parse_model_name(name: &str) -> EmbeddingResult<EmbeddingModel> {
    let wanted = name.trim();
    if let Some(curated) = CURATED.iter().find(|c| curated_matches(c, wanted)) {
        return Ok(curated.model.clone());
    }

    let registry = TextEmbedding::list_supported_models();
    let exact = registry
        .iter()
        .find(|info| {
            info.model_code.eq_ignore_ascii_case(wanted)
                || format!("{:?}", info.model).eq_ignore_ascii_case(wanted)
        })
        .map(|info| info.model.clone());
    if let Some(model) = exact {
        return Ok(model);
    }

    // The registry names a model by the repository that hosts its ONNX build,
    // which is often a mirror: `bge-large-en-v1.5` lives under `Xenova`, not
    // under `BAAI`. A user writes the name they know, so the repository owner
    // is ignored when the remaining name picks out one model.
    let by_leaf: Vec<&fastembed::ModelInfo<EmbeddingModel>> = registry
        .iter()
        .filter(|info| leaf(&info.model_code).eq_ignore_ascii_case(leaf(wanted)))
        .collect();
    match by_leaf.as_slice() {
        [one] => return Ok(one.model.clone()),
        [] => {}
        several => {
            let names = several
                .iter()
                .map(|info| info.model_code.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(EmbeddingError::ConfigError(format!(
                "The name '{name}' addresses more than one model: {names}. \
                 Write the full repository name."
            )));
        }
    }

    let curated = CURATED
        .iter()
        .map(|c| c.canonical_name)
        .collect::<Vec<_>>()
        .join(", ");
    Err(EmbeddingError::ConfigError(format!(
        "The embedding backend does not know the model '{name}'. Crucible \
         curates and downloads these: {curated}. Any other model the backend \
         supports may be named by its repository, such as \
         'BAAI/bge-large-en-v1.5'."
    )))
}

/// The provider metadata for a model, derived from the catalog.
#[must_use]
pub fn model_info(model: &EmbeddingModel) -> ModelInfo {
    let entry = entry(model);
    let mut builder = ModelInfo::builder()
        .name(entry.canonical_name.clone())
        .family(family_of(&entry.canonical_name))
        .dimensions(entry.dimensions)
        .format("onnx")
        .recommended(entry.curated);
    if let Some(millions) = entry.parameter_millions {
        builder = builder.parameter_size(ParameterSize::new(millions, true));
    }
    if let Some(tokens) = entry.max_input_tokens {
        builder = builder.max_tokens(tokens as usize);
    }
    builder.build()
}

/// The directory fastembed reads and writes models in.
///
/// This repeats fastembed's own precedence — `HF_HOME` wins over the
/// configured directory — because a probe that answered for a different
/// directory than the download uses is worse than no probe.
/// The directory is made absolute, because the relative default
/// (`.fastembed_cache`) would otherwise answer for the daemon's working
/// directory: the probe and the path the CLI prints would both change when the
/// daemon restarts somewhere else.
#[must_use]
pub fn cache_dir(configured: Option<&Path>) -> PathBuf {
    let dir = std::env::var_os("HF_HOME").map_or_else(
        || {
            configured
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(fastembed::get_cache_dir()))
        },
        PathBuf::from,
    );
    std::path::absolute(&dir).unwrap_or(dir)
}

/// Every file fastembed reads before it can build the encoder.
///
/// `TextEmbedding::try_new` fetches the ONNX file, then `additional_files`,
/// then these four tokenizer files (`fastembed::common::load_tokenizer_hf_hub`).
/// The list matters because hf-hub writes `refs/main` on the *first* file, so
/// the ref proves only that a download started.
fn required_files(info: &fastembed::ModelInfo<EmbeddingModel>) -> impl Iterator<Item = &str> {
    const TOKENIZER: [&str; 4] = [
        "tokenizer.json",
        "config.json",
        "special_tokens_map.json",
        "tokenizer_config.json",
    ];
    std::iter::once(info.model_file.as_str())
        .chain(info.additional_files.iter().map(String::as_str))
        .chain(TOKENIZER)
}

/// Whether the model's files are already in `cache_dir`.
///
/// A directory probe over the HuggingFace cache layout
/// (`models--<org>--<repo>/refs/main` names the commit, `snapshots/<commit>/`
/// holds the files). It stats a handful of paths and downloads nothing, so a
/// caller may ask it for all 44 models to render a table.
#[must_use]
pub fn is_downloaded(model: &EmbeddingModel, cache_dir: &Path) -> bool {
    snapshot_dir(model, cache_dir).is_some()
}

/// The directory that holds the model's files, or `None` when any is absent.
///
/// The same probe as [`is_downloaded`], and the answer `cru models embeddings
/// download` prints: a user who asks where the file went gets a path rather
/// than a yes.
///
/// Every file [`required_files`] names must be present. The ONNX file alone is
/// not enough: an interrupted download leaves the ONNX file and the ref behind
/// with no tokenizer, and a probe that answered `true` for that state would
/// tell the user a model is ready that `cru process` cannot load. Three models
/// carry a multi-gigabyte `model.onnx_data` in `additional_files`, which is
/// where the interruption is most likely.
#[must_use]
pub fn snapshot_dir(model: &EmbeddingModel, cache_dir: &Path) -> Option<PathBuf> {
    let info = TextEmbedding::get_model_info(model).ok()?;
    let repo = cache_dir.join(format!("models--{}", info.model_code.replace('/', "--")));
    let commit = std::fs::read_to_string(repo.join("refs").join("main")).ok()?;
    let dir = repo.join("snapshots").join(commit.trim());
    required_files(info)
        .all(|file| dir.join(file).exists())
        .then_some(dir)
}

/// The number of bytes the model occupies, or `None` when it is absent.
///
/// The sum covers the files this model needs and no others, because eight of
/// the 44 models share a repository with a quantised sibling: a walk over the
/// snapshot directory would report each of the pair as the size of both.
/// `metadata` follows the symbolic links the HuggingFace cache writes, so the
/// number is the size of the blobs themselves.
#[must_use]
pub fn disk_bytes(model: &EmbeddingModel, cache_dir: &Path) -> Option<u64> {
    let dir = snapshot_dir(model, cache_dir)?;
    let info = TextEmbedding::get_model_info(model).ok()?;
    Some(
        required_files(info)
            .filter_map(|file| std::fs::metadata(dir.join(file)).ok())
            .map(|meta| meta.len())
            .sum(),
    )
}

/// Fetch the model into `cache_dir`, then report the directory it landed in.
///
/// fastembed downloads a model as a side effect of constructing the encoder,
/// so this constructs one and drops it. The daemon does the fetch because the
/// CLI links no ONNX runtime and holds no cache.
///
/// The progress bar is off. fastembed writes it to this process's stdout,
/// which is the daemon's log file or `/dev/null` — never the terminal the user
/// is watching. Reporting progress to the caller needs a session event, so the
/// CLI says what the wait is for instead.
pub async fn download(model: &EmbeddingModel, cache_dir: &Path) -> EmbeddingResult<PathBuf> {
    let options = fastembed::InitOptions::new(model.clone())
        .with_cache_dir(cache_dir.to_path_buf())
        .with_show_download_progress(false);
    tokio::task::spawn_blocking(move || TextEmbedding::try_new(options))
        .await
        .map_err(|e| EmbeddingError::ProviderError {
            provider: "FastEmbed".to_string(),
            message: format!("The download task did not finish: {e}"),
        })?
        .map_err(|e| EmbeddingError::ProviderError {
            provider: "FastEmbed".to_string(),
            message: format!("The download failed: {e}"),
        })?;

    snapshot_dir(model, cache_dir).ok_or_else(|| EmbeddingError::ProviderError {
        provider: "FastEmbed".to_string(),
        message: "The download reported success but the cache holds no files.".to_string(),
    })
}

/// The vector width, read from the backend's registry rather than restated
/// here.
fn dimensions(model: &EmbeddingModel) -> usize {
    TextEmbedding::get_model_info(model)
        .map(|info| info.dim)
        .unwrap_or_default()
}

/// The three families the provider metadata distinguishes.
fn family_of(canonical_name: &str) -> ModelFamily {
    if canonical_name.starts_with("clip-") {
        ModelFamily::Clip
    } else if canonical_name.contains("mpnet") {
        ModelFamily::Mpnet
    } else {
        ModelFamily::Bert
    }
}

#[cfg(test)]
mod tests;
