pub mod routes;
pub mod server;
pub mod services;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;

mod assets;
mod error;
mod events;
pub mod middleware;

pub use crucible_core::config::WebConfig;
pub use error::{Result, WebError};
pub use events::ChatEvent;
pub use server::start_server;
