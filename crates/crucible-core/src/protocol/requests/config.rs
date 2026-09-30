//! Wire types of the `config.*` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// Request for `config.get` and `config.origin`.
///
/// With `key`, the daemon answers for the leaf at that dot-joined path.
/// Without it, the daemon answers for the whole store.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigLookupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request for `config.reset`, `config.pop` and `config.unset`: one leaf,
/// named by its dot-joined path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigKeyRequest {
    pub key: String,
}

/// Request for `config.set` and `config.save`: the values to merge.
///
/// The two methods take the same shape. They differ only in the layer that
/// they write.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigValuesRequest {
    pub values: serde_json::Map<String, serde_json::Value>,
}

/// Reply from `config.set`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigSetReply {
    /// Always true: `config.set` never partially refuses.
    pub ok: bool,
    /// The top-level keys the merge dropped because they name where the
    /// daemon acts (`kilns`, `kiln_path`, `projects`, and the like).
    pub rejected: Vec<String>,
}

/// Reply from `config.save`.
///
/// A refusal rides in the answer rather than in an error: refusal is per
/// leaf, the siblings the caller changed in the same call did save, and
/// `refused` carries the file and the line a human's config holds the key
/// on.
///
/// Moved here from `crucible-daemon`: the type it named
/// ([`crate::config::PinnedLeaf`]) already lived in core, so nothing kept it
/// out.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigSaveReply {
    /// Whether every leaf the caller sent reached the `Settings` layer.
    pub ok: bool,
    /// The leaves a pin refused, each with the source that holds it.
    pub refused: Vec<crate::config::PinnedLeaf>,
    /// The top-level keys that name where the daemon acts, which no save may
    /// write. They are dropped rather than refused, so they are reported apart
    /// from `refused`.
    pub rejected: Vec<String>,
}
