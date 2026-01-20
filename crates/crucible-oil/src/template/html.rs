use super::parse_color;
use crate::node::*;
use crate::style::*;
use html_parser::{Dom, Element, Node as HtmlNode};
use std::fmt;

#[derive(Debug)]
pub enum HtmlError {
    ParseError(String),
    UnsupportedElement(String),
}

impl fmt::Display for HtmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HtmlError::ParseError(msg) => write!(f, "HTML parse error: {}", msg),
            HtmlError::UnsupportedElement(tag) => write!(f, "unsupported element: {}", tag),
        }
    }
}

impl std::error::Error for HtmlError {}

pub type HtmlResult<T> = Result<T, HtmlError>;

pub fn html_to_node(html: &str) -> HtmlResult<Node> {
    let dom = Dom::parse(html).map_err(|e| HtmlError::ParseError(e.to_string()))?;

    if dom.children.is_empty() {
        return Ok(Node::Empty);
    }

    let nodes: Vec<Node> = dom
        .children
        .iter()
        .filter_map(|child| html_node_to_ink(child).ok())
        .filter(|n| !matches!(n, Node::Empty))
        .collect();

    match nodes.len() {
        0 => Ok(Node::Empty),
        1 => Ok(nodes.into_iter().next().unwrap()),
        _ => Ok(fragment(nodes)),
    }
}

fn html_node_to_ink(node: &HtmlNode) -> HtmlResult<Node> {
    match node {
        HtmlNode::Text(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                Ok(Node::Empty)
            } else {
                Ok(text(trimmed.to_string()))
            }
        }
        HtmlNode::Element(el) => element_to_node(el),
        HtmlNode::Comment(_) => Ok(Node::Empty),
    }
}

fn element_to_node(el: &Element) -> HtmlResult<Node> {
    let tag = el.name.to_lowercase();
    let children: Vec<Node> = el
        .children
        .iter()
        .filter_map(|c| html_node_to_ink(c).ok())
        .filter(|n| !matches!(n, Node::Empty))
        .collect();

    match tag.as_str() {
        "div" | "section" | "article" | "main" | "aside" | "col" => {
            let mut node = col(children);
            node = apply_common_attrs(node, el);
            Ok(node)
        }
        "span" | "row" => {
            let mut node = row(children);
            node = apply_common_attrs(node, el);
            Ok(node)
        }
        "p" | "text" => {
            let content = collect_text_content(el);
            let mut node = text(content);
            node = apply_text_style(node, el);
            Ok(node)
        }
        "b" | "strong" => {
            let content = collect_text_content(el);
            Ok(styled(content, Style::default().bold()))
        }
        "i" | "em" => {
            let content = collect_text_content(el);
            Ok(styled(content, Style::default().italic()))
        }
        "u" => {
            let content = collect_text_content(el);
            Ok(styled(content, Style::default().underline()))
        }
        "code" => {
            let content = collect_text_content(el);
            Ok(styled(content, Style::default().fg(Color::Cyan)))
        }
        "hr" => Ok(horizontal_rule()),
        "br" => Ok(text("\n".to_string())),
        "ul" => {
            let items: Vec<String> = el
                .children
                .iter()
                .filter_map(|c| {
                    if let HtmlNode::Element(li) = c {
                        if li.name.to_lowercase() == "li" {
                            return Some(collect_text_content(li));
                        }
                    }
                    None
                })
                .collect();
            Ok(bullet_list(items))
        }
        "ol" => {
            let items: Vec<String> = el
                .children
                .iter()
                .filter_map(|c| {
                    if let HtmlNode::Element(li) = c {
                        if li.name.to_lowercase() == "li" {
                            return Some(collect_text_content(li));
                        }
                    }
                    None
                })
                .collect();
            Ok(numbered_list(items))
        }
        "spacer" => Ok(spacer()),
        "spinner" => {
            let label = el.attributes.get("label").and_then(|v| v.clone());
            Ok(spinner(label, 0))
        }
        "badge" => {
            let content = collect_text_content(el);
            let style = Style::default().bold();
            Ok(badge(&content, style))
        }
        _ => {
            if children.is_empty() {
                let content = collect_text_content(el);
                if content.is_empty() {
                    Ok(Node::Empty)
                } else {
                    Ok(text(content))
                }
            } else if children.len() == 1 {
                Ok(children.into_iter().next().unwrap())
            } else {
                Ok(fragment(children))
            }
        }
    }
}

fn collect_text_content(el: &Element) -> String {
    el.children
        .iter()
        .map(|c| match c {
            HtmlNode::Text(t) => t.clone(),
            HtmlNode::Element(child) => collect_text_content(child),
            HtmlNode::Comment(_) => String::new(),
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn apply_common_attrs(mut node: Node, el: &Element) -> Node {
    if let Some(Some(gap_str)) = el.attributes.get("gap") {
        if let Ok(gap_val) = gap_str.parse::<u16>() {
            node = node.gap(Gap::all(gap_val));
        }
    }

    if let Some(Some(padding_str)) = el.attributes.get("padding") {
        if let Ok(p) = padding_str.parse::<u16>() {
            node = node.with_padding(Padding::all(p));
        }
    }

    if let Some(Some(border_str)) = el.attributes.get("border") {
        let border = match border_str.as_str() {
            "double" => Some(Border::Double),
            "rounded" => Some(Border::Rounded),
            "heavy" => Some(Border::Heavy),
            "single" => Some(Border::Single),
            _ => None,
        };
        if let Some(b) = border {
            node = node.with_border(b);
        }
    }

    if let Some(Some(justify_str)) = el.attributes.get("justify") {
        let justify = match justify_str.replace('-', "_").as_str() {
            "start" => JustifyContent::Start,
            "end" => JustifyContent::End,
            "center" => JustifyContent::Center,
            "space_between" => JustifyContent::SpaceBetween,
            "space_around" => JustifyContent::SpaceAround,
            "space_evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::Start,
        };
        node = node.justify(justify);
    }

    if let Some(Some(align_str)) = el.attributes.get("align") {
        let align = match align_str.as_str() {
            "start" => AlignItems::Start,
            "end" => AlignItems::End,
            "center" => AlignItems::Center,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::Start,
        };
        node = node.align(align);
    }

    node
}

fn apply_text_style(node: Node, el: &Element) -> Node {
    let mut style = Style::default();

    if let Some(Some(color_str)) = el.attributes.get("color") {
        if let Ok(color) = parse_color(color_str) {
            style = style.fg(color);
        }
    }

    if let Some(Some(bg_str)) = el.attributes.get("bg") {
        if let Ok(color) = parse_color(bg_str) {
            style = style.bg(color);
        }
    }

    if el.attributes.contains_key("bold") {
        style = style.bold();
    }
    if el.attributes.contains_key("italic") {
        style = style.italic();
    }
    if el.attributes.contains_key("underline") {
        style = style.underline();
    }

    if style != Style::default() {
        if let Node::Text(text_node) = node {
            return Node::Text(TextNode { style, ..text_node });
        }
    }

    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let node = html_to_node("<p>Hello</p>").unwrap();
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn test_div_to_col() {
        let node = html_to_node("<div><p>Line 1</p><p>Line 2</p></div>").unwrap();
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_span_to_row() {
        let node = html_to_node("<span><b>Bold</b> text</span>").unwrap();
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_unordered_list() {
        let node = html_to_node("<ul><li>Item 1</li><li>Item 2</li></ul>").unwrap();
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_ordered_list() {
        let node = html_to_node("<ol><li>First</li><li>Second</li></ol>").unwrap();
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_hr() {
        let node = html_to_node("<hr>").unwrap();
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn test_bold_text() {
        let node = html_to_node("<b>Bold</b>").unwrap();
        if let Node::Text(text_node) = node {
            assert!(text_node.style.bold);
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_italic_text() {
        let node = html_to_node("<i>Italic</i>").unwrap();
        if let Node::Text(text_node) = node {
            assert!(text_node.style.italic);
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_div_with_gap() {
        let node = html_to_node(r#"<div gap="2"></div>"#).unwrap();
        if let Node::Box(box_node) = node {
            assert_eq!(box_node.gap, Gap::all(2));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_empty_html() {
        let node = html_to_node("").unwrap();
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn test_whitespace_only() {
        let node = html_to_node("   \n  \t  ").unwrap();
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn test_nested_structure() {
        let html = r#"
            <div>
                <p>Header</p>
                <div>
                    <span><b>Bold</b> and <i>italic</i></span>
                </div>
            </div>
        "#;
        let node = html_to_node(html).unwrap();
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_spinner() {
        let node = html_to_node(r#"<spinner label="Loading..."></spinner>"#).unwrap();
        assert!(matches!(node, Node::Spinner(_)));
    }

    #[test]
    fn test_badge() {
        let node = html_to_node(r#"<badge>OK</badge>"#).unwrap();
        assert!(matches!(node, Node::Text(_)));
    }
}
