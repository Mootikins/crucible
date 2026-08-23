//! Parser types, re-exported here so `traits::` keeps one module per domain.

pub use crate::parser::error::{ParserError, ParserResult};
pub use crate::parser::traits::ParserCapabilities;
pub use crate::parser::types::ParsedNote;
