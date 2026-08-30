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

    /// A declaration the host cannot read — a tool parameter whose declared
    /// type is not a type. Separate from [`Self::LoadError`] because the
    /// discovery pass FAILS OPEN on a spec error (a plugin whose spec cannot
    /// be read still loads, and merely exports nothing), and an unreadable
    /// declaration must not take that path: it would reach the agent surface
    /// as a made-up shape.
    #[error("Invalid declaration: {0}")]
    InvalidDeclaration(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type LifecycleResult<T> = Result<T, LifecycleError>;
