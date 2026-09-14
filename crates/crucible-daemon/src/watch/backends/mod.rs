//! Native file notifications, shared by all watches in a group.

mod notify_backend;
pub use notify_backend::NotifyWatcher;
