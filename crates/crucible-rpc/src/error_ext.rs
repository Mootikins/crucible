use crucible_core::traits::chat::{ChatError, ChatResult};

/// Extension trait to convert `Result<T, E: Display>` into `ChatResult<T>`.
///
/// Replaces the verbose `.map_err(|e| ChatError::Communication(e.to_string()))` pattern.
pub trait ChatResultExt<T> {
    fn chat_comm(self) -> ChatResult<T>;
}

impl<T, E: std::fmt::Display> ChatResultExt<T> for Result<T, E> {
    fn chat_comm(self) -> ChatResult<T> {
        self.map_err(|e| ChatError::Communication(e.to_string()))
    }
}
