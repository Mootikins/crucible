pub mod routes;
pub mod server;
pub mod services;
#[cfg(test)]
pub mod test_support;

mod assets;
mod error;
mod events;
mod middleware;

pub use crucible_config::WebConfig;
pub use error::{Result, WebError};
pub use events::ChatEvent;
pub use server::start_server;
