//! Shell modal, interaction modal, and screen management for OilChatApp.

use std::path::PathBuf;
use std::process::Command;

use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermissionScope};

use crate::tui::oil::app::Action;
use crate::tui::oil::components::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, ShellHistoryItem, ShellModal,
    ShellModalMsg, ShellModalOutput, ShellStatus,
};
use crate::tui::oil::viewport_cache::CachedShellExecution;

use super::messages::ChatAppMsg;
use super::OilChatApp;

impl OilChatApp {
    pub(super) fn handle_shell_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let shell_cmd = cmd[1..].trim().to_string();
        if shell_cmd.is_empty() {
            self.notification_area
                .add(crucible_core::types::Notification::warning(
                    "Empty shell command".to_string(),
                ));
            return Action::Continue;
        }

        if !self
            .shell_history
            .shell_history
            .back()
            .is_some_and(|last| last == &shell_cmd)
        {
            self.push_shell_history(shell_cmd.clone());
        }
        self.shell_history.shell_history_index = None;

        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        match ShellModal::spawn(shell_cmd.clone(), working_dir) {
            Ok(modal) => {
                self.enter_alternate_screen();
                self.shell_modal = Some(modal);
            }
            Err(e) => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(e));
            }
        }

        Action::Continue
    }

    pub(super) fn handle_shell_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Action<ChatAppMsg> {
        let visible_lines = self.modal_visible_lines();

        if let Some(ref mut modal) = self.shell_modal {
            let output = modal.update(ShellModalMsg::Key(key), visible_lines);
            self.handle_shell_modal_output(output);
        }

        Action::Continue
    }

    pub(super) fn handle_shell_modal_output(&mut self, output: ShellModalOutput) {
        match output {
            ShellModalOutput::None => {}
            ShellModalOutput::Close(history_item) => {
                self.save_shell_output();

                self.message_queue.message_counter += 1;
                self.container_list
                    .add_shell_execution(CachedShellExecution::new(
                        format!("shell-{}", self.message_queue.message_counter),
                        &history_item.command,
                        history_item.exit_code,
                        history_item.output_tail,
                        history_item.output_path,
                    ));

                self.shell_modal = None;
                self.leave_alternate_screen();
            }
            ShellModalOutput::InsertOutput { content, truncated } => {
                let label = if truncated { " (truncated)" } else { "" };
                self.input.insert_str(&format!(
                    "Here is the output of a shell command I ran{}:\n\n```\n{}\n```\n",
                    label, content
                ));
            }
        }
    }

    pub(super) fn handle_interaction_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Action<ChatAppMsg> {
        let modal = match &mut self.interaction_modal {
            Some(m) => m,
            None => return Action::Continue,
        };

        match modal.update(InteractionModalMsg::Key(key)) {
            InteractionModalOutput::None => Action::Continue,
            InteractionModalOutput::Close => {
                self.close_interaction();
                Action::Continue
            }
            InteractionModalOutput::PermissionResponse {
                request_id,
                response,
            } => {
                if let Some(ref pattern) = response.pattern {
                    let config_scope = match response.scope {
                        PermissionScope::Project => {
                            Some(crucible_config::components::permissions::PermissionScope::Project)
                        }
                        PermissionScope::User => {
                            Some(crucible_config::components::permissions::PermissionScope::User)
                        }
                        _ => None,
                    };
                    if let Some(scope) = config_scope {
                        match crucible_config::components::permissions::write_permission_rule(
                            scope, pattern, None,
                        ) {
                            Ok(()) => {
                                let path = if response.scope == PermissionScope::User {
                                    "~/.config/crucible/config.toml"
                                } else {
                                    "crucible.toml"
                                };
                                self.notify_toast(format!("Rule saved to {path}"));
                            }
                            Err(e) => {
                                self.notify_toast(format!("Failed to save rule: {e}"));
                            }
                        }
                    }
                }
                self.close_interaction_and_show_next();
                Action::Send(ChatAppMsg::CloseInteraction {
                    request_id,
                    response: InteractionResponse::Permission(response),
                })
            }
            InteractionModalOutput::AskResponse {
                request_id,
                response,
            } => {
                self.close_interaction();
                Action::Send(ChatAppMsg::CloseInteraction {
                    request_id,
                    response,
                })
            }
            InteractionModalOutput::ToggleDiff => Action::Continue,
            InteractionModalOutput::Notify(msg) => {
                self.notify_toast(msg);
                Action::Continue
            }
        }
    }

    pub(super) fn close_interaction_and_show_next(&mut self) {
        self.interaction_modal = None;
        if let Some((next_id, next_perm)) = self.permission.permission_queue.pop_front() {
            self.interaction_modal = Some(InteractionModal::new(
                next_id,
                InteractionRequest::Permission(next_perm),
                self.permission.perm_show_diff,
            ));
        }
    }

    pub(super) fn modal_visible_lines(&self) -> usize {
        let (_, term_height) = self.terminal_size.get();
        let term_height = term_height as usize;
        term_height.saturating_sub(2)
    }

    pub(super) fn tick_shell_modal(&mut self) {
        let visible_lines = self.modal_visible_lines();
        if let Some(ref mut modal) = self.shell_modal {
            let output = modal.update(ShellModalMsg::Tick, visible_lines);
            self.handle_shell_modal_output(output);
        }
    }

    #[allow(dead_code)]
    pub(super) fn cancel_shell(&mut self) {
        if let Some(ref mut modal) = self.shell_modal {
            modal.cancel();
        }
    }

    #[allow(dead_code)]
    pub(super) fn close_shell_modal_with_history(&mut self, history_item: ShellHistoryItem) {
        self.message_queue.message_counter += 1;
        self.container_list
            .add_shell_execution(CachedShellExecution::new(
                format!("shell-{}", self.message_queue.message_counter),
                &history_item.command,
                history_item.exit_code,
                history_item.output_tail,
                history_item.output_path,
            ));
        self.shell_modal = None;
        self.leave_alternate_screen();
    }

    pub(super) fn enter_alternate_screen(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, EnterAlternateScreen, cursor::Hide);
        let _ = std::io::Write::flush(&mut stdout);
    }

    pub(super) fn leave_alternate_screen(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, cursor::Show);
        let _ = std::io::Write::flush(&mut stdout);
        self.needs_full_redraw = true;
    }

    pub(super) fn save_shell_output(&mut self) -> Option<PathBuf> {
        let session_dir = self.session_dir.clone()?;
        let modal = self.shell_modal.as_mut()?;
        modal.save_output(&session_dir)
    }

    pub(super) fn maybe_spill_tool_output(&mut self, name: &str) {
        if !self.container_list.tool_should_spill(name) {
            return;
        }

        let Some(session_dir) = self.session_dir.clone() else {
            return;
        };

        let tool_dir = session_dir.join("tools");
        if let Err(e) = std::fs::create_dir_all(&tool_dir) {
            tracing::error!(path = %tool_dir.display(), error = %e, "Failed to create tool output directory");
            return;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let name_slug: String = name
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.txt", timestamp, name_slug);
        let path = tool_dir.join(&filename);

        if let Some(output) = self.container_list.get_tool_output(name) {
            if let Err(e) = std::fs::write(&path, &output) {
                tracing::error!(path = %path.display(), error = %e, "Failed to write tool output");
                return;
            }
            self.container_list.set_tool_output_path(name, path);
        }
    }

    #[allow(dead_code)]
    pub(super) fn send_shell_output(&mut self, truncated: bool) {
        let path = self.save_shell_output();

        if let Some(ref modal) = self.shell_modal {
            let path_str = path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|n| format!("shell/{}", n))
                .unwrap_or_else(|| "(not saved)".to_string());

            let exit_str = match modal.status() {
                ShellStatus::Completed { exit_code } => format!("exit {}", exit_code),
                ShellStatus::Cancelled => "cancelled".to_string(),
                ShellStatus::Running => "running".to_string(),
            };

            let mut message = format!(
                "[Shell: {}]\n$ {} ({})\n\n",
                path_str,
                modal.command(),
                exit_str
            );

            let output_lines = modal.output_lines();
            if truncated {
                let total = output_lines.len();
                let show_lines = 50.min(total);
                if total > show_lines {
                    message.push_str(&format!(
                        "[Truncated: showing last {} of {} lines]\n\n",
                        show_lines, total
                    ));
                }
                for line in output_lines.iter().rev().take(show_lines).rev() {
                    message.push_str(line);
                    message.push('\n');
                }
            } else {
                for line in output_lines {
                    message.push_str(line);
                    message.push('\n');
                }
            }

            self.add_system_message(message);
        }
    }

    #[allow(dead_code)]
    pub(super) fn open_shell_output_in_editor(&mut self) {
        let path = match self.save_shell_output() {
            Some(p) => p,
            None => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(
                        "Failed to save output file".to_string(),
                    ));
                return;
            }
        };

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        crossterm::terminal::disable_raw_mode().ok();
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen).ok();

        let status = Command::new(&editor).arg(&path).status();

        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen).ok();
        crossterm::terminal::enable_raw_mode().ok();

        if let Err(e) = status {
            self.notification_area
                .add(crucible_core::types::Notification::warning(format!(
                    "Failed to open editor: {}",
                    e
                )));
        }
    }

    pub(super) fn notify_toast(&mut self, msg: impl Into<String>) {
        self.notification_area
            .add(crucible_core::types::Notification::toast(msg));
    }
}
