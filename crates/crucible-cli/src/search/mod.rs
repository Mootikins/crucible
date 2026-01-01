//! Text search backends

pub mod regex_backend;
pub mod ripgrep_backend;

pub use regex_backend::RegexSearcher;
pub use ripgrep_backend::RipgrepSearcher;
