//! Markdown parsing for inbox files

use regex::Regex;
use std::sync::LazyLock;

use crate::{Inbox, InboxItem, Status};

// Regex patterns
static SECTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^## (.+)$").expect("valid regex"));

static PROJECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^### (.+)$").expect("valid regex"));

static ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^- \[(.)\] (.+?) \[pane:: (\d+)\]$").expect("valid regex"));

/// Parse an inbox from markdown content
pub fn parse(content: &str) -> Inbox {
    let mut inbox = Inbox::new();
    let mut current_status = Status::Waiting;
    let mut current_project = String::new();

    for line in content.lines() {
        let line = line.trim_end();

        // Check for section header
        if let Some(caps) = SECTION_RE.captures(line) {
            let section_name = caps.get(1).unwrap().as_str();
            current_status = match section_name {
                "Waiting for Input" => Status::Waiting,
                "Background" => Status::Working,
                _ => current_status,
            };
            continue;
        }

        // Check for project header
        if let Some(caps) = PROJECT_RE.captures(line) {
            current_project = caps.get(1).unwrap().as_str().to_string();
            continue;
        }

        // Check for item
        if let Some(caps) = ITEM_RE.captures(line) {
            let status_char = caps.get(1).unwrap().as_str().chars().next().unwrap();
            let text = caps.get(2).unwrap().as_str().to_string();
            let pane_id: u32 = caps.get(3).unwrap().as_str().parse().unwrap_or(0);

            let status = Status::from_char(status_char).unwrap_or(current_status);

            inbox.items.push(InboxItem {
                text,
                pane_id,
                project: current_project.clone(),
                status,
            });
        }
    }

    inbox
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let inbox = parse("");
        assert!(inbox.is_empty());
    }

    #[test]
    fn parse_single_item() {
        let content = r#"## Waiting for Input

### crucible
- [ ] claude-code: Auth question [pane:: 42]
"#;
        let inbox = parse(content);
        assert_eq!(inbox.items.len(), 1);
        assert_eq!(inbox.items[0].text, "claude-code: Auth question");
        assert_eq!(inbox.items[0].pane_id, 42);
        assert_eq!(inbox.items[0].project, "crucible");
        assert_eq!(inbox.items[0].status, Status::Waiting);
    }

    #[test]
    fn parse_multiple_sections() {
        let content = r#"## Waiting for Input

### crucible
- [ ] claude-code: Auth question [pane:: 42]

### k3s
- [ ] claude-code: Helm review [pane:: 17]

## Background

### crucible
- [/] indexer: Processing files [pane:: 5]
"#;
        let inbox = parse(content);
        assert_eq!(inbox.items.len(), 3);

        assert_eq!(inbox.items[0].project, "crucible");
        assert_eq!(inbox.items[0].status, Status::Waiting);

        assert_eq!(inbox.items[1].project, "k3s");
        assert_eq!(inbox.items[1].status, Status::Waiting);

        assert_eq!(inbox.items[2].project, "crucible");
        assert_eq!(inbox.items[2].status, Status::Working);
    }
}
