use crate::focus::FocusId;
use crate::node::*;
use crate::overlay::OverlayAnchor;
use crate::style::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeSpec {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<NodeSpec>),
    Object(HashMap<String, NodeSpec>),
}

impl NodeSpec {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            NodeSpec::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            NodeSpec::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            NodeSpec::Int(n) => Some(*n),
            NodeSpec::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            NodeSpec::Float(f) => Some(*f),
            NodeSpec::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> Option<u16> {
        self.as_i64().and_then(|n| u16::try_from(n).ok())
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_i64().and_then(|n| usize::try_from(n).ok())
    }

    pub fn as_array(&self) -> Option<&[NodeSpec]> {
        match self {
            NodeSpec::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, NodeSpec>> {
        match self {
            NodeSpec::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

impl From<&str> for NodeSpec {
    fn from(s: &str) -> Self {
        NodeSpec::String(s.to_string())
    }
}

impl From<String> for NodeSpec {
    fn from(s: String) -> Self {
        NodeSpec::String(s)
    }
}

impl From<bool> for NodeSpec {
    fn from(b: bool) -> Self {
        NodeSpec::Bool(b)
    }
}

impl From<i64> for NodeSpec {
    fn from(n: i64) -> Self {
        NodeSpec::Int(n)
    }
}

impl From<i32> for NodeSpec {
    fn from(n: i32) -> Self {
        NodeSpec::Int(n as i64)
    }
}

impl From<f64> for NodeSpec {
    fn from(f: f64) -> Self {
        NodeSpec::Float(f)
    }
}

impl<T: Into<NodeSpec>> From<Vec<T>> for NodeSpec {
    fn from(v: Vec<T>) -> Self {
        NodeSpec::Array(v.into_iter().map(Into::into).collect())
    }
}

pub type NodeAttrs = HashMap<String, NodeSpec>;

#[derive(Debug, Clone)]
pub enum NodeSpecError {
    InvalidTag(String),
    MissingTag,
    InvalidAttribute { key: String, message: String },
    InvalidChild(String),
    UnknownElement(String),
}

impl std::fmt::Display for NodeSpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeSpecError::InvalidTag(msg) => write!(f, "invalid tag: {}", msg),
            NodeSpecError::MissingTag => write!(f, "missing tag in node spec"),
            NodeSpecError::InvalidAttribute { key, message } => {
                write!(f, "invalid attribute '{}': {}", key, message)
            }
            NodeSpecError::InvalidChild(msg) => write!(f, "invalid child: {}", msg),
            NodeSpecError::UnknownElement(tag) => write!(f, "unknown element: {}", tag),
        }
    }
}

impl std::error::Error for NodeSpecError {}

pub type NodeSpecResult<T> = Result<T, NodeSpecError>;

pub fn spec_to_node(value: &NodeSpec) -> NodeSpecResult<Node> {
    match value {
        NodeSpec::Null => Ok(Node::Empty),
        NodeSpec::String(s) => Ok(text(s)),
        NodeSpec::Array(arr) => parse_element(arr),
        _ => Err(NodeSpecError::InvalidChild(format!(
            "expected array, string, or null, got {:?}",
            value
        ))),
    }
}

fn parse_element(arr: &[NodeSpec]) -> NodeSpecResult<Node> {
    if arr.is_empty() {
        return Err(NodeSpecError::MissingTag);
    }

    let tag = arr[0]
        .as_str()
        .ok_or_else(|| NodeSpecError::InvalidTag("tag must be a string".to_string()))?;

    let (attrs, children_start) = if arr.len() > 1 {
        if let Some(obj) = arr[1].as_object() {
            (obj.clone(), 2)
        } else {
            (HashMap::new(), 1)
        }
    } else {
        (HashMap::new(), 1)
    };

    let children: Vec<NodeSpec> = arr[children_start..].to_vec();

    match tag {
        "text" => parse_text(&attrs, &children),
        "col" | "column" => parse_box(Direction::Column, &attrs, &children),
        "row" => parse_box(Direction::Row, &attrs, &children),
        "spacer" => Ok(spacer()),
        "flex" => parse_flex(&attrs, &children),
        "fixed" => parse_fixed(&attrs, &children),
        "spinner" => parse_spinner(&attrs),
        "input" => parse_input(&attrs),
        "popup" => parse_popup(&attrs, &children),
        "fragment" => parse_fragment(&children),
        "focusable" => parse_focusable(&attrs, &children),
        "scrollback" => parse_scrollback(&attrs, &children),
        "error-boundary" => parse_error_boundary(&children),
        "overlay" => parse_overlay(&attrs, &children),
        "divider" => parse_divider(&attrs),
        "hr" => Ok(horizontal_rule()),
        "progress" => parse_progress(&attrs),
        "badge" => parse_badge(&attrs, &children),
        "bullet-list" => parse_bullet_list(&children),
        "numbered-list" => parse_numbered_list(&children),
        "key-value" | "kv" => parse_key_value(&attrs),
        _ => Err(NodeSpecError::UnknownElement(tag.to_string())),
    }
}

fn parse_text(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let content = if !children.is_empty() {
        children
            .iter()
            .filter_map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join("")
    } else {
        attrs
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let style = parse_style(attrs)?;

    if style == Style::default() {
        Ok(text(content))
    } else {
        Ok(styled(content, style))
    }
}

fn parse_box(
    direction: Direction,
    attrs: &NodeAttrs,
    children: &[NodeSpec],
) -> NodeSpecResult<Node> {
    let child_nodes: Vec<Node> = children
        .iter()
        .map(spec_to_node)
        .collect::<NodeSpecResult<Vec<_>>>()?;

    let mut node = BoxNode {
        children: child_nodes,
        direction,
        ..Default::default()
    };

    if let Some(gap) = parse_gap(attrs)? {
        node.gap = gap;
    }
    if let Some(padding) = parse_padding(attrs, "padding")? {
        node.padding = padding;
    }
    if let Some(margin) = parse_padding(attrs, "margin")? {
        node.margin = margin;
    }
    if let Some(border) = parse_border(attrs)? {
        node.border = Some(border);
    }
    if let Some(justify) = parse_justify(attrs)? {
        node.justify = justify;
    }
    if let Some(align) = parse_align(attrs)? {
        node.align = align;
    }
    if let Some(size) = parse_size(attrs)? {
        node.size = size;
    }

    node.style = parse_style(attrs)?;

    Ok(Node::Box(node))
}

fn parse_flex(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let weight = attrs.get("weight").and_then(|v| v.as_u16()).unwrap_or(1);

    if children.len() == 1 {
        let child = spec_to_node(&children[0])?;
        Ok(flex(weight, child))
    } else {
        let child_nodes: Vec<Node> = children
            .iter()
            .map(spec_to_node)
            .collect::<NodeSpecResult<Vec<_>>>()?;
        Ok(flex(weight, fragment(child_nodes)))
    }
}

fn parse_fixed(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let height = attrs
        .get("height")
        .and_then(|v| v.as_u16())
        .ok_or_else(|| NodeSpecError::InvalidAttribute {
            key: "height".to_string(),
            message: "fixed requires a height attribute".to_string(),
        })?;

    if children.len() == 1 {
        let child = spec_to_node(&children[0])?;
        Ok(fixed(height, child))
    } else {
        let child_nodes: Vec<Node> = children
            .iter()
            .map(spec_to_node)
            .collect::<NodeSpecResult<Vec<_>>>()?;
        Ok(fixed(height, fragment(child_nodes)))
    }
}

fn parse_spinner(attrs: &NodeAttrs) -> NodeSpecResult<Node> {
    let label = attrs
        .get("label")
        .and_then(|v| v.as_str())
        .map(String::from);
    let frame = attrs.get("frame").and_then(|v| v.as_usize()).unwrap_or(0);
    Ok(spinner(label, frame))
}

fn parse_input(attrs: &NodeAttrs) -> NodeSpecResult<Node> {
    let value = attrs
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cursor = attrs.get("cursor").and_then(|v| v.as_usize()).unwrap_or(0);
    let placeholder = attrs
        .get("placeholder")
        .and_then(|v| v.as_str())
        .map(String::from);
    let focused = attrs
        .get("focused")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let style = parse_style(attrs)?;

    Ok(Node::Input(InputNode {
        value,
        cursor,
        placeholder,
        style,
        focused,
    }))
}

fn parse_popup(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let selected = attrs
        .get("selected")
        .and_then(|v| v.as_usize())
        .unwrap_or(0);
    let max_visible = attrs
        .get("max_visible")
        .and_then(|v| v.as_usize())
        .unwrap_or(10);

    let items: Vec<PopupItemNode> = children
        .iter()
        .filter_map(|child| {
            if let Some(label) = child.as_str() {
                return Some(popup_item(label));
            }
            if let Some(arr) = child.as_array() {
                if let Some(label) = arr.first().and_then(|v| v.as_str()) {
                    let mut item = popup_item(label);
                    if let Some(attrs) = arr.get(1).and_then(|v| v.as_object()) {
                        if let Some(desc) = attrs.get("desc").and_then(|v| v.as_str()) {
                            item = item.desc(desc);
                        }
                        if let Some(kind) = attrs.get("kind").and_then(|v| v.as_str()) {
                            item = item.kind(kind);
                        }
                    }
                    return Some(item);
                }
            }
            None
        })
        .collect();

    Ok(popup(items, selected, max_visible))
}

fn parse_fragment(children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let child_nodes: Vec<Node> = children
        .iter()
        .map(spec_to_node)
        .collect::<NodeSpecResult<Vec<_>>>()?;
    Ok(fragment(child_nodes))
}

fn parse_focusable(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let id = attrs.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
        NodeSpecError::InvalidAttribute {
            key: "id".to_string(),
            message: "focusable requires an id attribute".to_string(),
        }
    })?;

    let auto_focus = attrs
        .get("auto_focus")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if children.is_empty() {
        return Err(NodeSpecError::InvalidChild(
            "focusable requires a child".to_string(),
        ));
    }

    let child = spec_to_node(&children[0])?;

    Ok(Node::Focusable(FocusableNode {
        id: FocusId::new(id),
        child: Box::new(child),
        auto_focus,
    }))
}

fn parse_scrollback(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let key = attrs.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
        NodeSpecError::InvalidAttribute {
            key: "key".to_string(),
            message: "scrollback requires a key attribute".to_string(),
        }
    })?;

    let newline = attrs
        .get("newline")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let child_nodes: Vec<Node> = children
        .iter()
        .map(spec_to_node)
        .collect::<NodeSpecResult<Vec<_>>>()?;

    Ok(Node::Static(StaticNode {
        key: key.to_string(),
        children: child_nodes,
        newline,
    }))
}

fn parse_error_boundary(children: &[NodeSpec]) -> NodeSpecResult<Node> {
    if children.len() < 2 {
        return Err(NodeSpecError::InvalidChild(
            "error-boundary requires child and fallback".to_string(),
        ));
    }

    let child = spec_to_node(&children[0])?;
    let fallback = spec_to_node(&children[1])?;

    Ok(error_boundary(child, fallback))
}

fn parse_overlay(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let anchor = if let Some(offset) = attrs.get("from_bottom").and_then(|v| v.as_usize()) {
        OverlayAnchor::FromBottom(offset)
    } else {
        OverlayAnchor::FromBottom(0)
    };

    if children.is_empty() {
        return Err(NodeSpecError::InvalidChild(
            "overlay requires a child".to_string(),
        ));
    }

    let child = spec_to_node(&children[0])?;

    Ok(Node::Overlay(OverlayNode {
        child: Box::new(child),
        anchor,
    }))
}

fn parse_divider(attrs: &NodeAttrs) -> NodeSpecResult<Node> {
    let char = attrs
        .get("char")
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next())
        .unwrap_or('─');
    let width = attrs.get("width").and_then(|v| v.as_u16()).unwrap_or(80);
    Ok(divider(char, width))
}

fn parse_progress(attrs: &NodeAttrs) -> NodeSpecResult<Node> {
    let value = attrs.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let width = attrs.get("width").and_then(|v| v.as_u16()).unwrap_or(20);
    Ok(progress_bar(value, width))
}

fn parse_badge(attrs: &NodeAttrs, children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let label = if !children.is_empty() {
        children
            .iter()
            .filter_map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join("")
    } else {
        attrs
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let style = parse_style(attrs)?;
    Ok(badge(label, style))
}

fn parse_bullet_list(children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let items: Vec<String> = children
        .iter()
        .filter_map(|c| c.as_str().map(String::from))
        .collect();
    Ok(bullet_list(items))
}

fn parse_numbered_list(children: &[NodeSpec]) -> NodeSpecResult<Node> {
    let items: Vec<String> = children
        .iter()
        .filter_map(|c| c.as_str().map(String::from))
        .collect();
    Ok(numbered_list(items))
}

fn parse_key_value(attrs: &NodeAttrs) -> NodeSpecResult<Node> {
    let key = attrs.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
        NodeSpecError::InvalidAttribute {
            key: "key".to_string(),
            message: "key-value requires a key attribute".to_string(),
        }
    })?;
    let value = attrs.get("value").and_then(|v| v.as_str()).unwrap_or("");
    Ok(key_value(key, value))
}

fn parse_style(attrs: &NodeAttrs) -> NodeSpecResult<Style> {
    let mut style = Style::default();

    if let Some(fg) = attrs.get("fg").and_then(|v| v.as_str()) {
        style.fg = Some(parse_color(fg)?);
    }
    if let Some(bg) = attrs.get("bg").and_then(|v| v.as_str()) {
        style.bg = Some(parse_color(bg)?);
    }
    if attrs.get("bold").and_then(|v| v.as_bool()).unwrap_or(false) {
        style.bold = true;
    }
    if attrs
        .get("italic")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        style.italic = true;
    }
    if attrs
        .get("underline")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        style.underline = true;
    }
    if attrs.get("dim").and_then(|v| v.as_bool()).unwrap_or(false) {
        style.dim = true;
    }

    Ok(style)
}

pub fn parse_color(s: &str) -> NodeSpecResult<Color> {
    match s.to_lowercase().as_str() {
        "black" => Ok(Color::Black),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "yellow" => Ok(Color::Yellow),
        "blue" => Ok(Color::Blue),
        "magenta" => Ok(Color::Magenta),
        "cyan" => Ok(Color::Cyan),
        "white" => Ok(Color::White),
        "gray" | "grey" => Ok(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Ok(Color::DarkGray),
        "reset" => Ok(Color::Reset),
        _ if s.starts_with('#') => parse_hex_color(s),
        _ if s.starts_with("rgb(") => parse_rgb_color(s),
        _ => Err(NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("unknown color: {}", s),
        }),
    }
}

fn parse_hex_color(s: &str) -> NodeSpecResult<Color> {
    let hex = s.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("invalid hex color: {}", s),
        });
    }

    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| NodeSpecError::InvalidAttribute {
        key: "color".to_string(),
        message: format!("invalid hex color: {}", s),
    })?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| NodeSpecError::InvalidAttribute {
        key: "color".to_string(),
        message: format!("invalid hex color: {}", s),
    })?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| NodeSpecError::InvalidAttribute {
        key: "color".to_string(),
        message: format!("invalid hex color: {}", s),
    })?;

    Ok(Color::Rgb(r, g, b))
}

fn parse_rgb_color(s: &str) -> NodeSpecResult<Color> {
    let inner = s.trim_start_matches("rgb(").trim_end_matches(')').trim();

    let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return Err(NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("invalid rgb color: {}", s),
        });
    }

    let r: u8 = parts[0]
        .parse()
        .map_err(|_| NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("invalid rgb color: {}", s),
        })?;
    let g: u8 = parts[1]
        .parse()
        .map_err(|_| NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("invalid rgb color: {}", s),
        })?;
    let b: u8 = parts[2]
        .parse()
        .map_err(|_| NodeSpecError::InvalidAttribute {
            key: "color".to_string(),
            message: format!("invalid rgb color: {}", s),
        })?;

    Ok(Color::Rgb(r, g, b))
}

fn parse_gap(attrs: &NodeAttrs) -> NodeSpecResult<Option<Gap>> {
    if let Some(val) = attrs.get("gap") {
        if let Some(n) = val.as_u16() {
            return Ok(Some(Gap::all(n)));
        }
        if let Some(obj) = val.as_object() {
            let row = obj.get("row").and_then(|v| v.as_u16()).unwrap_or(0);
            let col = obj.get("column").and_then(|v| v.as_u16()).unwrap_or(0);
            return Ok(Some(Gap::new(row, col)));
        }
    }
    Ok(None)
}

fn parse_padding(attrs: &NodeAttrs, key: &str) -> NodeSpecResult<Option<Padding>> {
    if let Some(val) = attrs.get(key) {
        if let Some(n) = val.as_u16() {
            return Ok(Some(Padding::all(n)));
        }
        if let Some(obj) = val.as_object() {
            return Ok(Some(Padding {
                top: obj.get("top").and_then(|v| v.as_u16()).unwrap_or(0),
                right: obj.get("right").and_then(|v| v.as_u16()).unwrap_or(0),
                bottom: obj.get("bottom").and_then(|v| v.as_u16()).unwrap_or(0),
                left: obj.get("left").and_then(|v| v.as_u16()).unwrap_or(0),
            }));
        }
    }
    Ok(None)
}

fn parse_border(attrs: &NodeAttrs) -> NodeSpecResult<Option<Border>> {
    if let Some(val) = attrs.get("border") {
        if let Some(s) = val.as_str() {
            return match s.to_lowercase().as_str() {
                "single" => Ok(Some(Border::Single)),
                "double" => Ok(Some(Border::Double)),
                "rounded" => Ok(Some(Border::Rounded)),
                "heavy" => Ok(Some(Border::Heavy)),
                _ => Err(NodeSpecError::InvalidAttribute {
                    key: "border".to_string(),
                    message: format!("unknown border style: {}", s),
                }),
            };
        }
        if val.as_bool() == Some(true) {
            return Ok(Some(Border::Single));
        }
    }
    Ok(None)
}

fn parse_justify(attrs: &NodeAttrs) -> NodeSpecResult<Option<JustifyContent>> {
    if let Some(val) = attrs.get("justify") {
        if let Some(s) = val.as_str() {
            return match s.to_lowercase().replace('-', "_").as_str() {
                "start" => Ok(Some(JustifyContent::Start)),
                "end" => Ok(Some(JustifyContent::End)),
                "center" => Ok(Some(JustifyContent::Center)),
                "space_between" => Ok(Some(JustifyContent::SpaceBetween)),
                "space_around" => Ok(Some(JustifyContent::SpaceAround)),
                "space_evenly" => Ok(Some(JustifyContent::SpaceEvenly)),
                _ => Err(NodeSpecError::InvalidAttribute {
                    key: "justify".to_string(),
                    message: format!("unknown justify value: {}", s),
                }),
            };
        }
    }
    Ok(None)
}

fn parse_align(attrs: &NodeAttrs) -> NodeSpecResult<Option<AlignItems>> {
    if let Some(val) = attrs.get("align") {
        if let Some(s) = val.as_str() {
            return match s.to_lowercase().as_str() {
                "start" => Ok(Some(AlignItems::Start)),
                "end" => Ok(Some(AlignItems::End)),
                "center" => Ok(Some(AlignItems::Center)),
                "stretch" => Ok(Some(AlignItems::Stretch)),
                _ => Err(NodeSpecError::InvalidAttribute {
                    key: "align".to_string(),
                    message: format!("unknown align value: {}", s),
                }),
            };
        }
    }
    Ok(None)
}

fn parse_size(attrs: &NodeAttrs) -> NodeSpecResult<Option<Size>> {
    if let Some(val) = attrs.get("size") {
        if let Some(s) = val.as_str() {
            if s == "content" {
                return Ok(Some(Size::Content));
            }
            if let Some(n) = s.strip_prefix("flex(").and_then(|s| s.strip_suffix(')')) {
                if let Ok(weight) = n.parse::<u16>() {
                    return Ok(Some(Size::Flex(weight)));
                }
            }
            if let Some(n) = s.strip_prefix("fixed(").and_then(|s| s.strip_suffix(')')) {
                if let Ok(height) = n.parse::<u16>() {
                    return Ok(Some(Size::Fixed(height)));
                }
            }
        }
        if let Some(obj) = val.as_object() {
            if let Some(flex) = obj.get("flex").and_then(|v| v.as_u16()) {
                return Ok(Some(Size::Flex(flex)));
            }
            if let Some(fixed) = obj.get("fixed").and_then(|v| v.as_u16()) {
                return Ok(Some(Size::Fixed(fixed)));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(items: Vec<NodeSpec>) -> NodeSpec {
        NodeSpec::Array(items)
    }

    fn obj(items: Vec<(&str, NodeSpec)>) -> NodeSpec {
        NodeSpec::Object(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    fn s(text: &str) -> NodeSpec {
        NodeSpec::String(text.to_string())
    }

    fn n(num: i64) -> NodeSpec {
        NodeSpec::Int(num)
    }

    fn b(val: bool) -> NodeSpec {
        NodeSpec::Bool(val)
    }

    #[test]
    fn test_text_node() {
        let hiccup = arr(vec![s("text"), s("Hello")]);
        let node = spec_to_node(&hiccup).unwrap();
        assert!(matches!(node, Node::Text(TextNode { content, .. }) if content == "Hello"));
    }

    #[test]
    fn test_text_with_style() {
        let hiccup = arr(vec![
            s("text"),
            obj(vec![("fg", s("red")), ("bold", b(true))]),
            s("Styled"),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Text(t) = node {
            assert_eq!(t.content, "Styled");
            assert_eq!(t.style.fg, Some(Color::Red));
            assert!(t.style.bold);
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_col_node() {
        let hiccup = arr(vec![
            s("col"),
            obj(vec![("gap", n(1))]),
            arr(vec![s("text"), s("a")]),
            arr(vec![s("text"), s("b")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.children.len(), 2);
            assert_eq!(b.gap, Gap::all(1));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_row_node() {
        let hiccup = arr(vec![
            s("row"),
            arr(vec![s("text"), s("left")]),
            arr(vec![s("spacer")]),
            arr(vec![s("text"), s("right")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.direction, Direction::Row);
            assert_eq!(b.children.len(), 3);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_spacer_node() {
        let hiccup = arr(vec![s("spacer")]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.size, Size::Flex(1));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_spinner_node() {
        let hiccup = arr(vec![s("spinner"), obj(vec![("label", s("Loading..."))])]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Spinner(sp) = node {
            assert_eq!(sp.label, Some("Loading...".to_string()));
        } else {
            panic!("Expected Spinner node");
        }
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("red").unwrap(), Color::Red);
        assert_eq!(parse_color("GREEN").unwrap(), Color::Green);
        assert_eq!(parse_color("darkgray").unwrap(), Color::DarkGray);
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#ff0000").unwrap(), Color::Rgb(255, 0, 0));
        assert_eq!(parse_color("#00ff00").unwrap(), Color::Rgb(0, 255, 0));
        assert_eq!(parse_color("#8080ff").unwrap(), Color::Rgb(128, 128, 255));
    }

    #[test]
    fn test_parse_color_rgb() {
        assert_eq!(
            parse_color("rgb(255, 0, 0)").unwrap(),
            Color::Rgb(255, 0, 0)
        );
        assert_eq!(
            parse_color("rgb(128, 128, 128)").unwrap(),
            Color::Rgb(128, 128, 128)
        );
    }

    #[test]
    fn test_fragment_node() {
        let hiccup = arr(vec![
            s("fragment"),
            arr(vec![s("text"), s("a")]),
            arr(vec![s("text"), s("b")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Fragment(children) = node {
            assert_eq!(children.len(), 2);
        } else {
            panic!("Expected Fragment node");
        }
    }

    #[test]
    fn test_focusable_node() {
        let hiccup = arr(vec![
            s("focusable"),
            obj(vec![("id", s("my-input")), ("auto_focus", b(true))]),
            arr(vec![s("text"), s("content")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Focusable(f) = node {
            assert_eq!(f.id.0, "my-input");
            assert!(f.auto_focus);
        } else {
            panic!("Expected Focusable node");
        }
    }

    #[test]
    fn test_divider_node() {
        let hiccup = arr(vec![
            s("divider"),
            obj(vec![("char", s("-")), ("width", n(40))]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Text(t) = node {
            assert_eq!(t.content.len(), 40);
            assert!(t.content.chars().all(|c| c == '-'));
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_progress_bar() {
        let hiccup = arr(vec![
            s("progress"),
            obj(vec![("value", NodeSpec::Float(0.5)), ("width", n(10))]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn test_border_styles() {
        for (style_str, expected) in [
            ("single", Border::Single),
            ("double", Border::Double),
            ("rounded", Border::Rounded),
            ("heavy", Border::Heavy),
        ] {
            let hiccup = arr(vec![s("col"), obj(vec![("border", s(style_str))])]);
            let node = spec_to_node(&hiccup).unwrap();
            if let Node::Box(b) = node {
                assert_eq!(b.border, Some(expected));
            } else {
                panic!("Expected Box node");
            }
        }
    }

    #[test]
    fn test_justify_content() {
        let hiccup = arr(vec![s("row"), obj(vec![("justify", s("space-between"))])]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.justify, JustifyContent::SpaceBetween);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_align_items() {
        let hiccup = arr(vec![s("col"), obj(vec![("align", s("center"))])]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.align, AlignItems::Center);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_padding() {
        let hiccup = arr(vec![s("col"), obj(vec![("padding", n(2))])]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.padding, Padding::all(2));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_complex_padding() {
        let hiccup = arr(vec![
            s("col"),
            obj(vec![(
                "padding",
                obj(vec![
                    ("top", n(1)),
                    ("right", n(2)),
                    ("bottom", n(3)),
                    ("left", n(4)),
                ]),
            )]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Box(b) = node {
            assert_eq!(b.padding.top, 1);
            assert_eq!(b.padding.right, 2);
            assert_eq!(b.padding.bottom, 3);
            assert_eq!(b.padding.left, 4);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_string_becomes_text() {
        let node = spec_to_node(&s("plain text")).unwrap();
        if let Node::Text(t) = node {
            assert_eq!(t.content, "plain text");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_null_becomes_empty() {
        let node = spec_to_node(&NodeSpec::Null).unwrap();
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn test_unknown_element_error() {
        let hiccup = arr(vec![s("unknown-element")]);
        let result = spec_to_node(&hiccup);
        assert!(matches!(result, Err(NodeSpecError::UnknownElement(_))));
    }

    #[test]
    fn test_missing_tag_error() {
        let hiccup = arr(vec![]);
        let result = spec_to_node(&hiccup);
        assert!(matches!(result, Err(NodeSpecError::MissingTag)));
    }

    #[test]
    fn test_popup_node() {
        let hiccup = arr(vec![
            s("popup"),
            obj(vec![("selected", n(1)), ("max_visible", n(5))]),
            s("Item 1"),
            s("Item 2"),
            s("Item 3"),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Popup(p) = node {
            assert_eq!(p.items.len(), 3);
            assert_eq!(p.selected, 1);
            assert_eq!(p.max_visible, 5);
        } else {
            panic!("Expected Popup node");
        }
    }

    #[test]
    fn test_scrollback_node() {
        let hiccup = arr(vec![
            s("scrollback"),
            obj(vec![("key", s("msg-1")), ("newline", b(false))]),
            arr(vec![s("text"), s("content")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Static(st) = node {
            assert_eq!(st.key, "msg-1");
            assert!(!st.newline);
            assert_eq!(st.children.len(), 1);
        } else {
            panic!("Expected Static node");
        }
    }

    #[test]
    fn test_error_boundary_node() {
        let hiccup = arr(vec![
            s("error-boundary"),
            arr(vec![s("text"), s("child")]),
            arr(vec![s("text"), s("fallback")]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        assert!(matches!(node, Node::ErrorBoundary(_)));
    }

    #[test]
    fn test_input_node() {
        let hiccup = arr(vec![
            s("input"),
            obj(vec![
                ("value", s("hello")),
                ("cursor", n(5)),
                ("placeholder", s("Type here...")),
                ("focused", b(true)),
            ]),
        ]);
        let node = spec_to_node(&hiccup).unwrap();
        if let Node::Input(i) = node {
            assert_eq!(i.value, "hello");
            assert_eq!(i.cursor, 5);
            assert_eq!(i.placeholder, Some("Type here...".to_string()));
            assert!(i.focused);
        } else {
            panic!("Expected Input node");
        }
    }
}
