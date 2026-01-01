//! Text search backends

pub mod factory;
pub mod regex_backend;
pub mod ripgrep_backend;
pub mod wikilink;

pub use factory::{create_text_searcher, detect_backend, SearchBackend};
pub use regex_backend::RegexSearcher;
pub use ripgrep_backend::RipgrepSearcher;
pub use wikilink::find_backlinks;
