use anyhow::Result;
use thiserror::Error;

/// Errors specific to Burn providers
#[derive(Debug, Error)]
pub enum BurnProviderError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid backend configuration: {0}")]
    InvalidBackend(String),

    #[error("Backend not supported: {0:?}")]
    BackendNotSupported(crate::hardware::BackendType),

    #[error("Model loading failed: {0}")]
    ModelLoadingFailed(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Hardware detection failed: {0}")]
    HardwareDetectionFailed(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

/// Base provider functionality shared by all Burn providers
#[derive(Debug)]
pub struct BurnProviderBase {
    pub model_info: crate::models::ModelInfo,
    pub backend: crate::hardware::BackendType,
    pub config: crate::config::BurnConfig,
}

impl BurnProviderBase {
    /// Create a new provider base
    pub fn new(
        model_info: crate::models::ModelInfo,
        backend: crate::hardware::BackendType,
        config: crate::config::BurnConfig,
    ) -> Self {
        Self {
            model_info,
            backend,
            config,
        }
    }

    /// Validate that the backend is supported for the current hardware
    pub async fn validate_backend(&self) -> Result<()> {
        let hardware_info = crate::hardware::HardwareInfo::detect().await?;

        if !hardware_info.is_backend_supported(&self.backend) {
            return Err(BurnProviderError::BackendNotSupported(self.backend.clone()).into());
        }

        Ok(())
    }

    /// Check if the model is valid for the provider type
    pub fn validate_model(&self) -> Result<()> {
        if !self.model_info.is_complete() {
            return Err(BurnProviderError::ModelLoadingFailed(
                format!("Model {} is incomplete", self.model_info.name)
            ).into());
        }

        Ok(())
    }
}