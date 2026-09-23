use serde::{Deserialize, Serialize};

/// The client-facing status item. The daemon keeps the authored list; clients
/// only decide where and how it fits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDisplayItem {
    pub id: String,
    pub text: String,
    pub priority: u8,
    pub color_group: String,
    pub action: Option<String>,
    pub pinned: bool,
    pub plugin: String,
}
