//! Permission system for Crucible tools
//!
//! This module handles permission checking for tool execution.
//! In the current phase, it provides a permissive default implementation
//! but establishes the interface for future security controls.

use crate::types::{ToolError, ToolExecutionRequest};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Permission types for tool execution
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Permission to execute a specific tool
    ExecuteTool(String),
    /// Permission to access the filesystem
    FilesystemRead,
    FilesystemWrite,
    /// Permission to access the network
    NetworkAccess,
    /// Permission to access the database
    DatabaseRead,
    DatabaseWrite,
    /// Administrator permission (all access)
    Admin,
}

/// Manager for handling tool permissions
#[derive(Debug, Clone)]
pub struct PermissionManager {
    /// Set of granted permissions
    granted_permissions: Arc<RwLock<HashSet<Permission>>>,
}

impl PermissionManager {
    /// Create a new permission manager with default permissions
    pub fn new() -> Self {
        let mut permissions = HashSet::new();
        // Default to permissive for now (Phase 3)
        permissions.insert(Permission::Admin);
        
        Self {
            granted_permissions: Arc::new(RwLock::new(permissions)),
        }
    }

    /// Create a permission manager with specific permissions
    pub fn with_permissions(permissions: Vec<Permission>) -> Self {
        let mut set = HashSet::new();
        for p in permissions {
            set.insert(p);
        }
        
        Self {
            granted_permissions: Arc::new(RwLock::new(set)),
        }
    }

    /// Check if a request is permitted
    pub fn check_permission(&self, request: &ToolExecutionRequest) -> Result<(), ToolError> {
        let permissions = self.granted_permissions.read().map_err(|_| {
            ToolError::Other("Failed to acquire permission lock".to_string())
        })?;

        // Admin has access to everything
        if permissions.contains(&Permission::Admin) {
            return Ok(());
        }

        // Check specific tool permission
        if permissions.contains(&Permission::ExecuteTool(request.tool_name.clone())) {
            return Ok(());
        }

        // TODO: Add more granular checks based on tool type/category if needed

        Err(ToolError::Other(format!(
            "Permission denied for tool: {}",
            request.tool_name
        )))
    }

    /// Grant a permission
    pub fn grant(&self, permission: Permission) -> Result<(), ToolError> {
        let mut permissions = self.granted_permissions.write().map_err(|_| {
            ToolError::Other("Failed to acquire permission lock".to_string())
        })?;
        
        permissions.insert(permission);
        Ok(())
    }

    /// Revoke a permission
    pub fn revoke(&self, permission: &Permission) -> Result<(), ToolError> {
        let mut permissions = self.granted_permissions.write().map_err(|_| {
            ToolError::Other("Failed to acquire permission lock".to_string())
        })?;
        
        permissions.remove(permission);
        Ok(())
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}
