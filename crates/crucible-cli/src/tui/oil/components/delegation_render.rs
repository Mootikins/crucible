use crate::tui::oil::node::{col, row, scrollback, styled, text, Node, BRAILLE_SPINNER_FRAMES};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::utils::truncate_first_line;
use crate::tui::oil::viewport_cache::{CachedDelegation, DelegationStatus};
use std::time::Duration;

use super::tool_render::format_elapsed;

pub fn render_delegation(delegation: &CachedDelegation, spinner_frame: usize) -> Node {
    let theme = ThemeTokens::default_ref();
    let (icon, icon_style) = match delegation.status {
        DelegationStatus::Running => {
            let frame = BRAILLE_SPINNER_FRAMES[spinner_frame % BRAILLE_SPINNER_FRAMES.len()];
            (format!(" {} ", frame), Style::new().fg(theme.text_accent))
        }
        DelegationStatus::Completed => (" ✓ ".to_string(), Style::new().fg(theme.success)),
        DelegationStatus::Failed => (" ✗ ".to_string(), Style::new().fg(theme.error)),
    };

    let prompt_preview = truncate_first_line(&delegation.prompt, 60, true);

    let status_text = match delegation.status {
        DelegationStatus::Running => {
            let elapsed = delegation.elapsed();
            format_elapsed_display(elapsed)
        }
        DelegationStatus::Completed => delegation
            .summary
            .as_ref()
            .map(|s| format!(" → {}", truncate_first_line(s, 50, true)))
            .unwrap_or_default(),
        DelegationStatus::Failed => delegation
            .error
            .as_ref()
            .map(|e| format!(" → {}", truncate_first_line(e, 50, true)))
            .unwrap_or_default(),
    };

    let status_style = match delegation.status {
        DelegationStatus::Running => theme.dim(),
        DelegationStatus::Completed => theme.muted(),
        DelegationStatus::Failed => theme.error_style(),
    };

    let header = row([
        styled(icon, icon_style),
        styled("delegation", Style::new().fg(theme.text_primary)),
        styled(format!(" {}", prompt_preview), theme.muted()),
        styled(status_text, status_style),
    ]);

    scrollback(delegation.id.to_string(), [col([text(""), header])])
}

fn format_elapsed_display(elapsed: Duration) -> String {
    format!("  {}", format_elapsed(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_plain_text;
    use std::sync::Arc;

    #[test]
    fn render_delegation_running() {
        let mut delegation = CachedDelegation::new("deleg-1", "Analyze the code");
        delegation.status = DelegationStatus::Running;
        let node = render_delegation(&delegation, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("delegation"));
        assert!(plain.contains("Analyze the code"));
    }

    #[test]
    fn render_delegation_completed() {
        let mut delegation = CachedDelegation::new("deleg-1", "Analyze the code");
        delegation.status = DelegationStatus::Completed;
        delegation.summary = Some(Arc::from("Analysis complete"));
        let node = render_delegation(&delegation, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✓"));
        assert!(plain.contains("delegation"));
        assert!(plain.contains("Analysis complete"));
    }

    #[test]
    fn render_delegation_failed() {
        let mut delegation = CachedDelegation::new("deleg-1", "Analyze the code");
        delegation.status = DelegationStatus::Failed;
        delegation.error = Some(Arc::from("Connection timeout"));
        let node = render_delegation(&delegation, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✗"));
        assert!(plain.contains("delegation"));
        assert!(plain.contains("Connection timeout"));
    }

    #[test]
    fn render_delegation_truncates_long_prompt() {
        let long_prompt = "a".repeat(100);
        let delegation = CachedDelegation::new("deleg-1", &long_prompt);
        let node = render_delegation(&delegation, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("…"));
    }
}
