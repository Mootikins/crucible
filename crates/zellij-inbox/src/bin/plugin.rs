//! Zellij plugin binary entry point
//!
//! This binary is compiled to WASM for the Zellij plugin.
//! The register_plugin! macro generates the main() function.
//!
//! Build with: cargo build --release --target wasm32-wasip1 --bin zellij-inbox-plugin

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This binary is only for WASM. Use: cargo build --target wasm32-wasip1 --bin zellij-inbox-plugin");
    std::process::exit(1);
}

// ============================================================================
// WASM Plugin Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
use std::path::PathBuf;

#[cfg(target_arch = "wasm32")]
use zellij_tile::prelude::*;

#[cfg(target_arch = "wasm32")]
use zellij_inbox::{file, tui, Inbox};

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct InboxPlugin {
    inbox: Inbox,
    selected: usize,
    inbox_path: Option<PathBuf>,
    /// Track visibility for toggle detection
    visible: bool,
}

#[cfg(target_arch = "wasm32")]
impl InboxPlugin {
    fn load_inbox(&mut self) {
        if let Some(ref path) = self.inbox_path {
            if let Ok(inbox) = file::load(path) {
                self.inbox = inbox;
                // Clamp selection
                if !self.inbox.is_empty() && self.selected >= self.inbox.items.len() {
                    self.selected = self.inbox.items.len() - 1;
                }
            }
        }
    }

    fn selected_pane_id(&self) -> Option<u32> {
        self.inbox.items.get(self.selected).map(|i| i.pane_id)
    }
}

#[cfg(target_arch = "wasm32")]
register_plugin!(InboxPlugin);

#[cfg(target_arch = "wasm32")]
impl ZellijPlugin for InboxPlugin {
    fn load(&mut self, config: BTreeMap<String, String>) {
        // Request permissions
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);

        // Subscribe to events
        subscribe(&[
            EventType::Key,
            EventType::FileSystemUpdate,
            EventType::SessionUpdate,
        ]);

        // Check for explicit inbox_file config
        if let Some(path) = config.get("inbox_file") {
            self.inbox_path = Some(PathBuf::from(path));
            watch_filesystem();
            self.load_inbox();
        }
        // Otherwise, we'll get session name from SessionUpdate event
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::SessionUpdate(session_list, _) => {
                // Skip if inbox_file was explicitly configured
                if self.inbox_path.is_some() {
                    return false;
                }

                // Find the current session (the one marked as is_current_session)
                if let Some(current) = session_list.iter().find(|s| s.is_current_session) {
                    let session_name = &current.name;
                    self.inbox_path = Some(file::inbox_path_for_session(session_name));

                    // Watch for file changes
                    watch_filesystem();

                    self.load_inbox();
                }
                true
            }

            Event::FileSystemUpdate(_paths) => {
                self.load_inbox();
                true
            }

            Event::Key(key) if key.has_no_modifiers() => match key.bare_key {
                BareKey::Up | BareKey::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                    true
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.inbox.is_empty() {
                        self.selected = (self.selected + 1).min(self.inbox.items.len() - 1);
                    }
                    true
                }
                BareKey::Enter => {
                    if let Some(pane_id) = self.selected_pane_id() {
                        focus_terminal_pane(pane_id, true);
                    }
                    hide_self();
                    self.visible = false;
                    false
                }
                BareKey::Esc | BareKey::Char('q') => {
                    hide_self();
                    self.visible = false;
                    false
                }
                _ => false,
            },

            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Track that we're visible (for toggle detection)
        self.visible = true;

        let opts = tui::RenderOptions {
            width: cols,
            height: rows,
        };
        let output = tui::render_tui_full(&self.inbox, self.selected, opts);
        print!("{}", output);
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // Handle toggle message from CLI/keybind
        if pipe_message.name == "toggle" {
            if self.visible {
                hide_self();
                self.visible = false;
                return false;
            }
            // Not visible yet, will render and become visible
            return true;
        }
        false
    }
}
