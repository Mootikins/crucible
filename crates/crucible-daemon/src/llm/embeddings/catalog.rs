//! The catalog of local embedding models.
//!
//! fastembed ships 44 text models. Crucible used to know four of them: the
//! name parser matched a dozen literal strings, and everything it did not
//! recognise became a `ConfigError`. A second `match` then answered the model
//! metadata, and that one *did* carry a wildcard arm — so `bge-large-en-v1.5`
//! parsed, fell through to the fallback, and reported 768 dimensions for a
//! model that gives 1024. The two lists disagreed, and neither list said which
//! model a user should choose.
//!
//! One table replaces both. [`facts`] is a total function over
//! `fastembed::EmbeddingModel` with no wildcard arm, so a model that the crate
//! adds does not compile until someone writes the row for it. Everything else
//! reads that table: [`parse_model_name`], [`model_info`], and the list the
//! CLI renders.
//!
//! **The set of models is never hand-written.** [`all`] walks
//! `TextEmbedding::list_supported_models()` — the crate's own registry — and
//! the exhaustive `match` supplies the row for each. A hand-kept array would
//! go stale in exactly the way this module exists to prevent, and the
//! dimension comes from the same registry rather than from a number typed
//! here.
//!
//! **A number that nobody published is `None`.** `retrieval_score` holds the
//! MTEB v1 English retrieval score (nDCG@10) that the model authors report.
//! Many models publish none. A blank column is honest; an invented number
//! sends a user to a worse model.
//!
//! **Context is the model's limit, not Crucible's.** fastembed truncates the
//! input at 512 tokens unless the caller raises `max_length`, and Crucible
//! does not raise it. So `max_input_tokens` says what the model accepts, and a
//! model that accepts 8192 tokens still sees 512 today.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use fastembed::{EmbeddingModel, TextEmbedding};
use std::path::{Path, PathBuf};

use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{ModelFamily, ModelInfo, ParameterSize};

/// The note every quantised build carries.
///
/// One constant, because the sentence is the same for all eleven of them: the
/// weights lose precision, the vector does not change shape.
const QUANTISED: &str =
    "A quantised build. The vector keeps the same dimensions. The file on disk is smaller.";

/// What Crucible knows about one fastembed text model.
///
/// The row a user reads. `dimensions` comes from fastembed's own registry;
/// every other field is Crucible's editorial answer to "should I pick this
/// one".
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// The fastembed model this row describes.
    pub model: EmbeddingModel,

    /// The name a user writes in the config file. Unique across the catalog.
    pub canonical_name: &'static str,

    /// Other names that resolve to this model, such as the HuggingFace form.
    pub aliases: &'static [&'static str],

    /// The width of the vector this model produces.
    pub dimensions: usize,

    /// The parameter count in millions, as the model authors report it.
    pub parameter_millions: u32,

    /// The longest input the model accepts, in tokens.
    ///
    /// fastembed truncates at 512 tokens, so a larger number here is a
    /// property of the model and not yet of Crucible.
    pub max_input_tokens: u32,

    /// The MTEB v1 English retrieval score (nDCG@10), or `None` when the
    /// authors publish no score. Never a guess.
    pub retrieval_score: Option<f32>,

    /// Whether Crucible recommends this model for a knowledge corpus.
    pub recommended: bool,

    /// One sentence that tells a user why to pick this model, or why not.
    pub note: &'static str,
}

impl CatalogEntry {
    /// Whether `name` addresses this model. The comparison ignores case and
    /// surrounding space.
    ///
    /// The variant's own Rust name (`BGESmallENV15`) always resolves, which is
    /// what fastembed's `FromStr` accepts. A model therefore stays reachable
    /// even before someone writes a friendly alias for it.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        let wanted = name.trim();
        self.canonical_name.eq_ignore_ascii_case(wanted)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(wanted))
            || format!("{:?}", self.model).eq_ignore_ascii_case(wanted)
    }
}

/// Every model fastembed exposes, ordered by canonical name.
///
/// The order is stable so that a table the CLI prints does not shuffle between
/// runs; `list_supported_models` answers from a `HashMap`.
#[must_use]
pub fn all() -> Vec<CatalogEntry> {
    let mut entries: Vec<CatalogEntry> = TextEmbedding::list_supported_models()
        .into_iter()
        .map(|info| entry(&info.model))
        .collect();
    entries.sort_by_key(|e| e.canonical_name);
    entries
}

/// The catalog row for one model.
#[must_use]
pub fn entry(model: &EmbeddingModel) -> CatalogEntry {
    let facts = facts(model);
    CatalogEntry {
        model: model.clone(),
        canonical_name: facts.canonical_name,
        aliases: facts.aliases,
        dimensions: dimensions(model),
        parameter_millions: facts.parameter_millions,
        max_input_tokens: facts.max_input_tokens,
        retrieval_score: facts.retrieval_score,
        recommended: facts.recommended,
        note: facts.note,
    }
}

/// The row whose canonical name or alias is `name`, or `None`.
#[must_use]
pub fn find(name: &str) -> Option<CatalogEntry> {
    all().into_iter().find(|entry| entry.matches(name))
}

/// Resolve a configured model name to the fastembed model.
///
/// The error names the closest catalog entries. A fixed sentence that listed
/// four models sent users to a config file to guess, and the guess was usually
/// a HuggingFace name the parser did not hold.
pub fn parse_model_name(name: &str) -> EmbeddingResult<EmbeddingModel> {
    if let Some(entry) = find(name) {
        return Ok(entry.model);
    }

    let suggestions = suggestions_for(name).join(", ");
    Err(EmbeddingError::ConfigError(format!(
        "Unknown embedding model '{name}'. The closest catalog names are: {suggestions}."
    )))
}

/// The provider metadata for a model, derived from the catalog.
///
/// `name` is the canonical catalog name, which is also the key the block store
/// caches a vector under. Two models therefore never share a cache key, and a
/// model that used to fall through the old wildcard arm no longer reports a
/// Rust variant name as its identity.
#[must_use]
pub fn model_info(model: &EmbeddingModel) -> ModelInfo {
    let entry = entry(model);
    ModelInfo::builder()
        .name(entry.canonical_name)
        .family(family_of(entry.canonical_name))
        .dimensions(entry.dimensions)
        .parameter_size(ParameterSize::new(entry.parameter_millions, true))
        .max_tokens(entry.max_input_tokens as usize)
        .format("onnx")
        .recommended(entry.recommended)
        .build()
}

/// The directory fastembed reads and writes models in.
///
/// This repeats fastembed's own precedence — `HF_HOME` wins over the
/// configured directory — because a probe that answered for a different
/// directory than the download uses is worse than no probe.
#[must_use]
pub fn cache_dir(configured: Option<&Path>) -> PathBuf {
    std::env::var_os("HF_HOME").map_or_else(
        || {
            configured
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(fastembed::get_cache_dir()))
        },
        PathBuf::from,
    )
}

/// Whether the model's files are already in `cache_dir`.
///
/// A directory probe over the HuggingFace cache layout
/// (`models--<org>--<repo>/refs/main` names the commit, `snapshots/<commit>/`
/// holds the files). It reads two paths and downloads nothing, so a caller may
/// ask it for all 44 models to render a table.
#[must_use]
pub fn is_downloaded(model: &EmbeddingModel, cache_dir: &Path) -> bool {
    snapshot_dir(model, cache_dir).is_some()
}

/// The directory that holds the model's files, or `None` when they are absent.
///
/// The same probe as [`is_downloaded`], and the answer `cru models embeddings
/// download` prints: a user who asks where the file went gets a path rather
/// than a yes.
#[must_use]
pub fn snapshot_dir(model: &EmbeddingModel, cache_dir: &Path) -> Option<PathBuf> {
    let info = TextEmbedding::get_model_info(model).ok()?;
    let repo = cache_dir.join(format!("models--{}", info.model_code.replace('/', "--")));
    let commit = std::fs::read_to_string(repo.join("refs").join("main")).ok()?;
    let dir = repo.join("snapshots").join(commit.trim());
    dir.join(&info.model_file).exists().then_some(dir)
}

/// The number of bytes the model occupies, or `None` when it is absent.
///
/// The walk follows the symbolic links that the HuggingFace cache writes into
/// a snapshot directory, so the number is the size of the blobs themselves.
#[must_use]
pub fn disk_bytes(model: &EmbeddingModel, cache_dir: &Path) -> Option<u64> {
    let mut pending = vec![snapshot_dir(model, cache_dir)?];
    let mut total = 0;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match std::fs::metadata(entry.path()) {
                Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    Some(total)
}

/// Fetch the model into `cache_dir`, then report the directory it landed in.
///
/// fastembed downloads a model as a side effect of constructing the encoder,
/// so this constructs one and drops it. The daemon does the fetch because the
/// CLI links no ONNX runtime and holds no cache.
pub async fn download(model: &EmbeddingModel, cache_dir: &Path) -> EmbeddingResult<PathBuf> {
    let options = fastembed::InitOptions::new(model.clone())
        .with_cache_dir(cache_dir.to_path_buf())
        .with_show_download_progress(true);
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

/// The vector width, read from fastembed's registry rather than restated here.
fn dimensions(model: &EmbeddingModel) -> usize {
    TextEmbedding::get_model_info(model)
        .map(|info| info.dim)
        .expect("fastembed registers a dimension for every model it exposes")
}

/// The architecture family, from the name.
///
/// A rule, not a table: the catalog would otherwise carry a 44th answer that
/// nobody reads, and the three families the names distinguish are the three
/// this projection has.
fn family_of(canonical_name: &str) -> ModelFamily {
    if canonical_name.starts_with("clip-") {
        ModelFamily::Clip
    } else if canonical_name.contains("mpnet") {
        ModelFamily::Mpnet
    } else {
        ModelFamily::Bert
    }
}

/// Up to three catalog names close to `name`.
///
/// Closeness is a shared prefix or a shared substring, which is what a typo
/// and a half-remembered HuggingFace name both look like. When nothing is
/// close the answer is the recommended set, because a user who typed a name
/// from another provider needs a choice rather than a correction.
fn suggestions_for(name: &str) -> Vec<&'static str> {
    let wanted = name.trim().to_ascii_lowercase();
    let mut scored: Vec<(usize, &'static str)> = all()
        .into_iter()
        .map(|entry| {
            let candidate = entry.canonical_name.to_ascii_lowercase();
            let shared = candidate
                .chars()
                .zip(wanted.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let contained = usize::from(
                !wanted.is_empty() && (candidate.contains(&wanted) || wanted.contains(&candidate)),
            );
            (shared + contained * 8, entry.canonical_name)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    scored.truncate(3);

    if scored.is_empty() {
        return all()
            .into_iter()
            .filter(|entry| entry.recommended)
            .map(|entry| entry.canonical_name)
            .collect();
    }
    scored.into_iter().map(|(_, name)| name).collect()
}

/// The part of a catalog row that Crucible writes down.
struct Facts {
    canonical_name: &'static str,
    aliases: &'static [&'static str],
    parameter_millions: u32,
    max_input_tokens: u32,
    retrieval_score: Option<f32>,
    recommended: bool,
    note: &'static str,
}

impl Facts {
    fn new(
        canonical_name: &'static str,
        aliases: &'static [&'static str],
        parameter_millions: u32,
        max_input_tokens: u32,
    ) -> Self {
        Self {
            canonical_name,
            aliases,
            parameter_millions,
            max_input_tokens,
            retrieval_score: None,
            recommended: false,
            note: "",
        }
    }

    /// The published MTEB v1 English retrieval score. Call it only for a model
    /// whose authors publish one.
    fn score(mut self, score: f32) -> Self {
        self.retrieval_score = Some(score);
        self
    }

    fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }

    fn note(mut self, note: &'static str) -> Self {
        self.note = note;
        self
    }
}

/// The table.
///
/// **No wildcard arm, ever.** A model that fastembed adds must fail to compile
/// until someone answers for it; that compile error is the whole mechanism,
/// and the two module-level denies above stop a `_` arm from silencing it.
fn facts(model: &EmbeddingModel) -> Facts {
    match model {
        // -- The four models Crucible recommends -----------------------------
        EmbeddingModel::BGESmallENV15 => {
            Facts::new("bge-small-en-v1.5", &["BAAI/bge-small-en-v1.5"], 33, 512)
                .score(51.68)
                .recommended()
                .note("The default. The smallest model here with a published retrieval score.")
        }
        EmbeddingModel::BGEBaseENV15 => {
            Facts::new("bge-base-en-v1.5", &["BAAI/bge-base-en-v1.5"], 109, 512)
                .score(53.25)
                .recommended()
                .note("It retrieves better than bge-small, and it costs more CPU time.")
        }
        EmbeddingModel::SnowflakeArcticEmbedM => Facts::new(
            "arctic-embed-m",
            &[
                "Snowflake/snowflake-arctic-embed-m",
                "snowflake-arctic-embed-m",
            ],
            109,
            512,
        )
        .score(54.90)
        .recommended()
        .note("The best published retrieval score in this catalog."),
        EmbeddingModel::GTEBaseENV15 => Facts::new(
            "gte-base-en-v1.5",
            &["Alibaba-NLP/gte-base-en-v1.5"],
            137,
            8192,
        )
        .score(54.09)
        .recommended()
        .note("A strong score, and the model itself accepts 8192 tokens."),

        // -- The rest of the BGE family --------------------------------------
        EmbeddingModel::BGESmallENV15Q => Facts::new(
            "bge-small-en-v1.5-q",
            &["Qdrant/bge-small-en-v1.5-onnx-Q"],
            33,
            512,
        )
        .note(QUANTISED),
        EmbeddingModel::BGEBaseENV15Q => Facts::new(
            "bge-base-en-v1.5-q",
            &["Qdrant/bge-base-en-v1.5-onnx-Q"],
            109,
            512,
        )
        .note(QUANTISED),
        EmbeddingModel::BGELargeENV15 => Facts::new(
            "bge-large-en-v1.5",
            &["BAAI/bge-large-en-v1.5"],
            335,
            512,
        )
        .score(54.29)
        .note("The largest English BGE model. It gives 1024 dimensions, and it is slow on a CPU."),
        EmbeddingModel::BGELargeENV15Q => Facts::new(
            "bge-large-en-v1.5-q",
            &["Qdrant/bge-large-en-v1.5-onnx-Q"],
            335,
            512,
        )
        .note(QUANTISED),
        EmbeddingModel::BGESmallZHV15 => {
            Facts::new("bge-small-zh-v1.5", &["BAAI/bge-small-zh-v1.5"], 24, 512)
                .note("A Chinese model. Use it for a Chinese corpus.")
        }
        EmbeddingModel::BGELargeZHV15 => {
            Facts::new("bge-large-zh-v1.5", &["BAAI/bge-large-zh-v1.5"], 326, 512)
                .note("A large Chinese model. Use it for a Chinese corpus.")
        }
        EmbeddingModel::BGEM3 => Facts::new("bge-m3", &["BAAI/bge-m3"], 568, 8192)
            .note("A large multilingual model. The authors publish no English retrieval score."),

        // -- Snowflake Arctic ------------------------------------------------
        EmbeddingModel::SnowflakeArcticEmbedXS => Facts::new(
            "arctic-embed-xs",
            &[
                "snowflake/snowflake-arctic-embed-xs",
                "snowflake-arctic-embed-xs",
            ],
            23,
            512,
        )
        .score(50.15)
        .note("The smallest Arctic model. It trades retrieval quality for speed."),
        EmbeddingModel::SnowflakeArcticEmbedXSQ => {
            Facts::new("arctic-embed-xs-q", &[], 23, 512).note(QUANTISED)
        }
        EmbeddingModel::SnowflakeArcticEmbedS => Facts::new(
            "arctic-embed-s",
            &[
                "snowflake/snowflake-arctic-embed-s",
                "snowflake-arctic-embed-s",
            ],
            33,
            512,
        )
        .score(51.98)
        .note("The size of bge-small, with a slightly better score."),
        EmbeddingModel::SnowflakeArcticEmbedSQ => {
            Facts::new("arctic-embed-s-q", &[], 33, 512).note(QUANTISED)
        }
        EmbeddingModel::SnowflakeArcticEmbedMQ => {
            Facts::new("arctic-embed-m-q", &[], 109, 512).note(QUANTISED)
        }
        EmbeddingModel::SnowflakeArcticEmbedMLong => Facts::new(
            "arctic-embed-m-long",
            &["snowflake/snowflake-arctic-embed-m-long"],
            137,
            2048,
        )
        .note("A long-context Arctic model. Snowflake publishes no score for it."),
        EmbeddingModel::SnowflakeArcticEmbedMLongQ => {
            Facts::new("arctic-embed-m-long-q", &[], 137, 2048).note(QUANTISED)
        }
        EmbeddingModel::SnowflakeArcticEmbedL => Facts::new(
            "arctic-embed-l",
            &[
                "snowflake/snowflake-arctic-embed-l",
                "snowflake-arctic-embed-l",
            ],
            335,
            512,
        )
        .note("The largest Arctic model. Snowflake publishes no score for it."),
        EmbeddingModel::SnowflakeArcticEmbedLQ => {
            Facts::new("arctic-embed-l-q", &[], 335, 512).note(QUANTISED)
        }

        // -- GTE -------------------------------------------------------------
        EmbeddingModel::GTEBaseENV15Q => {
            Facts::new("gte-base-en-v1.5-q", &[], 137, 8192).note(QUANTISED)
        }
        EmbeddingModel::GTELargeENV15 => Facts::new(
            "gte-large-en-v1.5",
            &["Alibaba-NLP/gte-large-en-v1.5"],
            434,
            8192,
        )
        .note("The large GTE model. Alibaba publishes no retrieval score for it."),
        EmbeddingModel::GTELargeENV15Q => {
            Facts::new("gte-large-en-v1.5-q", &[], 434, 8192).note(QUANTISED)
        }

        // -- Nomic -----------------------------------------------------------
        EmbeddingModel::NomicEmbedTextV1 => Facts::new(
            "nomic-embed-text-v1",
            &["nomic-ai/nomic-embed-text-v1"],
            137,
            8192,
        )
        .note("The first Nomic model. Version 1.5 supersedes it."),
        EmbeddingModel::NomicEmbedTextV15 => Facts::new(
            "nomic-embed-text-v1.5",
            &["nomic-ai/nomic-embed-text-v1.5"],
            137,
            8192,
        )
        .score(53.25)
        .note("A long-context model with a good score. It gives 768 dimensions."),
        EmbeddingModel::NomicEmbedTextV15Q => {
            Facts::new("nomic-embed-text-v1.5-q", &[], 137, 8192).note(QUANTISED)
        }

        // -- Sentence-transformers -------------------------------------------
        EmbeddingModel::AllMiniLML6V2 => Facts::new(
            "all-MiniLM-L6-v2",
            &[
                "sentence-transformers/all-MiniLM-L6-v2",
                "Qdrant/all-MiniLM-L6-v2-onnx",
            ],
            23,
            256,
        )
        .note("Very small and very fast. The authors publish no retrieval score."),
        EmbeddingModel::AllMiniLML6V2Q => {
            Facts::new("all-MiniLM-L6-v2-q", &[], 23, 256).note(QUANTISED)
        }
        EmbeddingModel::AllMiniLML12V2 => Facts::new(
            "all-MiniLM-L12-v2",
            &["sentence-transformers/all-MiniLM-L12-v2"],
            33,
            256,
        )
        .note("A deeper MiniLM. The authors publish no retrieval score."),
        EmbeddingModel::AllMiniLML12V2Q => {
            Facts::new("all-MiniLM-L12-v2-q", &[], 33, 256).note(QUANTISED)
        }
        EmbeddingModel::AllMpnetBaseV2 => Facts::new(
            "all-mpnet-base-v2",
            &["sentence-transformers/all-mpnet-base-v2"],
            109,
            384,
        )
        .note("A sentence-similarity model. The authors publish no retrieval score."),
        EmbeddingModel::ParaphraseMLMiniLML12V2 => Facts::new(
            "paraphrase-multilingual-MiniLM-L12-v2",
            &[
                "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
                "paraphrase-minilm-l12-v2",
                "sentence-transformers/paraphrase-minilm-l12-v2",
            ],
            118,
            128,
        )
        .note("A multilingual paraphrase model. It accepts 128 tokens only."),
        EmbeddingModel::ParaphraseMLMiniLML12V2Q => {
            Facts::new("paraphrase-multilingual-MiniLM-L12-v2-q", &[], 118, 128).note(QUANTISED)
        }
        EmbeddingModel::ParaphraseMLMpnetBaseV2 => Facts::new(
            "paraphrase-multilingual-mpnet-base-v2",
            &["sentence-transformers/paraphrase-multilingual-mpnet-base-v2"],
            278,
            128,
        )
        .note("A larger multilingual paraphrase model. It accepts 128 tokens only."),

        // -- Multilingual E5 --------------------------------------------------
        EmbeddingModel::MultilingualE5Small => Facts::new(
            "multilingual-e5-small",
            &["intfloat/multilingual-e5-small"],
            118,
            512,
        )
        .note("A small multilingual model. The authors publish no English retrieval score."),
        EmbeddingModel::MultilingualE5Base => Facts::new(
            "multilingual-e5-base",
            &["intfloat/multilingual-e5-base"],
            278,
            512,
        )
        .note("A multilingual model. The authors publish no English retrieval score."),
        EmbeddingModel::MultilingualE5Large => Facts::new(
            "multilingual-e5-large",
            &[
                "intfloat/multilingual-e5-large",
                "Qdrant/multilingual-e5-large-onnx",
            ],
            560,
            512,
        )
        .note("A large multilingual model. It is slow on a CPU."),

        // -- Everything else --------------------------------------------------
        EmbeddingModel::MxbaiEmbedLargeV1 => Facts::new(
            "mxbai-embed-large-v1",
            &["mixedbread-ai/mxbai-embed-large-v1"],
            335,
            512,
        )
        .note("A large English model. It gives 1024 dimensions."),
        EmbeddingModel::MxbaiEmbedLargeV1Q => {
            Facts::new("mxbai-embed-large-v1-q", &[], 335, 512).note(QUANTISED)
        }
        EmbeddingModel::ModernBertEmbedLarge => Facts::new(
            "modernbert-embed-large",
            &["lightonai/modernbert-embed-large"],
            396,
            8192,
        )
        .note("A ModernBERT model. It accepts 8192 tokens, and it is slow on a CPU."),
        EmbeddingModel::EmbeddingGemma300M => Facts::new(
            "embeddinggemma-300m",
            &[
                "google/embeddinggemma-300m",
                "onnx-community/embeddinggemma-300m-ONNX",
            ],
            308,
            2048,
        )
        .note("A Gemma model from Google. The authors publish no MTEB v1 retrieval score."),
        EmbeddingModel::JinaEmbeddingsV2BaseCode => Facts::new(
            "jina-embeddings-v2-base-code",
            &["jinaai/jina-embeddings-v2-base-code"],
            161,
            8192,
        )
        .note("A code model. Use it for a corpus of source code."),
        EmbeddingModel::JinaEmbeddingsV2BaseEN => Facts::new(
            "jina-embeddings-v2-base-en",
            &["jinaai/jina-embeddings-v2-base-en"],
            137,
            8192,
        )
        .note("A long-context English model. The authors publish no MTEB v1 retrieval score."),
        EmbeddingModel::ClipVitB32 => {
            Facts::new("clip-vit-b-32-text", &["Qdrant/clip-ViT-B-32-text"], 63, 77)
                .note("The text half of CLIP. It matches images, and it is weak on text retrieval.")
        }
    }
}

#[cfg(test)]
mod tests;
