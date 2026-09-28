//! The one table of Bases operations: their typed parameters, their answers,
//! and the dispatcher that the RPC methods and `cru.kiln` both call.
use super::disposition::Writer;
use super::*;
pub use crucible_core::bases::BaseOperation;
use serde::de::DeserializeOwned;
use serde_json::Value as Json;

/// The JSON-RPC code of a Bases refusal whose base, note or kiln is absent.
pub const NOT_FOUND: i32 = -32004;

/// Deserialize a field that must be present but may be JSON `null`, keeping
/// `null` apart from absence.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Json>, D::Error> {
    Json::deserialize(d).map(Some)
}

/// `base.list`: the saved `.base` files of a kiln.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListParams {
    #[serde(default)]
    pub kiln: Option<String>,
}

/// `base.views`: the named views of one base.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ViewsParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub source: Source,
}

/// `base.query` as a client sends it: with no kiln, the daemon's default
/// kiln answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct QueryParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub view: Option<String>,
    /// The note that embeds the base, for `this`.
    #[serde(default, rename = "this")]
    pub host: Option<String>,
}

/// `base.set_property`: set or delete one frontmatter property of a note.
///
/// `value` present, including JSON `null`, sets the property; `null` writes
/// an empty property, as Obsidian does. An absent `value` needs
/// `delete: true`, so that a forgotten value never deletes a property.
/// The key `file.folder` moves the note into the folder that `value` names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetPropertyParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub path: String,
    pub key: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<serde_json::Value>))]
    pub value: Option<Json>,
    #[serde(default)]
    pub delete: bool,
    pub ancestor_hash: String,
}

/// `base.create_entry`: create a note that the base's filters admit.
///
/// `group` names the group the entry joins; JSON `null` is the group of
/// entries with no value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateEntryParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<serde_json::Value>))]
    pub group: Option<Json>,
}

/// `base.reorder_groups`: save a view's group order. `null` removes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReorderGroupsParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub view: Option<String>,
    /// The note that embeds an inline base.
    #[serde(default, rename = "this")]
    pub host: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Vec<serde_json::Value>>))]
    pub group_order: Option<Vec<Json>>,
    pub ancestor_hash: String,
}

/// `cru.kiln.ensure_base`: create a `.base` file only when it is absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsureBaseParams {
    #[serde(default)]
    pub kiln: Option<String>,
    pub path: String,
    pub yaml: String,
}

/// One pending proposed note write in a kiln, with its final properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWrite {
    pub path: String,
    pub proposal: String,
    pub properties: Json,
}

/// What a Bases write did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WriteOutcome {
    /// The disk holds the change. `ancestor_hash` is the hash of the file now,
    /// for the next write.
    Applied { path: String, ancestor_hash: String },
    /// The file already said what the write asked for, so nothing was
    /// written: an equal value, or a base that already exists.
    Unchanged { path: String, ancestor_hash: String },
    /// The session proposes the write; the disk is unchanged.
    Proposed { path: String, proposal: String },
    /// The file changed since the caller read it.
    Stale { path: String, current_hash: String },
    /// A permission rule or a `base:before_write` policy refused the write.
    Refused { path: String, reason: String },
}

/// How a caller should treat a failed Bases call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The base, the note, the view or the kiln is absent.
    NotFound,
    /// The request cannot run as asked.
    Invalid,
    /// The daemon failed.
    Internal,
}
/// A failure that names its kind explicitly.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(super) struct Classified {
    pub kind: Failure,
    pub message: String,
}
pub(super) fn not_found(message: impl Into<String>) -> anyhow::Error {
    Classified {
        kind: Failure::NotFound,
        message: message.into(),
    }
    .into()
}
pub(super) fn internal(error: impl std::fmt::Display) -> anyhow::Error {
    Classified {
        kind: Failure::Internal,
        message: error.to_string(),
    }
    .into()
}
impl Failure {
    /// The kind of `error`: an explicit classification, else a missing file,
    /// else another I/O failure, else the request.
    pub fn of(error: &anyhow::Error) -> Self {
        for cause in error.chain() {
            if let Some(classified) = cause.downcast_ref::<Classified>() {
                return classified.kind;
            }
            if let Some(io) = cause.downcast_ref::<std::io::Error>() {
                return if io.kind() == std::io::ErrorKind::NotFound {
                    Self::NotFound
                } else {
                    Self::Internal
                };
            }
        }
        Self::Invalid
    }
    pub fn rpc_code(self) -> i32 {
        match self {
            Self::NotFound => NOT_FOUND,
            Self::Invalid => crate::protocol::INVALID_PARAMS,
            Self::Internal => crate::protocol::INTERNAL_ERROR,
        }
    }
}

fn parse<T: DeserializeOwned>(params: Json) -> Result<T> {
    serde_json::from_value(params).context("Invalid Bases parameters")
}
fn answer(outcome: WriteOutcome) -> Result<Json> {
    Ok(serde_json::to_value(outcome)?)
}

/// Run `operation` in the kiln at `root`, which is canonical, for `writer`.
pub(super) async fn execute(
    operation: BaseOperation,
    root: &Path,
    params: Json,
    writer: &Writer<'_>,
) -> Result<Json> {
    match operation {
        BaseOperation::List => {
            let files = entries_scoped(root, writer.scope(root).as_ref())
                .await?
                .into_iter()
                .filter(|e| {
                    crucible_core::kiln::KilnFileKind::of(Path::new(&e.path))
                        == crucible_core::kiln::KilnFileKind::Base
                })
                .map(|e| e.path)
                .collect::<Vec<_>>();
            Ok(serde_json::json!(files))
        }
        BaseOperation::Views => {
            let params: ViewsParams = parse(params)?;
            if let Source::Path { path } = &params.source {
                writer.read_path(root, &source_path(root, path)?)?;
            }
            let base = load(root, &params.source).await?;
            Ok(serde_json::json!(base
                .views
                .iter()
                .map(ViewSummary::from)
                .collect::<Vec<_>>()))
        }
        BaseOperation::Query => {
            let params: QueryParams = parse(params)?;
            let request = Query {
                kiln: params.kiln.unwrap_or_default(),
                source: params.source,
                view: params.view,
                host: params.host,
            };
            Ok(serde_json::to_value(
                query_scoped(root, &request, writer.scope(root).as_ref()).await?,
            )?)
        }
        BaseOperation::SetProperty => {
            answer(write::set_property(root, &parse(params)?, writer).await?)
        }
        BaseOperation::CreateEntry => {
            answer(write::create_entry(root, &parse(params)?, writer).await?)
        }
        BaseOperation::ReorderGroups => {
            answer(write::reorder_groups(root, &parse(params)?, writer).await?)
        }
        BaseOperation::EnsureBase => {
            answer(write::ensure_base(root, &parse(params)?, writer).await?)
        }
        BaseOperation::PendingWrites => Ok(serde_json::to_value(writer.pending_writes(root)?)?),
    }
}
