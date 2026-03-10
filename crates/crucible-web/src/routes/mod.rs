mod chat;
mod health;
mod kiln;
mod mcp;
mod plugin;
mod project;
mod search;
mod session;
mod shell;

pub use chat::chat_routes;
pub use health::health_routes;
pub use mcp::mcp_routes;
pub use kiln::kiln_routes;
pub use plugin::plugin_routes;
pub use project::project_routes;
pub use search::search_routes;
pub use session::session_routes;
pub use shell::shell_routes;
