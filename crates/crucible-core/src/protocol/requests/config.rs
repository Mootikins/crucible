//! Wire types of the `config.*` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// Request for `config.get` and `config.origin`.
///
/// With `key`, the daemon answers for the leaf at that dot-joined path.
/// Without it, the daemon answers for the whole store.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigLookupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request for `config.reset`, `config.pop` and `config.unset`: one leaf,
/// named by its dot-joined path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigKeyRequest {
    pub key: String,
}

/// Request for `config.set` and `config.save`: the values to merge.
///
/// The two methods take the same shape. They differ only in the layer that
/// they write.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConfigValuesRequest {
    pub values: serde_json::Map<String, serde_json::Value>,
}
