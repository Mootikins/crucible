use crate::tui::ink::style::{Border, Color, Padding, Style};

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Empty,
    Text(TextNode),
    Box(BoxNode),
    Static(StaticNode),
    Input(InputNode),
    Spinner(SpinnerNode),
    Popup(PopupNode),
    Fragment(Vec<Node>),
}

impl Default for Node {
    fn default() -> Self {
        Node::Empty
    }
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
    pub border: Option<Border>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticNode {
    pub key: String,
    pub children: Vec<Node>,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupNode {
    pub items: Vec<PopupItemNode>,
    pub selected: usize,
    pub viewport_offset: usize,
    pub max_visible: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    Fixed(u16),
    Flex(u16),
    #[default]
    Content,
}

pub const SPINNER_FRAMES: &[char] = &['◐', '◓', '◑', '◒'];

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
    Node::Static(StaticNode {
        key: key.into(),
        children: children.into_iter().collect(),
        newline: true,
    })
}

pub fn scrollback_continuation(
    key: impl Into<String>,
    children: impl IntoIterator<Item = Node>,
) -> Node {
    Node::Static(StaticNode {
        key: key.into(),
        children: children.into_iter().collect(),
        newline: false,
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
    })
}

pub fn fragment(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Fragment(children.into_iter().collect())
}

pub fn popup(items: Vec<PopupItemNode>, selected: usize, max_visible: usize) -> Node {
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
        SPINNER_FRAMES[self.frame % SPINNER_FRAMES.len()]
    }
}
