use crate::tui::ink::ansi::visible_width;
use crate::tui::ink::node::{
    BoxNode, Direction, InputNode, Node, PopupNode, SpinnerNode, TextNode,
};
use crate::tui::ink::style::{Color, Style};
use crossterm::style::Stylize;
use std::io::{self, Write};
use textwrap::{wrap, Options, WordSplitter};

pub fn render_to_string(node: &Node, width: usize) -> String {
    let mut output = String::new();
    render_node_to_string(node, width, &mut output);
    output
}

fn render_node_to_string(node: &Node, width: usize, output: &mut String) {
    match node {
        Node::Empty => {}

        Node::Text(text) => {
            render_text(text, width, output);
        }

        Node::Box(boxnode) => {
            render_box(boxnode, width, output);
        }

        Node::Static(static_node) => {
            for child in &static_node.children {
                render_node_to_string(child, width, output);
            }
        }

        Node::Input(input) => {
            render_input(input, output);
        }

        Node::Spinner(spinner) => {
            render_spinner(spinner, output);
        }

        Node::Popup(popup) => {
            render_popup(popup, width, output);
        }

        Node::Fragment(children) => {
            for child in children {
                render_node_to_string(child, width, output);
            }
        }

        Node::Focusable(focusable) => {
            render_node_to_string(&focusable.child, width, output);
        }

        Node::ErrorBoundary(boundary) => {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut child_output = String::new();
                render_node_to_string(&boundary.child, width, &mut child_output);
                child_output
            }));

            match result {
                Ok(child_output) => output.push_str(&child_output),
                Err(_) => render_node_to_string(&boundary.fallback, width, output),
            }
        }
    }
}

fn render_text(text: &TextNode, width: usize, output: &mut String) {
    let styled_content = apply_style(&text.content, &text.style);

    if width == 0 || text.content.chars().count() <= width {
        output.push_str(&styled_content);
    } else {
        let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
        let wrapped: Vec<_> = wrap(&text.content, options);

        for (i, line) in wrapped.iter().enumerate() {
            if i > 0 {
                output.push_str("\r\n");
            }
            output.push_str(&apply_style(line, &text.style));
        }
    }
}

fn render_box(boxnode: &BoxNode, width: usize, output: &mut String) {
    let border_size = if boxnode.border.is_some() { 2 } else { 0 };
    let inner_width = width
        .saturating_sub(boxnode.padding.horizontal() as usize)
        .saturating_sub(border_size);

    let mut children_output: Vec<String> = Vec::new();

    match boxnode.direction {
        Direction::Column => {
            for child in &boxnode.children {
                if matches!(child, Node::Empty) {
                    continue;
                }
                let mut child_str = String::new();
                render_node_to_string(child, inner_width, &mut child_str);
                children_output.push(child_str);
            }
        }
        Direction::Row => {
            let all_text = boxnode.children.iter().all(|c| matches!(c, Node::Text(_)));
            if all_text {
                for child in &boxnode.children {
                    let mut child_str = String::new();
                    render_node_to_string(child, inner_width, &mut child_str);
                    if !child_str.is_empty() {
                        children_output.push(child_str);
                    }
                }
            } else {
                let mut remaining_width = inner_width;
                for child in &boxnode.children {
                    let mut child_str = String::new();
                    render_node_to_string(child, remaining_width, &mut child_str);
                    if !child_str.is_empty() {
                        let first_line = child_str.lines().next().unwrap_or("");
                        let child_width = visible_width(first_line);
                        remaining_width = remaining_width.saturating_sub(child_width);
                        children_output.push(child_str);
                    }
                }
            }
        }
    }

    let content = match boxnode.direction {
        Direction::Column => children_output.join("\r\n"),
        Direction::Row => children_output.join(""),
    };

    if let Some(border) = &boxnode.border {
        render_bordered_content(&content, border, width, &boxnode.style, output);
    } else {
        output.push_str(&content);
    }
}

fn render_bordered_content(
    content: &str,
    border: &crate::tui::ink::style::Border,
    width: usize,
    style: &Style,
    output: &mut String,
) {
    let chars = border.chars();
    let inner_width = width.saturating_sub(2);

    let top = format!(
        "{}{}{}",
        chars.top_left,
        chars.horizontal.to_string().repeat(inner_width),
        chars.top_right
    );
    output.push_str(&apply_style(&top, style));
    output.push_str("\r\n");

    for line in content.lines() {
        let visible_len = strip_ansi_codes(line).chars().count();
        let padding = inner_width.saturating_sub(visible_len);
        let padded_line = format!("{}{}", line, " ".repeat(padding));
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str(&padded_line);
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str("\r\n");
    }

    if content.is_empty() {
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str(&" ".repeat(inner_width));
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str("\r\n");
    }

    let bottom = format!(
        "{}{}{}",
        chars.bottom_left,
        chars.horizontal.to_string().repeat(inner_width),
        chars.bottom_right
    );
    output.push_str(&apply_style(&bottom, style));
}

fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn render_input(input: &InputNode, output: &mut String) {
    if input.value.is_empty() {
        if let Some(placeholder) = &input.placeholder {
            let styled = apply_style(placeholder, &Style::new().dim());
            output.push_str(&styled);
        }
    } else {
        let styled = apply_style(&input.value, &input.style);
        output.push_str(&styled);
    }
}

fn render_spinner(spinner: &SpinnerNode, output: &mut String) {
    let frame_char = spinner.current_char();
    let styled_spinner = apply_style(&frame_char.to_string(), &spinner.style);

    output.push_str(&styled_spinner);

    if let Some(label) = &spinner.label {
        output.push(' ');
        output.push_str(&apply_style(label, &spinner.style));
    }
}

fn render_popup(popup: &PopupNode, width: usize, output: &mut String) {
    let popup_width = width.saturating_sub(2);
    if popup_width == 0 || popup.items.is_empty() {
        return;
    }

    let popup_bg = Color::Rgb(45, 50, 60);
    let selected_bg = Color::Rgb(60, 70, 90);

    let visible_end = (popup.viewport_offset + popup.max_visible).min(popup.items.len());
    let visible_items = &popup.items[popup.viewport_offset..visible_end];

    for (i, item) in visible_items.iter().enumerate() {
        let actual_index = popup.viewport_offset + i;
        let is_selected = actual_index == popup.selected;
        let bg = if is_selected { selected_bg } else { popup_bg };

        let mut line = String::new();
        line.push(' ');

        if is_selected {
            line.push_str("▸ ");
        } else {
            line.push_str("  ");
        }

        if let Some(kind) = &item.kind {
            line.push_str(kind);
            line.push(' ');
        }

        line.push_str(&item.label);

        let label_width = visible_width(&line);

        if let Some(desc) = &item.description {
            let available = popup_width.saturating_sub(label_width + 3);
            if available > 10 {
                let truncated = if desc.chars().count() > available {
                    let s: String = desc.chars().take(available - 1).collect();
                    format!("{}…", s)
                } else {
                    desc.clone()
                };
                line.push_str("  ");
                let desc_style = Style::new().bg(bg).dim();
                output.push_str(&apply_style(&line, &Style::new().bg(bg)));
                line.clear();
                line.push_str(&truncated);
                let after_desc_width = label_width + 2 + visible_width(&truncated);
                let padding = popup_width.saturating_sub(after_desc_width);
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                output.push_str(&apply_style(&line, &desc_style));
            } else {
                let padding = popup_width.saturating_sub(label_width);
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                output.push_str(&apply_style(&line, &Style::new().bg(bg)));
            }
        } else {
            let padding = popup_width.saturating_sub(label_width);
            line.push_str(&" ".repeat(padding));
            line.push(' ');
            output.push_str(&apply_style(&line, &Style::new().bg(bg)));
        }

        if i < visible_items.len() - 1 {
            output.push_str("\r\n");
        }
    }
}

pub fn render_popup_standalone(popup: &PopupNode, width: usize) -> String {
    let mut output = String::new();
    render_popup(popup, width, &mut output);
    output
}

fn apply_style(content: &str, style: &Style) -> String {
    if style == &Style::default() {
        return content.to_string();
    }

    use crossterm::style::StyledContent;
    let ct_style = style.to_crossterm();
    format!("{}", StyledContent::new(ct_style, content))
}

pub fn print_to_stdout(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "{}", content)?;
    stdout.flush()
}

pub fn println_to_stdout(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", content)?;
    stdout.flush()
}
