use crate::focus::FocusId;
use crate::overlay::OverlayAnchor;
use crate::style::{AlignItems, Border, Color, Gap, JustifyContent, Padding, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementKind {
    #[default]
    Block,
    Continuation,
    ToolCall,
}

impl ElementKind {
    pub fn wants_blank_line_before(self, prev: Option<ElementKind>) -> bool {
        match (prev, self) {
            (None, _) => false,
            (_, ElementKind::Continuation) => false,
            (Some(ElementKind::Continuation), _) => false,
            (Some(ElementKind::Block), ElementKind::Block) => true,
            (Some(ElementKind::ToolCall), ElementKind::Block) => true,
            (Some(ElementKind::Block), ElementKind::ToolCall) => false,
            (Some(ElementKind::ToolCall), ElementKind::ToolCall) => false,
        }
    }

    pub fn wants_newline_after(self) -> bool {
        match self {
            ElementKind::Block => true,
            ElementKind::Continuation => false,
            ElementKind::ToolCall => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Node {
    #[default]
    Empty,
    Text(TextNode),
    Box(BoxNode),
    Static(StaticNode),
    Input(InputNode),
    Spinner(SpinnerNode),
    Popup(PopupNode),
    Fragment(Vec<Node>),
    Focusable(FocusableNode),
    ErrorBoundary(ErrorBoundaryNode),
    Overlay(OverlayNode),
    /// Raw escape sequence passthrough (for protocol-specific content like images)
    Raw(RawNode),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FocusableNode {
    pub id: FocusId,
    pub child: Box<Node>,
    pub auto_focus: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorBoundaryNode {
    pub child: Box<Node>,
    pub fallback: Box<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayNode {
    pub child: Box<Node>,
    pub anchor: OverlayAnchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawNode {
    pub content: String,
    pub display_width: u16,
    pub display_height: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextNode {
    pub content: String,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoxNode {
    pub children: Vec<Node>,
    pub direction: Direction,
    pub size: Size,
    pub padding: Padding,
    pub margin: Padding,
    pub border: Option<Border>,
    pub style: Style,
    pub justify: JustifyContent,
    pub align: AlignItems,
    pub gap: Gap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticNode {
    pub key: String,
    pub children: Vec<Node>,
    pub kind: ElementKind,
    pub newline: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputNode {
    pub value: String,
    pub cursor: usize,
    pub placeholder: Option<String>,
    pub style: Style,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpinnerNode {
    pub label: Option<String>,
    pub style: Style,
    pub frame: usize,
    /// Custom spinner frames. If None, uses default SPINNER_FRAMES.
    pub frames: Option<&'static [char]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupNode {
    pub items: Vec<PopupItemNode>,
    pub selected: usize,
    pub viewport_offset: usize,
    pub max_visible: usize,
    pub bg_style: Style,
    pub selected_style: Style,
    pub unselected_style: Style,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupItemNode {
    pub label: String,
    pub description: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Column,
    Row,
}

/// Sizing behavior for layout nodes.
///
/// Controls how a node's width/height is determined during layout:
///
/// - `Fixed(n)`: Exact size in characters/lines. Does not grow or shrink.
/// - `Flex(weight)`: Proportional share of remaining space after fixed/content sizes.
///   Higher weight = larger share. Use for "fill remaining space" behavior.
/// - `Content`: Shrink-to-fit. Measures actual content and uses exactly that size.
///   Does not expand to fill available space.
///
/// # Examples
///
/// ```ignore
/// // Two-column layout: narrow label, wide content
/// row([
///     col([text("Label:")]),        // Content: shrinks to "Label:" width
///     flex(1, col([text("...")])),  // Flex: fills remaining space
/// ])
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    Fixed(u16),
    Flex(u16),
    #[default]
    Content,
}

pub const SPINNER_FRAMES: &[char] = &['◐', '◓', '◑', '◒'];
pub const BRAILLE_SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn text(content: impl Into<String>) -> Node {
    Node::Text(TextNode {
        content: content.into(),
        style: Style::default(),
    })
}

pub fn styled(content: impl Into<String>, style: Style) -> Node {
    Node::Text(TextNode {
        content: content.into(),
        style,
    })
}

pub fn col(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Box(BoxNode {
        children: children.into_iter().collect(),
        direction: Direction::Column,
        ..Default::default()
    })
}

pub fn row(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Box(BoxNode {
        children: children.into_iter().collect(),
        direction: Direction::Row,
        ..Default::default()
    })
}

pub fn scrollback(key: impl Into<String>, children: impl IntoIterator<Item = Node>) -> Node {
    scrollback_with_kind(key, ElementKind::Block, children)
}

pub fn scrollback_continuation(
    key: impl Into<String>,
    children: impl IntoIterator<Item = Node>,
) -> Node {
    scrollback_with_kind(key, ElementKind::Continuation, children)
}

pub fn scrollback_tool(key: impl Into<String>, children: impl IntoIterator<Item = Node>) -> Node {
    scrollback_with_kind(key, ElementKind::ToolCall, children)
}

pub fn scrollback_with_kind(
    key: impl Into<String>,
    kind: ElementKind,
    children: impl IntoIterator<Item = Node>,
) -> Node {
    Node::Static(StaticNode {
        key: key.into(),
        children: children.into_iter().collect(),
        kind,
        newline: kind.wants_newline_after(),
    })
}

pub fn text_input(value: impl Into<String>, cursor: usize) -> Node {
    Node::Input(InputNode {
        value: value.into(),
        cursor,
        placeholder: None,
        style: Style::default(),
        focused: true,
    })
}

pub fn spinner(label: Option<String>, frame: usize) -> Node {
    Node::Spinner(SpinnerNode {
        label,
        style: Style::default(),
        frame,
        frames: None,
    })
}

pub fn spinner_styled(frame: usize, style: Style) -> Node {
    Node::Spinner(SpinnerNode {
        label: None,
        style,
        frame,
        frames: None,
    })
}

pub fn spinner_with_frames(frame: usize, style: Style, frames: &'static [char]) -> Node {
    Node::Spinner(SpinnerNode {
        label: None,
        style,
        frame,
        frames: Some(frames),
    })
}

pub fn fragment(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Fragment(children.into_iter().collect())
}

pub fn popup(items: Vec<PopupItemNode>, selected: usize, max_visible: usize) -> Node {
    // Default styles with original hardcoded colors
    let bg_style = Style::new().bg(Color::Rgb(45, 50, 60));
    let selected_style = Style::new().bg(Color::Rgb(60, 70, 90));
    let unselected_style = Style::new().bg(Color::Rgb(45, 50, 60));
    popup_styled(
        items,
        selected,
        max_visible,
        bg_style,
        selected_style,
        unselected_style,
    )
}

pub fn popup_styled(
    items: Vec<PopupItemNode>,
    selected: usize,
    max_visible: usize,
    bg_style: Style,
    selected_style: Style,
    unselected_style: Style,
) -> Node {
    let viewport_offset = if selected >= max_visible {
        selected.saturating_sub(max_visible - 1)
    } else {
        0
    };
    Node::Popup(PopupNode {
        items,
        selected,
        viewport_offset,
        max_visible,
        bg_style,
        selected_style,
        unselected_style,
    })
}

pub fn popup_item(label: impl Into<String>) -> PopupItemNode {
    PopupItemNode {
        label: label.into(),
        description: None,
        kind: None,
    }
}

impl PopupItemNode {
    pub fn desc(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}

pub fn focusable(id: impl Into<String>, child: Node) -> Node {
    Node::Focusable(FocusableNode {
        id: FocusId::new(id),
        child: Box::new(child),
        auto_focus: false,
    })
}

pub fn focusable_auto(id: impl Into<String>, child: Node) -> Node {
    Node::Focusable(FocusableNode {
        id: FocusId::new(id),
        child: Box::new(child),
        auto_focus: true,
    })
}

impl FocusableNode {
    pub fn auto_focus(mut self) -> Self {
        self.auto_focus = true;
        self
    }
}

pub fn error_boundary(child: Node, fallback: Node) -> Node {
    Node::ErrorBoundary(ErrorBoundaryNode {
        child: Box::new(child),
        fallback: Box::new(fallback),
    })
}

pub fn overlay_from_bottom(child: Node, offset: usize) -> Node {
    Node::Overlay(OverlayNode {
        child: Box::new(child),
        anchor: OverlayAnchor::FromBottom(offset),
    })
}

pub fn overlay_from_bottom_right(child: Node, offset: usize) -> Node {
    Node::Overlay(OverlayNode {
        child: Box::new(child),
        anchor: OverlayAnchor::FromBottomRight(offset),
    })
}

pub fn raw(content: impl Into<String>, display_width: u16, display_height: u16) -> Node {
    Node::Raw(RawNode {
        content: content.into(),
        display_width,
        display_height,
    })
}

pub fn spacer() -> Node {
    Node::Box(BoxNode {
        size: Size::Flex(1),
        ..Default::default()
    })
}

pub fn flex(weight: u16, child: Node) -> Node {
    Node::Box(BoxNode {
        children: vec![child],
        size: Size::Flex(weight),
        ..Default::default()
    })
}

pub fn fixed(height: u16, child: Node) -> Node {
    Node::Box(BoxNode {
        children: vec![child],
        size: Size::Fixed(height),
        ..Default::default()
    })
}

pub fn when(condition: bool, node: Node) -> Node {
    if condition {
        node
    } else {
        Node::Empty
    }
}

pub fn if_else(condition: bool, then_node: Node, else_node: Node) -> Node {
    if condition {
        then_node
    } else {
        else_node
    }
}

pub fn maybe<T>(value: Option<T>, f: impl FnOnce(T) -> Node) -> Node {
    match value {
        Some(v) => f(v),
        None => Node::Empty,
    }
}

pub fn progress_bar(progress: f32, width: u16) -> Node {
    let progress = progress.clamp(0.0, 1.0);
    let filled = (progress * width as f32).round() as usize;
    let empty = (width as usize).saturating_sub(filled);

    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    text(bar)
}

pub fn progress_bar_styled(
    progress: f32,
    width: u16,
    filled_style: Style,
    empty_style: Style,
) -> Node {
    let progress = progress.clamp(0.0, 1.0);
    let filled = (progress * width as f32).round() as usize;
    let empty = (width as usize).saturating_sub(filled);

    row([
        styled("█".repeat(filled), filled_style),
        styled("░".repeat(empty), empty_style),
    ])
}

pub fn divider(char: char, width: u16) -> Node {
    text(char.to_string().repeat(width as usize))
}

pub fn horizontal_rule() -> Node {
    text("─".repeat(80))
}

pub fn badge(label: impl Into<String>, style: Style) -> Node {
    styled(format!(" {} ", label.into()), style)
}

pub fn key_value(key: impl Into<String>, value: impl Into<String>) -> Node {
    row([
        styled(format!("{}: ", key.into()), Style::new().bold()),
        text(value),
    ])
}

pub fn bullet_list(items: impl IntoIterator<Item = impl Into<String>>) -> Node {
    col(items
        .into_iter()
        .map(|item| row([styled("• ", Style::new().dim()), text(item)])))
}

pub fn numbered_list(items: impl IntoIterator<Item = impl Into<String>>) -> Node {
    col(items.into_iter().enumerate().map(|(i, item)| {
        row([
            styled(format!("{}. ", i + 1), Style::new().dim()),
            text(item),
        ])
    }))
}

impl Node {
    pub fn with_style(self, style: Style) -> Self {
        match self {
            Node::Text(mut t) => {
                t.style = style;
                Node::Text(t)
            }
            Node::Spinner(mut s) => {
                s.style = style;
                Node::Spinner(s)
            }
            Node::Input(mut i) => {
                i.style = style;
                Node::Input(i)
            }
            Node::Box(mut b) => {
                b.style = style;
                Node::Box(b)
            }
            other => other,
        }
    }

    pub fn with_padding(self, padding: Padding) -> Self {
        match self {
            Node::Box(mut b) => {
                b.padding = padding;
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                padding,
                ..Default::default()
            }),
        }
    }

    pub fn with_border(self, border: Border) -> Self {
        match self {
            Node::Box(mut b) => {
                b.border = Some(border);
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                border: Some(border),
                ..Default::default()
            }),
        }
    }

    pub fn with_margin(self, margin: Padding) -> Self {
        match self {
            Node::Box(mut b) => {
                b.margin = margin;
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                margin,
                ..Default::default()
            }),
        }
    }

    pub fn justify(self, justify: JustifyContent) -> Self {
        match self {
            Node::Box(mut b) => {
                b.justify = justify;
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                justify,
                ..Default::default()
            }),
        }
    }

    pub fn align(self, align: AlignItems) -> Self {
        match self {
            Node::Box(mut b) => {
                b.align = align;
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                align,
                ..Default::default()
            }),
        }
    }

    pub fn gap(self, gap: Gap) -> Self {
        match self {
            Node::Box(mut b) => {
                b.gap = gap;
                Node::Box(b)
            }
            other => Node::Box(BoxNode {
                children: vec![other],
                gap,
                ..Default::default()
            }),
        }
    }
}

impl TextNode {
    pub fn fg(mut self, color: Color) -> Self {
        self.style = self.style.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = self.style.bg(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.style = self.style.bold();
        self
    }

    pub fn dim(mut self) -> Self {
        self.style = self.style.dim();
        self
    }
}

impl InputNode {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl SpinnerNode {
    pub fn current_char(&self) -> char {
        let frames = self.frames.unwrap_or(SPINNER_FRAMES);
        debug_assert!(!frames.is_empty(), "spinner frames must not be empty");
        frames[self.frame % frames.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_creates_text_node() {
        let node = text("hello");
        assert!(matches!(node, Node::Text(TextNode { content, .. }) if content == "hello"));
    }

    #[test]
    fn test_col_creates_column_box() {
        let node = col([text("a"), text("b")]);
        match node {
            Node::Box(b) => {
                assert_eq!(b.direction, Direction::Column);
                assert_eq!(b.children.len(), 2);
            }
            _ => panic!("Expected BoxNode"),
        }
    }

    #[test]
    fn test_row_creates_row_box() {
        let node = row([text("a"), text("b")]);
        match node {
            Node::Box(b) => {
                assert_eq!(b.direction, Direction::Row);
                assert_eq!(b.children.len(), 2);
            }
            _ => panic!("Expected BoxNode"),
        }
    }

    #[test]
    fn test_when_returns_node_on_true() {
        let node = when(true, text("visible"));
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn test_when_returns_empty_on_false() {
        let node = when(false, text("hidden"));
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn test_if_else_returns_correct_branch() {
        let node_true = if_else(true, text("yes"), text("no"));
        let node_false = if_else(false, text("yes"), text("no"));

        match node_true {
            Node::Text(t) => assert_eq!(t.content, "yes"),
            _ => panic!("Expected Text"),
        }
        match node_false {
            Node::Text(t) => assert_eq!(t.content, "no"),
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_maybe_returns_node_on_some() {
        let node = maybe(Some("value"), text);
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn test_maybe_returns_empty_on_none() {
        let node: Node = maybe(None::<&str>, text);
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn test_progress_bar_clamps_values() {
        let bar_neg = progress_bar(-0.5, 10);
        let bar_over = progress_bar(1.5, 10);

        assert!(matches!(bar_neg, Node::Text(_)));
        assert!(matches!(bar_over, Node::Text(_)));
    }

    #[test]
    fn test_progress_bar_at_boundaries() {
        let bar_zero = progress_bar(0.0, 10);
        let bar_full = progress_bar(1.0, 10);

        if let Node::Text(t) = bar_zero {
            assert!(t.content.chars().all(|c| c == '░'));
        }
        if let Node::Text(t) = bar_full {
            assert!(t.content.chars().all(|c| c == '█'));
        }
    }

    #[test]
    fn test_spinner_current_char_cycles() {
        let spinner = SpinnerNode {
            label: None,
            style: Style::default(),
            frame: 0,
            frames: None,
        };
        assert_eq!(spinner.current_char(), SPINNER_FRAMES[0]);

        let spinner_4 = SpinnerNode {
            label: None,
            style: Style::default(),
            frame: 4,
            frames: None,
        };
        assert_eq!(spinner_4.current_char(), SPINNER_FRAMES[0]);
    }

    #[test]
    fn test_popup_viewport_offset() {
        let items = vec![
            popup_item("a"),
            popup_item("b"),
            popup_item("c"),
            popup_item("d"),
            popup_item("e"),
        ];

        let node = popup(items.clone(), 0, 3);
        if let Node::Popup(p) = node {
            assert_eq!(p.viewport_offset, 0);
        }

        let node_scroll = popup(items, 4, 3);
        if let Node::Popup(p) = node_scroll {
            assert_eq!(p.viewport_offset, 2);
        }
    }

    #[test]
    fn test_popup_item_builder() {
        let item = popup_item("Label").desc("Description").kind("file");
        assert_eq!(item.label, "Label");
        assert_eq!(item.description, Some("Description".to_string()));
        assert_eq!(item.kind, Some("file".to_string()));
    }

    #[test]
    fn test_node_with_style() {
        let node = text("hello").with_style(Style::new().bold());
        if let Node::Text(t) = node {
            assert!(t.style.bold);
        } else {
            panic!("Expected Text");
        }
    }

    #[test]
    fn test_node_with_border_wraps_non_box() {
        let node = text("hello").with_border(Border::Single);
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn test_spacer_has_flex_size() {
        let node = spacer();
        if let Node::Box(b) = node {
            assert_eq!(b.size, Size::Flex(1));
        } else {
            panic!("Expected BoxNode");
        }
    }

    #[test]
    fn test_divider_creates_repeated_char() {
        let node = divider('-', 5);
        if let Node::Text(t) = node {
            assert_eq!(t.content, "-----");
        } else {
            panic!("Expected Text");
        }
    }
}

#[cfg(test)]
mod raw_node_tests {
    use super::*;
    use crate::render::{render_to_plain_text, render_to_string};

    #[test]
    fn raw_node_renders_content_as_is() {
        let node = raw("\x1b]1337;File=inline=1:abc\x07", 10, 5);
        let output = render_to_string(&node, 80);
        assert!(output.contains("\x1b]1337;File=inline=1:abc\x07"));
    }

    #[test]
    fn raw_node_plain_text_shows_placeholder() {
        let node = raw("\x1b_Gi=1;abc\x1b\\", 10, 5);
        let output = render_to_plain_text(&node, 80);
        assert!(output.contains("[raw:"));
        assert!(output.contains("10x5"));
    }

    #[test]
    fn raw_node_pads_to_width() {
        let node = raw("\x1b_G\x1b\\", 3, 1);
        let output = render_to_string(&node, 10);
        // Raw content + 7 spaces of padding (10 - 3)
        assert!(output.starts_with("\x1b_G\x1b\\"));
        assert_eq!(output.len(), "\x1b_G\x1b\\".len() + 7);
    }

    #[test]
    fn raw_builder_creates_raw_node() {
        let node = raw("test", 5, 3);
        match node {
            Node::Raw(r) => {
                assert_eq!(r.content, "test");
                assert_eq!(r.display_width, 5);
                assert_eq!(r.display_height, 3);
            }
            _ => panic!("Expected Raw node"),
        }
    }
}

#[cfg(test)]
mod element_kind_tests {
    use super::ElementKind;

    #[test]
    fn block_to_block_wants_blank_line() {
        assert!(ElementKind::Block.wants_blank_line_before(Some(ElementKind::Block)));
    }

    #[test]
    fn block_after_tool_wants_blank_line() {
        assert!(ElementKind::Block.wants_blank_line_before(Some(ElementKind::ToolCall)));
        assert!(!ElementKind::ToolCall.wants_blank_line_before(Some(ElementKind::Block)));
    }

    #[test]
    fn continuation_never_wants_blank_line() {
        assert!(!ElementKind::Continuation.wants_blank_line_before(Some(ElementKind::Block)));
        assert!(!ElementKind::Continuation.wants_blank_line_before(Some(ElementKind::ToolCall)));
        assert!(!ElementKind::Continuation.wants_blank_line_before(Some(ElementKind::Continuation)));
    }

    #[test]
    fn first_element_never_wants_blank_line() {
        assert!(!ElementKind::Block.wants_blank_line_before(None));
        assert!(!ElementKind::ToolCall.wants_blank_line_before(None));
        assert!(!ElementKind::Continuation.wants_blank_line_before(None));
    }

    #[test]
    fn tool_calls_are_compact() {
        assert!(!ElementKind::ToolCall.wants_blank_line_before(Some(ElementKind::Block)));
        assert!(!ElementKind::ToolCall.wants_blank_line_before(Some(ElementKind::ToolCall)));
    }

    #[test]
    fn wants_newline_after_matches_kind() {
        assert!(ElementKind::Block.wants_newline_after());
        assert!(ElementKind::ToolCall.wants_newline_after());
        assert!(!ElementKind::Continuation.wants_newline_after());
    }
}
