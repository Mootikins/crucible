use crate::storage::StorageError;

/// Extension trait to convert any `Display` error results into `StorageResult`.
///
/// Replaces the verbose `.map_err(|e| StorageError::Backend(e.to_string()))` pattern.
pub trait StorageResultExt<T> {
    fn storage_backend(self) -> Result<T, StorageError>;
}

impl<T, E: std::fmt::Display> StorageResultExt<T> for Result<T, E> {
    fn storage_backend(self) -> Result<T, StorageError> {
        self.map_err(|e| StorageError::Backend(e.to_string()))
    }
}
