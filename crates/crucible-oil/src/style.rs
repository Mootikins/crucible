use crossterm::style::{Attribute, Color as CtColor, ContentStyle, Stylize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reverse: bool,
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

    pub fn reverse(mut self) -> Self {
        self.reverse = true;
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
        if self.reverse {
            style = style.attribute(Attribute::Reverse);
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
        if self.reverse {
            codes.push(7);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveColor {
    pub dark: Color,
    pub light: Color,
}

impl AdaptiveColor {
    /// Create an AdaptiveColor with the same color for both dark and light modes.
    pub fn from_single(color: Color) -> Self {
        Self {
            dark: color,
            light: color,
        }
    }

    /// Resolve the color based on terminal background detection.
    /// Returns Color::Reset if NO_COLOR environment variable is set.
    pub fn resolve(self, is_dark: bool) -> Color {
        if std::env::var("NO_COLOR").is_ok() {
            return Color::Reset;
        }
        if is_dark {
            self.dark
        } else {
            self.light
        }
    }
}

/// Detect if the terminal has a dark background.
/// Parses COLORFGBG environment variable (format: "fg;bg").
/// If bg < 8, terminal is dark; if bg >= 8, terminal is light.
/// Defaults to dark (true) if COLORFGBG is not set or cannot be parsed.
pub fn detect_dark_terminal() -> bool {
    match std::env::var("COLORFGBG") {
        Ok(value) => {
            // Format is "foreground;background"
            if let Some(bg_str) = value.split(';').nth(1) {
                if let Ok(bg) = bg_str.parse::<u8>() {
                    // bg < 8 means dark terminal, bg >= 8 means light terminal
                    return bg < 8;
                }
            }
            // Default to dark if parsing fails
            true
        }
        Err(_) => {
            // Default to dark if COLORFGBG is not set
            true
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

/// Spacing between children in a layout.
///
/// # Platform Support
///
/// - `row`: Vertical spacing (blank lines) between children in column layouts.
///   Works in both legacy render path and Taffy layout.
/// - `column`: Horizontal spacing (character columns) between children in row layouts.
///   **Only supported in Taffy layout path.** Legacy render ignores this field.
///
/// If you need horizontal row gap, use Taffy-based rendering.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_builder_chain() {
        let style = Style::new()
            .fg(Color::Red)
            .bg(Color::Blue)
            .bold()
            .italic()
            .underline()
            .dim();

        assert_eq!(style.fg, Some(Color::Red));
        assert_eq!(style.bg, Some(Color::Blue));
        assert!(style.bold);
        assert!(style.italic);
        assert!(style.underline);
        assert!(style.dim);
    }

    #[test]
    fn test_style_default() {
        let style = Style::default();
        assert!(style.fg.is_none());
        assert!(style.bg.is_none());
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.underline);
        assert!(!style.dim);
        assert!(!style.reverse);
    }

    #[test]
    fn test_style_to_ansi_codes_empty() {
        let style = Style::new();
        assert_eq!(style.to_ansi_codes(), "");
    }

    #[test]
    fn test_style_to_ansi_codes_bold() {
        let style = Style::new().bold();
        assert!(style.to_ansi_codes().contains("1"));
    }

    #[test]
    fn test_color_to_ansi_fg() {
        assert_eq!(Color::Red.to_ansi_fg(), vec![31]);
        assert_eq!(Color::Green.to_ansi_fg(), vec![32]);
        assert_eq!(Color::Blue.to_ansi_fg(), vec![34]);
    }

    #[test]
    fn test_color_to_ansi_bg() {
        assert_eq!(Color::Red.to_ansi_bg(), vec![41]);
        assert_eq!(Color::Green.to_ansi_bg(), vec![42]);
        assert_eq!(Color::Blue.to_ansi_bg(), vec![44]);
    }

    #[test]
    fn test_color_rgb_ansi() {
        assert_eq!(
            Color::Rgb(255, 128, 64).to_ansi_fg(),
            vec![38, 2, 255, 128, 64]
        );
        assert_eq!(
            Color::Rgb(255, 128, 64).to_ansi_bg(),
            vec![48, 2, 255, 128, 64]
        );
    }

    #[test]
    fn test_padding_all() {
        let p = Padding::all(5);
        assert_eq!(p.top, 5);
        assert_eq!(p.right, 5);
        assert_eq!(p.bottom, 5);
        assert_eq!(p.left, 5);
    }

    #[test]
    fn test_padding_xy() {
        let p = Padding::xy(10, 5);
        assert_eq!(p.top, 5);
        assert_eq!(p.right, 10);
        assert_eq!(p.bottom, 5);
        assert_eq!(p.left, 10);
    }

    #[test]
    fn test_padding_horizontal_vertical() {
        let p = Padding {
            top: 1,
            right: 2,
            bottom: 3,
            left: 4,
        };
        assert_eq!(p.horizontal(), 6);
        assert_eq!(p.vertical(), 4);
    }

    #[test]
    fn test_border_chars_single() {
        let chars = Border::Single.chars();
        assert_eq!(chars.top_left, '┌');
        assert_eq!(chars.horizontal, '─');
    }

    #[test]
    fn test_border_chars_rounded() {
        let chars = Border::Rounded.chars();
        assert_eq!(chars.top_left, '╭');
        assert_eq!(chars.top_right, '╮');
    }

    #[test]
    fn test_gap_constructors() {
        let g1 = Gap::all(5);
        assert_eq!(g1.row, 5);
        assert_eq!(g1.column, 5);

        let g2 = Gap::row(3);
        assert_eq!(g2.row, 3);
        assert_eq!(g2.column, 0);

        let g3 = Gap::column(4);
        assert_eq!(g3.row, 0);
        assert_eq!(g3.column, 4);

        let g4 = Gap::new(1, 2);
        assert_eq!(g4.row, 1);
        assert_eq!(g4.column, 2);
    }

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn new(key: &'static str, value: &str) -> Self {
            let old = std::env::var(key).ok();
            std::env::set_var(key, value);
            EnvGuard { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(old) = self.old.clone() {
                std::env::set_var(self.key, old);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn test_adaptive_color_from_single() {
        let ac = AdaptiveColor::from_single(Color::Red);
        assert_eq!(ac.dark, Color::Red);
        assert_eq!(ac.light, Color::Red);
    }

    #[test]
    fn test_adaptive_color_resolve_dark() {
        let ac = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };
        assert_eq!(ac.resolve(true), Color::Red);
    }

    #[test]
    fn test_adaptive_color_resolve_light() {
        let ac = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };
        assert_eq!(ac.resolve(false), Color::Blue);
    }

    #[test]
    fn test_adaptive_color_no_color_env() {
        let ac = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };
        // Use guard to ensure cleanup even if test panics
        let _guard = EnvGuard::new("NO_COLOR", "1");
        assert_eq!(ac.resolve(true), Color::Reset);
        assert_eq!(ac.resolve(false), Color::Reset);
    }

    #[test]
    fn test_detect_dark_terminal_dark_bg() {
        // bg < 8 means dark terminal
        let _guard = EnvGuard::new("COLORFGBG", "15;0");
        assert!(detect_dark_terminal());
    }

    #[test]
    fn test_detect_dark_terminal_light_bg() {
        // bg >= 8 means light terminal
        let _guard = EnvGuard::new("COLORFGBG", "0;15");
        assert!(!detect_dark_terminal());
    }

    #[test]
    fn test_detect_dark_terminal_not_set() {
        // Default to dark if not set
        std::env::remove_var("COLORFGBG");
        assert!(detect_dark_terminal());
    }

    #[test]
    fn test_detect_dark_terminal_invalid_format() {
        // Default to dark if format is invalid
        let _guard = EnvGuard::new("COLORFGBG", "invalid");
        assert!(detect_dark_terminal());
    }

    #[test]
    fn test_detect_dark_terminal_boundary() {
        // Test boundary: bg = 7 (dark), bg = 8 (light)
        let _guard = EnvGuard::new("COLORFGBG", "15;7");
        assert!(detect_dark_terminal());
        // Now test bg = 8 (light) - drop guard and create new one
        drop(_guard);
        let _guard2 = EnvGuard::new("COLORFGBG", "15;8");
        assert!(!detect_dark_terminal());
    }
}
