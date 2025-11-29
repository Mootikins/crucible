pub mod detect;
pub mod embed;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "benchmarks")]
pub mod bench;

pub mod llm;
pub mod models;