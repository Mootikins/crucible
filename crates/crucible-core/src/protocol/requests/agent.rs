//! Wire types of the `agent` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// The body of `session.configure_agent`, inside `Scoped`.
///
/// `agent` stays a `Value` on purpose: the handler answers a distinct
/// `Invalid agent config: {e}` for an `agent` that is not a `SessionAgent`,
/// and typing the field here would fold that into the generic params error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentConfig {
    pub agent: serde_json::Value,
}

/// The body of `session.knob.get`, inside `Scoped`.
///
/// `session.knob.set` needs no sibling of this: its body IS
/// [`crate::types::KnobValue`], which already names its own knob.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KnobRef {
    pub knob: crate::types::SessionKnob,
}

/// The body of `session.set_plugin_approval`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginApprovalChange {
    pub plugin: String,
    pub approval: String,
}

/// The body of `session.get_plugin_approval`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginRef {
    pub plugin: String,
}

/// The body of `session.undo`, inside `Scoped`.
///
/// An absent `count` undoes one turn.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UndoCount {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// Request for `subagent.collect`: wait for background jobs to finish.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SubagentCollectRequest {
    pub job_ids: Vec<String>,
    /// How long to wait, in seconds. An absent value waits two minutes.
    #[serde(default = "default_collect_timeout")]
    pub timeout_secs: f64,
}

fn default_collect_timeout() -> f64 {
    120.0
}

/// Request for `models.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListAllModelsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
}

/// Request for `embeddings.models`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmbeddingModelsRequest {
    /// A name to resolve through the catalog, in any form the catalog accepts.
    ///
    /// The answer carries the canonical form as `resolved`, and an unknown
    /// name is an error that names the near entries. The caller therefore
    /// holds no matcher of its own, so no second matcher can drift from the
    /// catalog's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Fetch `model` into the cache before the daemon answers.
    ///
    /// One method, two questions, because the answer to the second is the
    /// first asked again: after a download the caller wants the row, and the
    /// row is what says where the files are.
    #[serde(default)]
    pub download: bool,
}

/// One local embedding model, as `embeddings.models` reports it.
///
/// The daemon owns the catalog because it links fastembed and holds the model
/// cache. This struct is the projection the CLI renders; it carries no
/// fastembed type, so a build without that feature still compiles.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmbeddingModelRow {
    /// The name to write in the config file.
    pub name: String,
    /// The width of the vector.
    pub dimensions: usize,
    /// The parameter count in millions, or `None` for a model Crucible does
    /// not curate.
    pub parameter_millions: Option<u32>,
    /// The longest input the model accepts, or `None` for a model Crucible
    /// does not curate.
    pub max_input_tokens: Option<u32>,
    /// The MTEB v1 English retrieval score, or `None` when nobody published
    /// one. Never a guess.
    pub retrieval_score: Option<f32>,
    /// Whether Crucible curates this model, so `download` can fetch it.
    pub curated: bool,
    /// One sentence on why to pick this model, or why not.
    pub note: String,
    /// Whether the files are already in the cache.
    pub downloaded: bool,
}

/// The answer to `embeddings.models`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmbeddingCatalog {
    /// Every model the daemon can run, ordered by name. Empty when the daemon
    /// was built without the `fastembed` feature.
    #[serde(default)]
    pub models: Vec<EmbeddingModelRow>,
    /// The model the daemon's own config names, when it names one.
    #[serde(default)]
    pub configured: Option<String>,
    /// The directory the daemon reads and writes models in.
    #[serde(default)]
    pub cache_dir: Option<String>,
    /// The canonical catalog name of the model the request named.
    #[serde(default)]
    pub resolved: Option<String>,
    /// The directory the requested download landed in.
    #[serde(default)]
    pub downloaded_to: Option<String>,
    /// The bytes that download occupies.
    ///
    /// Only for the model just fetched. Every row carried this once, which
    /// cost a directory walk per model on a listing that never prints it.
    #[serde(default)]
    pub downloaded_bytes: Option<u64>,
}

/// Request for `providers.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListProvidersRequest {
    #[serde(default)]
    pub kiln_path: Option<String>,
    /// `false` skips per-provider model discovery (which dials endpoints).
    /// Omitted means `true` for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_models: Option<bool>,
}

/// The body of `session.connect_kiln` and `session.disconnect_kiln`, inside
/// `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NamedKiln {
    /// The registry NAME of the kiln. The parse refuses a path, so a caller
    /// that sends a path gets an error, and no session attaches a directory
    /// that nobody registered.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub kiln: crate::config::KilnName,
}

/// The body of `session.set_workspace`, inside `Scoped`.
///
/// `workspace: None` detaches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkspaceChoice {
    /// The workspace path. An absent or `null` value detaches: the session
    /// then has no workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// What `models.list` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelsListReply {
    pub models: Vec<String>,
}

/// What `providers.list` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProvidersListReply {
    pub providers: Vec<crate::types::ProviderInfo>,
}

/// One ACP agent profile, with the availability probe's verdict: a row of
/// `agents.list_profiles`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentProfileEntry {
    pub name: String,
    /// The profile's description, or an empty string when it declares none.
    pub description: String,
    /// The command that spawns the agent, or an empty string when the
    /// profile names none. A profile with no command can never spawn, so it
    /// is never available.
    pub command: String,
    /// Whether the daemon ships this profile, rather than a config declaring
    /// it.
    pub is_builtin: bool,
    /// Whether the probe found the command on PATH and it answered
    /// `--version`.
    pub available: bool,
}

/// What `agents.list_profiles` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentProfilesReply {
    pub profiles: Vec<AgentProfileEntry>,
}

/// What `agents.list_cards` answers.
///
/// No `ToSchema`: `AgentCard` does not derive it (its fields would need to,
/// transitively), and no web route publishes this reply's OpenAPI shape
/// today.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentCardsListReply {
    pub cards: Vec<crate::agent::AgentCard>,
}

/// What `agents.resolve_profile` answers for a name it knows. `None` when
/// the daemon has no profile of that name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentProfileResolved {
    pub name: String,
    pub description: String,
    pub command: String,
    pub is_builtin: bool,
    pub args: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
}
