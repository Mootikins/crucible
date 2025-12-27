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
                        false
                    }
                    BareKey::Esc | BareKey::Char('q') => {
                        hide_self();
                        false
                    }
                    _ => false,
                },

                _ => false,
            }
        }

        fn render(&mut self, _rows: usize, cols: usize) {
            // Box-drawing characters for clean UI
            const TOP_LEFT: &str = "\u{250c}"; // ┌
            const TOP_RIGHT: &str = "\u{2510}"; // ┐
            const BOTTOM_LEFT: &str = "\u{2514}"; // └
            const BOTTOM_RIGHT: &str = "\u{2518}"; // ┘
            const HORIZONTAL: &str = "\u{2500}"; // ─
            const VERTICAL: &str = "\u{2502}"; // │

            let title = " Agent Inbox ";
            let width = cols.min(50);

            // Top border with title
            let title_padding = width.saturating_sub(title.len() + 2);
            let left_pad = title_padding / 2;
            let right_pad = title_padding - left_pad;
            println!(
                "{}{}{}{}{}",
                TOP_LEFT,
                HORIZONTAL.repeat(left_pad),
                title,
                HORIZONTAL.repeat(right_pad),
                TOP_RIGHT
            );

            if self.inbox.is_empty() {
                println!("{}  (no items)", VERTICAL);
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
                        println!("{} {}", VERTICAL, item.status.section_name());
                    }

                    // Project header
                    if current_project != Some(&item.project) {
                        current_project = Some(&item.project);
                        println!("{}   {}", VERTICAL, item.project);
                    }

                    // Item
                    let marker = if item_index == self.selected {
                        "\u{25b6}" // ▶
                    } else {
                        " "
                    };
                    let text: String = item.text.chars().take(width - 8).collect();
                    println!("{} {} {}", VERTICAL, marker, text);

                    item_index += 1;
                }
            }

            // Bottom border with help
            println!("{}", VERTICAL);
            println!("{} j/k:nav  Enter:focus  esc:close", VERTICAL);
            println!(
                "{}{}{}",
                BOTTOM_LEFT,
                HORIZONTAL.repeat(width - 2),
                BOTTOM_RIGHT
            );
        }
    }
}
