use crucible_core::storage::StorageError;

/// Extension trait to convert `rusqlite::Error` results into `StorageResult`.
///
/// Replaces the verbose `.map_err(|e| StorageError::Backend(e.to_string()))` pattern.
pub(crate) trait SqliteResultExt<T> {
    fn sql(self) -> Result<T, StorageError>;
}

impl<T> SqliteResultExt<T> for Result<T, rusqlite::Error> {
    fn sql(self) -> Result<T, StorageError> {
        self.map_err(|e| StorageError::Backend(e.to_string()))
    }
}
