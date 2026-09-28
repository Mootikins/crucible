//! Wire types of the `common` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

// Re-export public types so the original `rpc_client::client::<Type>` paths
// still resolve after the split. Only types the parent `rpc_client` module
// re-exports externally need to land here; the rest remain reachable at
// `client::<submodule>::<Type>` if needed internally.
// `SessionCreateRequest` is exported (it was `#[cfg(test)]`-only, for the
// wire-format tests below) because the daemon's own `handle_session_create`
// now deserializes it: the client struct IS the server's contract rather than
// a shape the server re-derives by hand.

/// Daemon capabilities returned by `daemon.capabilities` RPC
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DaemonCapabilities {
    pub version: String,
    #[serde(default)]
    pub build_sha: Option<String>,
    pub protocol_version: String,
    pub capabilities: CapabilityFlags,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapabilityFlags {
    pub kilns: bool,
    pub sessions: bool,
    pub agents: bool,
    pub events: bool,
    pub model_switching: bool,
}

/// Empty request for methods that take no parameters.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmptyParams {}

/// Request for methods that take only a kiln path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnPathRequest {
    pub kiln: String,
}

/// Request for methods that take only a filesystem path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathRequest {
    pub path: String,
}

/// Request for methods that take only a name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NameRequest {
    pub name: String,
}

/// Request for `skills.list`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillsListRequest {
    pub kiln_path: String,
    /// The workspace whose skill roots are searched. Absent means no
    /// workspace, never the daemon's own working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_filter: Option<String>,
}

/// Request for `skills.get`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillsGetRequest {
    pub name: String,
    pub kiln_path: String,
    /// The workspace whose skill roots are searched. Absent means no
    /// workspace, never the daemon's own working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// Request for `skills.search`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillsSearchRequest {
    pub query: String,
    pub kiln_path: String,
    /// The workspace whose skill roots are searched. Absent means no
    /// workspace, never the daemon's own working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Absent means the server's own default (20), not zero — so the server
    /// keeps the `unwrap_or` rather than serde defaulting the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `agents.list_cards`.
///
/// Paths, not kiln names: the caller is `cru agents`, which knows the
/// directory it runs in and the kiln path its config names, and the daemon
/// resolves cards by directory (`agent_cards::card_directories`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentsListCardsRequest {
    /// The workspace a session started here would attach.
    pub workspace: String,
    /// The kiln whose `.crucible/agents/` is searched. `None` means no kiln.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    Match,
    Mismatch { client: String, daemon: String },
}

impl VersionCheck {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }
}

/// The value that an absent `bool` field with a `true` default gets.
pub(super) fn default_true() -> bool {
    true
}

/// Read a `null` field as the default value of its type.
///
/// `#[serde(default)]` covers an absent field only. Some callers send an
/// explicit `null`, and the handler read that as "absent" before it read a
/// typed request. This keeps both spellings valid.
pub(super) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::Deserialize<'de>,
{
    use serde::Deserialize;
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

/// Read the caller's kiln set from `kilns` (an array) or `kiln` (one string).
///
/// The four methods that span the session backlog take a kiln set. Callers
/// from before the kiln flatten send one kiln as `kiln`. The `kiln` alias and
/// the form with one string keep them valid. A `null` reads as the empty set.
pub(super) fn kiln_set<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(kiln)) => vec![kiln],
        Some(OneOrMany::Many(kilns)) => kilns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, serde::Deserialize)]
    struct Scoped {
        #[serde(default, alias = "kiln", deserialize_with = "kiln_set")]
        kilns: Vec<String>,
    }

    fn kilns(value: serde_json::Value) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_value::<Scoped>(value).map(|s| s.kilns)
    }

    #[test]
    fn a_kiln_set_reads_every_spelling_that_callers_send() {
        assert_eq!(kilns(json!({})).unwrap(), Vec::<String>::new());
        assert_eq!(
            kilns(json!({ "kilns": null })).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(kilns(json!({ "kilns": [] })).unwrap(), Vec::<String>::new());
        assert_eq!(kilns(json!({ "kilns": ["a", "b"] })).unwrap(), ["a", "b"]);
        assert_eq!(kilns(json!({ "kiln": "a" })).unwrap(), ["a"]);
    }

    /// A member that is not a string names no kiln. The parse refuses it. It
    /// must not read as an empty set, because an empty set widens
    /// `session.list` to every session.
    #[test]
    fn a_kiln_set_with_a_member_that_is_not_a_string_is_refused() {
        assert!(kilns(json!({ "kilns": [7] })).is_err());
        assert!(kilns(json!({ "kiln": 7 })).is_err());
    }
}
