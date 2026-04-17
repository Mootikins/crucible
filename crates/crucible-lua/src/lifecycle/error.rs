use crate::manifest::ManifestError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),

    #[error("Dependency not satisfied: {plugin} requires {dependency}")]
    DependencyNotSatisfied { plugin: String, dependency: String },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Load error: {0}")]
    LoadError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type LifecycleResult<T> = Result<T, LifecycleError>;
