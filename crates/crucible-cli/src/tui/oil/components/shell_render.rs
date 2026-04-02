//! Shell execution rendering component.
//!
//! Renders shell command executions with command line, exit code,
//! output tail, and optional output file path.

use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;
use crate::tui::oil::theme;
use crate::tui::oil::viewport_cache::CachedShellExecution;

/// Render a shell execution with command, exit code, and output.
pub fn render_shell_execution(shell: &CachedShellExecution) -> Node {
    let t = theme::active();
    let exit_style = if shell.exit_code == 0 {
        Style::new().fg(t.resolve_color(t.colors.success))
    } else {
        Style::new().fg(t.resolve_color(t.colors.error)).bold()
    };

    let header = row([
        styled(" $ ", Style::new().fg(t.resolve_color(t.colors.text_muted))),
        styled(
            shell.command.as_ref(),
            Style::new().fg(t.resolve_color(t.colors.text)),
        ),
        styled(format!("  exit {}", shell.exit_code), exit_style.dim()),
    ]);

    let tail_nodes: Vec<Node> = shell
        .output_tail
        .iter()
        .map(|line| {
            styled(
                format!("   {}", line),
                Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
            )
        })
        .collect();

    let path_node = shell
        .output_path
        .as_ref()
        .map(|p| {
            styled(
                format!("   → {}", p.display()),
                Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
            )
        })
        .unwrap_or(Node::Empty);

    col(std::iter::once(header)
        .chain(tail_nodes)
        .chain(std::iter::once(path_node)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn render_shell_execution_success() {
        let shell =
            CachedShellExecution::new("shell-1", "echo hello", 0, vec!["hello".to_string()], None);
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("$"));
        assert!(plain.contains("echo hello"));
        assert!(plain.contains("exit 0"));
    }

    #[test]
    fn render_shell_execution_failure() {
        let shell = CachedShellExecution::new("shell-1", "false", 1, vec![], None);
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("exit 1"));
    }

    #[test]
    fn render_shell_execution_with_output_tail() {
        let shell = CachedShellExecution::new(
            "shell-1",
            "ls",
            0,
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
            None,
        );
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("file1.rs"));
        assert!(plain.contains("file2.rs"));
    }

    #[test]
    fn render_shell_execution_with_output_path() {
        use std::path::PathBuf;
        let shell = CachedShellExecution::new(
            "shell-1",
            "long-running",
            0,
            vec![],
            Some(PathBuf::from("/tmp/output.txt")),
        );
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("→ /tmp/output.txt"));
    }
}
