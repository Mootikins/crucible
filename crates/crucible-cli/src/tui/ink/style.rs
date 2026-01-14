use crossterm::style::{Attribute, Color as CtColor, ContentStyle, Stylize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn to_crossterm(&self) -> ContentStyle {
        let mut style = ContentStyle::new();

        if let Some(fg) = self.fg {
            style = style.with(fg.to_crossterm());
        }
        if let Some(bg) = self.bg {
            style = style.on(bg.to_crossterm());
        }
        if self.bold {
            style = style.attribute(Attribute::Bold);
        }
        if self.italic {
            style = style.attribute(Attribute::Italic);
        }
        if self.underline {
            style = style.attribute(Attribute::Underlined);
        }
        if self.dim {
            style = style.attribute(Attribute::Dim);
        }

        style
    }

    pub fn to_ansi_codes(&self) -> String {
        let mut codes: Vec<u8> = Vec::new();

        if self.bold {
            codes.push(1);
        }
        if self.dim {
            codes.push(2);
        }
        if self.italic {
            codes.push(3);
        }
        if self.underline {
            codes.push(4);
        }
        if let Some(fg) = self.fg {
            codes.extend(fg.to_ansi_fg());
        }
        if let Some(bg) = self.bg {
            codes.extend(bg.to_ansi_bg());
        }

        if codes.is_empty() {
            String::new()
        } else {
            format!(
                "\x1b[{}m",
                codes
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(";")
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    DarkGray,
    Rgb(u8, u8, u8),
    Reset,
}

impl Color {
    pub fn to_crossterm(self) -> CtColor {
        match self {
            Color::Black => CtColor::Black,
            Color::Red => CtColor::Red,
            Color::Green => CtColor::Green,
            Color::Yellow => CtColor::Yellow,
            Color::Blue => CtColor::Blue,
            Color::Magenta => CtColor::Magenta,
            Color::Cyan => CtColor::Cyan,
            Color::White => CtColor::White,
            Color::Gray => CtColor::Grey,
            Color::DarkGray => CtColor::DarkGrey,
            Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
            Color::Reset => CtColor::Reset,
        }
    }

    pub fn to_ansi_fg(self) -> Vec<u8> {
        match self {
            Color::Black => vec![30],
            Color::Red => vec![31],
            Color::Green => vec![32],
            Color::Yellow => vec![33],
            Color::Blue => vec![34],
            Color::Magenta => vec![35],
            Color::Cyan => vec![36],
            Color::White => vec![37],
            Color::Gray => vec![38, 5, 250],
            Color::DarkGray => vec![38, 5, 240],
            Color::Rgb(r, g, b) => vec![38, 2, r, g, b],
            Color::Reset => vec![39],
        }
    }

    pub fn to_ansi_bg(self) -> Vec<u8> {
        match self {
            Color::Black => vec![40],
            Color::Red => vec![41],
            Color::Green => vec![42],
            Color::Yellow => vec![43],
            Color::Blue => vec![44],
            Color::Magenta => vec![45],
            Color::Cyan => vec![46],
            Color::White => vec![47],
            Color::Gray => vec![48, 5, 250],
            Color::DarkGray => vec![48, 5, 240],
            Color::Rgb(r, g, b) => vec![48, 2, r, g, b],
            Color::Reset => vec![49],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub fn all(n: u16) -> Self {
        Self {
            top: n,
            right: n,
            bottom: n,
            left: n,
        }
    }

    pub fn xy(x: u16, y: u16) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub fn horizontal(&self) -> u16 {
        self.left + self.right
    }

    pub fn vertical(&self) -> u16 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Single,
    Double,
    Rounded,
    Heavy,
}

impl Border {
    pub fn chars(&self) -> BorderChars {
        match self {
            Border::Single => BorderChars {
                top_left: '┌',
                top_right: '┐',
                bottom_left: '└',
                bottom_right: '┘',
                horizontal: '─',
                vertical: '│',
            },
            Border::Double => BorderChars {
                top_left: '╔',
                top_right: '╗',
                bottom_left: '╚',
                bottom_right: '╝',
                horizontal: '═',
                vertical: '║',
            },
            Border::Rounded => BorderChars {
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                horizontal: '─',
                vertical: '│',
            },
            Border::Heavy => BorderChars {
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
                horizontal: '━',
                vertical: '┃',
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BorderChars {
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub horizontal: char,
    pub vertical: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Start,
    End,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gap {
    pub row: u16,
    pub column: u16,
}

impl Gap {
    pub fn all(n: u16) -> Self {
        Self { row: n, column: n }
    }

    pub fn row(n: u16) -> Self {
        Self { row: n, column: 0 }
    }

    pub fn column(n: u16) -> Self {
        Self { row: 0, column: n }
    }

    pub fn new(row: u16, column: u16) -> Self {
        Self { row, column }
    }
}
