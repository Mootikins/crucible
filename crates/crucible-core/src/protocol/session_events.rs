//! Strongly-typed payloads for session setup events.
//!
//! The daemon emits these as `SessionEventMessage::data` values during the
//! setup task that runs immediately after `session.create`. Defining the
//! payloads here gives daemon emitters and CLI consumers a single compile-
//! time contract for the seven setup event shapes.
//!
//! Event name ↔ payload mapping:
//!
//! | event name                 | payload type                     |
//! |----------------------------|----------------------------------|
//! | `session_initialized`      | [`SessionInitializedPayload`]    |
//! | `providers_listed`         | [`ProvidersListedPayload`]       |
//! | `context_limit_resolved`   | [`ContextLimitResolvedPayload`]  |
//! | `workspace_indexed`        | [`WorkspaceIndexedPayload`]      |
//! | `kiln_notes_indexed`       | [`KilnNotesIndexedPayload`]      |
//! | `plugins_discovered`       | [`PluginsDiscoveredPayload`]     |
//! | `mcp_servers_ready`        | [`McpServersReadyPayload`]       |

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::mcp_status::McpServerInfo;
use crate::types::{PluginStatusEntry, ProviderInfo};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInitializedPayload {
    pub model: String,
    pub mode: String,
    pub agent_name: Option<String>,
    pub kiln_path: PathBuf,
    pub workspace_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvidersListedPayload {
    pub providers: Vec<ProviderInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextLimitResolvedPayload {
    pub limit: usize,
    pub source: ContextLimitSource,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextLimitSource {
    ProviderApi,
    Config,
    Default,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceIndexedPayload {
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KilnNotesIndexedPayload {
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginsDiscoveredPayload {
    pub plugins: Vec<PluginStatusEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServersReadyPayload {
    pub servers: Vec<McpServerInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_initialized_shape() {
        let p = SessionInitializedPayload {
            model: "glm-5".into(),
            mode: "normal".into(),
            agent_name: None,
            kiln_path: PathBuf::from("/k"),
            workspace_path: PathBuf::from("/w"),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["model"], "glm-5");
        assert_eq!(v["mode"], "normal");
        assert!(v["agent_name"].is_null());
        assert_eq!(v["kiln_path"], "/k");
        assert_eq!(v["workspace_path"], "/w");
    }

    #[test]
    fn session_initialized_shape_with_agent() {
        let p = SessionInitializedPayload {
            model: "sonnet-4".into(),
            mode: "plan".into(),
            agent_name: Some("claude".into()),
            kiln_path: PathBuf::from("/kiln"),
            workspace_path: PathBuf::from("/ws"),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["agent_name"], "claude");
    }

    #[test]
    fn providers_listed_shape() {
        let p = ProvidersListedPayload {
            providers: vec![ProviderInfo {
                name: "OpenAI".into(),
                provider_type: "openai".into(),
                available: true,
                default_model: Some("gpt-4o".into()),
                models: vec!["gpt-4o".into()],
                endpoint: Some("https://api.openai.com/v1".into()),
                reason: Some("config".into()),
                is_local: false,
            }],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v["providers"].is_array());
        assert_eq!(v["providers"][0]["name"], "OpenAI");
        assert_eq!(v["providers"][0]["provider_type"], "openai");
        assert_eq!(v["providers"][0]["available"], true);
        assert_eq!(v["providers"][0]["is_local"], false);
    }

    #[test]
    fn context_limit_resolved_shape() {
        let p = ContextLimitResolvedPayload {
            limit: 128_000,
            source: ContextLimitSource::ProviderApi,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["limit"], 128_000);
        assert_eq!(v["source"], "provider_api");
    }

    #[test]
    fn context_limit_source_snake_case() {
        assert_eq!(
            serde_json::to_value(ContextLimitSource::ProviderApi).unwrap(),
            serde_json::Value::String("provider_api".into()),
        );
        assert_eq!(
            serde_json::to_value(ContextLimitSource::Config).unwrap(),
            serde_json::Value::String("config".into()),
        );
        assert_eq!(
            serde_json::to_value(ContextLimitSource::Default).unwrap(),
            serde_json::Value::String("default".into()),
        );

        // round-trip deserialization
        let back: ContextLimitSource =
            serde_json::from_value(serde_json::Value::String("provider_api".into())).unwrap();
        assert_eq!(back, ContextLimitSource::ProviderApi);
    }

    #[test]
    fn workspace_indexed_shape() {
        let p = WorkspaceIndexedPayload {
            files: vec!["src/lib.rs".into(), "README.md".into()],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["files"], serde_json::json!(["src/lib.rs", "README.md"]));
    }

    #[test]
    fn kiln_notes_indexed_shape() {
        let p = KilnNotesIndexedPayload {
            notes: vec!["Daily/2026-04-17.md".into()],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["notes"], serde_json::json!(["Daily/2026-04-17.md"]));
    }

    #[test]
    fn plugins_discovered_shape() {
        let p = PluginsDiscoveredPayload {
            plugins: vec![PluginStatusEntry {
                name: "kiln-expert".into(),
                version: "0.1.0".into(),
                state: "loaded".into(),
                error: None,
            }],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v["plugins"].is_array());
        assert_eq!(v["plugins"][0]["name"], "kiln-expert");
        assert_eq!(v["plugins"][0]["version"], "0.1.0");
        assert_eq!(v["plugins"][0]["state"], "loaded");
        assert!(v["plugins"][0]["error"].is_null());
    }

    #[test]
    fn mcp_servers_ready_shape() {
        let p = McpServersReadyPayload {
            servers: vec![McpServerInfo {
                name: "context7".into(),
                prefix: "c7".into(),
                tools: vec!["query-docs".into(), "resolve-library-id".into()],
                connected: true,
            }],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v["servers"].is_array());
        assert_eq!(v["servers"][0]["name"], "context7");
        assert_eq!(v["servers"][0]["prefix"], "c7");
        assert_eq!(v["servers"][0]["connected"], true);
        assert_eq!(
            v["servers"][0]["tools"],
            serde_json::json!(["query-docs", "resolve-library-id"]),
        );
    }
}
