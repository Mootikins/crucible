//! Environment variable guard for test isolation.
//!
//! Provides RAII-based environment variable management to ensure tests don't
//! interfere with each other when setting environment variables.
//!
//! # Example
//!
//! ```rust,no_run
//! use crucible_core::test_support::EnvVarGuard;
//!
//! #[test]
//! fn test_with_env_var() {
//!     let _guard = EnvVarGuard::set("MY_VAR", "test_value".to_string());
//!     // MY_VAR is set to "test_value"
//!     assert_eq!(std::env::var("MY_VAR").unwrap(), "test_value");
//! } // guard is dropped here, restoring previous value
//! ```

/// RAII guard for environment variable changes.
///
/// Saves the previous value of an environment variable and restores it on drop.
/// If the variable didn't exist before, it is removed on drop.
pub struct EnvVarGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvVarGuard {
    /// Set an environment variable and return a guard that restores the previous value.
    ///
    /// # Arguments
    ///
    /// * `key` - The environment variable name (must be `'static`)
    /// * `value` - The new value to set
    ///
    /// # Returns
    ///
    /// A guard that will restore the previous value (or remove the variable) on drop.
    pub fn set(key: &'static str, value: String) -> Self {
        let old = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, old }
    }

    /// Remove an environment variable and return a guard that restores it on drop.
    ///
    /// If the variable was set before, it will be restored to its previous value on drop.
    /// If it wasn't set, it remains unset after drop.
    pub fn remove(key: &'static str) -> Self {
        let old = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, old }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(old) = self.old.clone() {
            std::env::set_var(self.key, old);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_guard_sets_and_restores() {
        let original = std::env::var("TEST_ENV_GUARD_VAR").ok();

        {
            let _guard = EnvVarGuard::set("TEST_ENV_GUARD_VAR", "test_value".to_string());
            assert_eq!(std::env::var("TEST_ENV_GUARD_VAR").unwrap(), "test_value");
        }

        // After guard drops, should be restored
        match original {
            Some(val) => assert_eq!(std::env::var("TEST_ENV_GUARD_VAR").unwrap(), val),
            None => assert!(std::env::var("TEST_ENV_GUARD_VAR").is_err()),
        }
    }

    #[test]
    fn env_guard_removes_if_not_existed() {
        // Ensure the var doesn't exist
        std::env::remove_var("TEST_ENV_GUARD_NEW_VAR");

        {
            let _guard = EnvVarGuard::set("TEST_ENV_GUARD_NEW_VAR", "new_value".to_string());
            assert_eq!(
                std::env::var("TEST_ENV_GUARD_NEW_VAR").unwrap(),
                "new_value"
            );
        }

        // After guard drops, should be removed
        assert!(std::env::var("TEST_ENV_GUARD_NEW_VAR").is_err());
    }
}
