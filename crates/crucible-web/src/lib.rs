pub mod routes;
pub mod server;
pub mod services;

mod assets;
mod error;
mod events;

pub use error::{Result, WebError};
pub use events::ChatEvent;
pub use server::{start_server, WebConfig};
