use crate::tui::ink::node::{BoxNode, Direction, InputNode, Node, SpinnerNode, TextNode};
use crate::tui::ink::style::Style;
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

        Node::Fragment(children) => {
            for child in children {
                render_node_to_string(child, width, output);
            }
        }
    }
}

fn render_text(text: &TextNode, width: usize, output: &mut String) {
    if text.content.is_empty() {
        return;
    }

    let styled_content = apply_style(&text.content, &text.style);

    if width == 0 || text.content.chars().count() <= width {
        output.push_str(&styled_content);
    } else {
        let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
        let wrapped: Vec<_> = wrap(&text.content, options);

        for (i, line) in wrapped.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&apply_style(line, &text.style));
        }
    }
}

fn render_box(boxnode: &BoxNode, width: usize, output: &mut String) {
    let inner_width = width
        .saturating_sub(boxnode.padding.horizontal() as usize)
        .saturating_sub(if boxnode.border.is_some() { 2 } else { 0 });

    let mut children_output: Vec<String> = Vec::new();

    for child in &boxnode.children {
        let mut child_str = String::new();
        render_node_to_string(child, inner_width, &mut child_str);
        if !child_str.is_empty() {
            children_output.push(child_str);
        }
    }

    match boxnode.direction {
        Direction::Column => {
            for (i, child_str) in children_output.iter().enumerate() {
                if i > 0 {
                    output.push('\n');
                }
                output.push_str(child_str);
            }
        }
        Direction::Row => {
            output.push_str(&children_output.join(""));
        }
    }
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
