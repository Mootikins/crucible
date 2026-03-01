use crate::error::{LuaError, LuaResult};

/// Extension trait to convert `Result<T, E: Display>` into `LuaResult<T>`.
///
/// Replaces the verbose `.map_err(|e| LuaError::Runtime(e.to_string()))` pattern.
pub trait LuaResultExt<T> {
    fn lua_runtime(self) -> LuaResult<T>;
}

impl<T, E: std::fmt::Display> LuaResultExt<T> for Result<T, E> {
    fn lua_runtime(self) -> LuaResult<T> {
        self.map_err(|e| LuaError::Runtime(e.to_string()))
    }
}
