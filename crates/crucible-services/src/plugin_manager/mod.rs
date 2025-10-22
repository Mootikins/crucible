//! # Plugin Manager
//!
//! A comprehensive plugin management system for the Crucible knowledge management platform.
//! This module provides process isolation, lifecycle management, resource monitoring,
//! security enforcement, and health monitoring for plugins.

pub mod config;
pub mod error;
pub mod types;
pub mod registry;
pub mod instance;
pub mod resource_manager;
pub mod security_manager;
pub mod health_monitor;
pub mod manager;

// Re-export main types and traits
pub use config::*;
pub use error::*;
pub use types::*;
pub use registry::{PluginRegistry, DefaultPluginRegistry, PluginInstaller, RegistryEvent};
pub use instance::{PluginInstance, DefaultPluginInstance, InstanceEvent};
pub use resource_manager::{ResourceManager, DefaultResourceManager, ResourceEvent};
pub use security_manager::{SecurityManager, DefaultSecurityManager, SecurityEvent};
pub use health_monitor::{HealthMonitor, DefaultHealthMonitor, HealthEvent};
pub use manager::PluginManagerService;