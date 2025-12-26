//! Zellij plugin implementation

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use zellij_tile::prelude::*;

    use crate::{file, Inbox, Status};

    #[derive(Default)]
    pub struct InboxPlugin {
        inbox: Inbox,
        selected: usize,
        inbox_path: Option<PathBuf>,
    }

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

    register_plugin!(InboxPlugin);

    impl ZellijPlugin for InboxPlugin {
        fn load(&mut self, _config: BTreeMap<String, String>) {
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

            // Get session name and build path
            // Note: We'll get this from SessionUpdate event
        }

        fn update(&mut self, event: Event) -> bool {
            match event {
                Event::SessionUpdate(session_info, _) => {
                    // Build inbox path from session name
                    let session_name = session_info.name;
                    let base = dirs::data_local_dir()
                        .unwrap_or_else(|| PathBuf::from("/tmp"))
                        .join("zellij-inbox");
                    self.inbox_path = Some(base.join(format!("{}.md", session_name)));

                    // Watch the file
                    if let Some(ref path) = self.inbox_path {
                        if let Some(path_str) = path.to_str() {
                            watch_filesystem_root(path_str);
                        }
                    }

                    self.load_inbox();
                    true
                }

                Event::FileSystemUpdate(_paths) => {
                    self.load_inbox();
                    true
                }

                Event::Key(Key::Up | Key::Char('k')) => {
                    self.selected = self.selected.saturating_sub(1);
                    true
                }

                Event::Key(Key::Down | Key::Char('j')) => {
                    if !self.inbox.is_empty() {
                        self.selected = (self.selected + 1).min(self.inbox.items.len() - 1);
                    }
                    true
                }

                Event::Key(Key::Enter) => {
                    if let Some(pane_id) = self.selected_pane_id() {
                        focus_terminal_pane(pane_id as i32, true);
                    }
                    hide_self();
                    false
                }

                Event::Key(Key::Esc | Key::Char('q')) => {
                    hide_self();
                    false
                }

                _ => false,
            }
        }

        fn render(&mut self, _rows: usize, cols: usize) {
            let title = "Agent Inbox";
            let width = cols.min(50);

            // Top border
            println!(
                "┌─ {} {}",
                title,
                "─".repeat(width.saturating_sub(title.len() + 4))
            );

            if self.inbox.is_empty() {
                println!("│ (no items)");
            } else {
                let mut current_status: Option<Status> = None;
                let mut current_project: Option<&str> = None;
                let mut item_index = 0;

                // Group and render
                for item in &self.inbox.items {
                    // Section header
                    if current_status != Some(item.status) {
                        current_status = Some(item.status);
                        current_project = None;
                        println!("│ {}", item.status.section_name());
                    }

                    // Project header
                    if current_project != Some(&item.project) {
                        current_project = Some(&item.project);
                        println!("│   {}", item.project);
                    }

                    // Item
                    let marker = if item_index == self.selected {
                        "▸"
                    } else {
                        " "
                    };
                    let text: String = item.text.chars().take(width - 8).collect();
                    println!("│ {} {}", marker, text);

                    item_index += 1;
                }
            }

            // Bottom border with help
            println!("│");
            println!("│ j/k navigate  ⏎ focus  esc close");
            println!("└{}", "─".repeat(width));
        }
    }
}
