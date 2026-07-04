//! Serde Serializer for Markdown output
//!
//! This module provides a custom serde Serializer that outputs Markdown
//! instead of JSON. Types that derive `Serialize` can be rendered to
//! Markdown using the same API as `serde_json`.
//!
//! # Example
//!
//! ```ignore
//! use crate::serde_md;
//!
//! #[derive(serde::Serialize)]
//! struct Message { role: String, content: String }
//!
//! let msg = Message { role: "user".into(), content: "Hello!".into() };
//! let md = serde_md::to_string(&msg).unwrap();
//! ```

mod error;
mod serializer;

pub use error::{Error, Result};
pub use serializer::{to_string, to_string_pretty, Serializer};
