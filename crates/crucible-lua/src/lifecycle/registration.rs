use super::PluginManager;
use crate::annotations::{DiscoveredCommand, DiscoveredHandler, DiscoveredTool, DiscoveredView};
use std::sync::atomic::{AtomicU64, Ordering};

static REGISTRATION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Handle for unregistering programmatically-added items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegistrationHandle(pub(super) u64);

impl RegistrationHandle {
    pub(super) fn new() -> Self {
        Self(REGISTRATION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone)]
pub(super) struct RegisteredItem<T> {
    pub(super) item: T,
    pub(super) handle: RegistrationHandle,
    pub(super) owner: Option<String>,
}

impl PluginManager {
    pub fn register_tool(
        &mut self,
        tool: DiscoveredTool,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.tools.push(RegisteredItem {
            item: tool,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_command(
        &mut self,
        command: DiscoveredCommand,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.commands.push(RegisteredItem {
            item: command,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_view(
        &mut self,
        view: DiscoveredView,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.views.push(RegisteredItem {
            item: view,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_handler(
        &mut self,
        handler: DiscoveredHandler,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.handlers.push(RegisteredItem {
            item: handler,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn unregister(&mut self, handle: RegistrationHandle) -> bool {
        let mut removed = false;

        if let Some(pos) = self.tools.iter().position(|t| t.handle == handle) {
            self.tools.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.commands.iter().position(|c| c.handle == handle) {
            self.commands.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.views.iter().position(|v| v.handle == handle) {
            self.views.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.handlers.iter().position(|h| h.handle == handle) {
            self.handlers.remove(pos);
            removed = true;
        }

        removed
    }

    pub fn unregister_by_owner(&mut self, owner: &str) -> usize {
        let before =
            self.tools.len() + self.commands.len() + self.views.len() + self.handlers.len();

        let matches_owner =
            |item_owner: &Option<String>| item_owner.as_ref().is_some_and(|o| o == owner);

        self.tools.retain(|t| !matches_owner(&t.owner));
        self.commands.retain(|c| !matches_owner(&c.owner));
        self.views.retain(|v| !matches_owner(&v.owner));
        self.handlers.retain(|h| !matches_owner(&h.owner));

        let after = self.tools.len() + self.commands.len() + self.views.len() + self.handlers.len();
        before - after
    }
}
