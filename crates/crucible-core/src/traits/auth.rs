//! Authentication types for LLM provider integrations.

use std::collections::HashMap;

/// Headers returned by authentication hooks.
///
/// Maps header names to their values (e.g., `"Authorization"` -> `"Bearer sk-..."`).
pub type AuthHeaders = HashMap<String, String>;
