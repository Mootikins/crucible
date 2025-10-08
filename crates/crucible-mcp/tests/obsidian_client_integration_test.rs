//! Integration tests for the Obsidian client
//!
//! This test suite validates the HTTP client integration with the Obsidian plugin API.
//! It uses a mock HTTP server to simulate API responses and test various scenarios.

mod integration;

// Re-export test modules for cargo test discovery
#[cfg(test)]
mod tests {
    // The tests are defined in the integration submodules
    // and will be discovered automatically by cargo test
}
