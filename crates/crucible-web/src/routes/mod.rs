mod auth;
mod canvas;
mod chat;
mod config;
mod events;
mod health;
mod helpers;
mod kiln;
mod layout;
mod plugin;
mod plugin_caller;
mod project;
mod rpc;
mod scm;
mod search;
mod session;
mod session_commands;
mod session_config;
mod session_status;
mod surface;
mod terminal;
mod webhook;

pub use auth::auth_routes;
pub use canvas::canvas_routes;
pub use chat::chat_routes;
pub use config::config_routes;
pub use events::{events_routes, ProposalChangedEvent};
pub use health::health_routes;
pub use kiln::kiln_routes;
pub use layout::layout_routes;
/// `PublicationChangedEvent` is exported for the wire name it carries.
///
/// `events.rs`'s `every_side_channel_event_name_has_a_frontend_listener` reads
/// `EVENT_NAME` off the compiled const rather than out of the source text, so
/// the const must leave the module. `SurfaceChangedEvent` below is the twin.
pub use plugin::{plugin_routes, PublicationChangedEvent};
pub use plugin_caller::{PluginCaller, APP_CALLER, PLUGIN_CALLER_HEADER};
pub use project::project_routes;
pub use rpc::rpc_routes;
pub use scm::scm_routes;
pub use search::search_routes;
pub use session::session_routes;
pub use surface::SurfaceChangedEvent;
pub use terminal::terminal_routes;
pub use webhook::webhook_routes;

mod bases;
pub use bases::bases_routes;
