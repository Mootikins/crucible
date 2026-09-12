mod agents;
mod auth;
mod canvas;
mod chat;
mod config;
mod fs;
mod health;
mod helpers;
mod kiln;
mod layout;
mod mcp;
mod plugin;
mod plugin_caller;
mod project;
mod scm;
mod search;
mod session;
mod session_commands;
mod session_config;
mod session_status;
mod shell;
mod skills;
mod surface;
mod terminal;
mod webhook;

pub use agents::agents_routes;
pub use auth::auth_routes;
pub use canvas::canvas_routes;
pub use chat::chat_routes;
pub use config::config_routes;
pub use fs::fs_routes;
pub use health::health_routes;
pub use kiln::kiln_routes;
pub use layout::layout_routes;
pub use mcp::mcp_routes;
/// `PublicationChangedEvent` is exported for the wire name it carries.
///
/// `events.rs`'s `every_side_channel_event_name_has_a_frontend_listener` reads
/// `EVENT_NAME` off the compiled const rather than out of the source text, so
/// the const must leave the module. `SurfaceChangedEvent` below is the twin.
pub use plugin::{plugin_routes, PublicationChangedEvent};
pub use plugin_caller::{PluginCaller, APP_CALLER, PLUGIN_CALLER_HEADER};
pub use project::project_routes;
pub use scm::scm_routes;
pub use search::search_routes;
pub use session::{session_routes_fail_closed, session_routes_with, EndpointPolicy};
pub use shell::shell_routes;
pub use skills::skills_routes;
pub use surface::{surface_routes, SurfaceChangedEvent};
pub use terminal::terminal_routes;
pub use webhook::webhook_routes;
