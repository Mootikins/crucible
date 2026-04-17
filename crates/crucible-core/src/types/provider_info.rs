//! Provider information exposed by the daemon to clients.
//!
//! `ProviderInfo` describes an available LLM provider — its backend type,
//! discovered models, endpoint, and availability status. Emitted as part of
//! the `providers_listed` session setup event and returned by the
//! `list_providers` RPC.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub provider_type: String,
    pub available: bool,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    pub endpoint: Option<String>,
    pub reason: Option<String>,
    pub is_local: bool,
}
