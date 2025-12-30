//! Error types for Steel scripting

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SteelError {
    #[error("Steel engine creation failed: {0}")]
    Engine(String),

    #[error("Compilation error: {0}")]
    Compile(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Contract violation: {0}")]
    Contract(String),

    #[error("Conversion error: {0}")]
    Conversion(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Task join error: {0}")]
    TaskJoin(String),
}
