use crate::node::*;
use crate::style::*;
use html_parser::{Dom, Element, Node as HtmlNode};

#[derive(Debug, thiserror::Error)]
pub enum HtmlError {
    #[error("HTML parse error: {0}")]
    ParseError(String),
    #[error("unsupported element: {0}")]
    UnsupportedElement(String),
}

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

/// A color: a name, `#rrggbb`, or `rgb(r, g, b)`. `None` for any other text.
///
/// The one color parser for HTML templates and Lua `cru.oil` nodes.
pub fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some(Color::DarkGray),
        "reset" => Some(Color::Reset),
        _ => {
            if let Some(hex) = s.strip_prefix('#').filter(|h| h.len() == 6) {
                let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                return Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
            }
            let inner = s.strip_prefix("rgb(")?.strip_suffix(')')?;
            let mut parts = inner.split(',').map(|p| p.trim().parse::<u8>());
            let (r, g, b) = (
                parts.next()?.ok()?,
                parts.next()?.ok()?,
                parts.next()?.ok()?,
            );
            parts.next().is_none().then_some(Color::Rgb(r, g, b))
        }
    }
}

fn apply_text_style(node: Node, el: &Element) -> Node {
    let mut style = Style::default();

    if let Some(Some(color_str)) = el.attributes.get("color") {
        if let Some(color) = parse_color(color_str) {
            style = style.fg(color);
        }
    }

    if let Some(Some(bg_str)) = el.attributes.get("bg") {
        if let Some(color) = parse_color(bg_str) {
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

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn a_color_is_a_name_a_hex_value_or_an_rgb_triple() {
        assert_eq!(parse_color("Red"), Some(Color::Red));
        assert_eq!(parse_color("dark_grey"), Some(Color::DarkGray));
        assert_eq!(parse_color("#ff8000"), Some(Color::Rgb(255, 128, 0)));
        assert_eq!(parse_color("rgb(1, 2, 3)"), Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn any_other_text_is_no_color() {
        for text in [
            "",
            "chartreuse",
            "#fff",
            "#gg0000",
            "rgb(1,2)",
            "rgb(1,2,3,4)",
            "rgb(256,0,0)",
        ] {
            assert_eq!(parse_color(text), None, "{text}");
        }
    }
}
