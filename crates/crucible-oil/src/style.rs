use crossterm::style::{Attribute, Color as CtColor, ContentStyle, Stylize};

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub fg: Option<Color>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub bg: Option<Color>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub bold: bool,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub italic: bool,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub underline: bool,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub dim: bool,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
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

#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
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
    /// Terminal palette 7. Named `Gray` for history; `White` is the same slot.
    Gray,
    /// Terminal palette 8 — "bright black" in most palettes.
    DarkGray,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// A raw palette index. 0-15 are the terminal's own colours, so a theme
    /// written against them follows whatever the user configured; 16-255 are
    /// the fixed xterm cube and greyscale ramp.
    Indexed(u8),
    Rgb(u8, u8, u8),
    Reset,
}

impl Color {
    /// Derive a selection-highlight surface from an arbitrary base color:
    /// step toward white on dark surfaces and toward black on light ones, so
    /// a selected row reads as "the same surface, raised" for ANY themed bg
    /// (e.g. a popup matching the command prompt's user-configured color).
    /// Non-RGB colors have no channel math to do; fall back to the default
    /// selection bg.
    pub fn selection_variant(self) -> Color {
        const STEP: u8 = 14;
        match self {
            Color::Rgb(r, g, b) => {
                let luminance =
                    (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
                if luminance < 128 {
                    Color::Rgb(
                        r.saturating_add(STEP),
                        g.saturating_add(STEP),
                        b.saturating_add(STEP),
                    )
                } else {
                    Color::Rgb(
                        r.saturating_sub(STEP),
                        g.saturating_sub(STEP),
                        b.saturating_sub(STEP),
                    )
                }
            }
            _ => crate::node::DEFAULT_POPUP_SELECTED_BG,
        }
    }

    /// Crossterm equivalent, matched by **palette slot, not by variant name**.
    ///
    /// crossterm's names are shifted from these: its `Color::Red` is the
    /// bright slot (9) and its `DarkRed` is the normal one (1). Mapping
    /// name-to-name — which is what this did — sent every chromatic colour to
    /// the bright slot and every `bright_*` colour to the normal one, so a
    /// theme asking for red got bright red on every frame. `palette_index` is
    /// the authority; see `named_colors_render_to_the_palette_slot_they_name`.
    pub fn to_crossterm(self) -> CtColor {
        match self {
            Color::Black => CtColor::Black,
            Color::Red => CtColor::DarkRed,
            Color::Green => CtColor::DarkGreen,
            Color::Yellow => CtColor::DarkYellow,
            Color::Blue => CtColor::DarkBlue,
            Color::Magenta => CtColor::DarkMagenta,
            Color::Cyan => CtColor::DarkCyan,
            // Slot 7 under both names; see the `Gray` variant's own doc.
            Color::White | Color::Gray => CtColor::Grey,
            Color::DarkGray => CtColor::DarkGrey,
            Color::BrightRed => CtColor::Red,
            Color::BrightGreen => CtColor::Green,
            Color::BrightYellow => CtColor::Yellow,
            Color::BrightBlue => CtColor::Blue,
            Color::BrightMagenta => CtColor::Magenta,
            Color::BrightCyan => CtColor::Cyan,
            Color::BrightWhite => CtColor::White,
            Color::Indexed(i) => CtColor::AnsiValue(i),
            Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
            Color::Reset => CtColor::Reset,
        }
    }

    /// Terminal palette index, for the colours that have one.
    ///
    /// `None` for `Rgb` and `Reset`, which are not palette entries.
    pub fn palette_index(self) -> Option<u8> {
        Some(match self {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::White | Color::Gray => 7,
            Color::DarkGray => 8,
            Color::BrightRed => 9,
            Color::BrightGreen => 10,
            Color::BrightYellow => 11,
            Color::BrightBlue => 12,
            Color::BrightMagenta => 13,
            Color::BrightCyan => 14,
            Color::BrightWhite => 15,
            Color::Indexed(i) => i,
            Color::Rgb(..) | Color::Reset => return None,
        })
    }

    /// SGR parameters for this colour as a foreground.
    ///
    /// Palette entries 0-15 emit the classic 30-37 / 90-97 codes rather than
    /// `38;5;n`, so they resolve against the terminal's own configured colours.
    /// `Gray` and `DarkGray` used to emit `38;5;250` / `38;5;240`, which pinned
    /// them to fixed greys the user's terminal theme could not influence — and
    /// disagreed with `to_crossterm`, which mapped them to the palette.
    ///
    /// The two render paths agree on the *slot* (not the encoding: this emits
    /// `30-37`/`90-97`, `to_crossterm` emits `38;5;n`). That agreement was
    /// asserted here in prose and nowhere in code, and was false for every
    /// chromatic colour until `to_crossterm` was corrected; it is now pinned
    /// by `named_colors_render_to_the_palette_slot_they_name`.
    pub fn to_ansi_fg(self) -> Vec<u8> {
        match self {
            Color::Rgb(r, g, b) => vec![38, 2, r, g, b],
            Color::Reset => vec![39],
            other => match other.palette_index() {
                Some(i @ 0..=7) => vec![30 + i],
                Some(i @ 8..=15) => vec![90 + (i - 8)],
                Some(i) => vec![38, 5, i],
                None => vec![39],
            },
        }
    }

    /// SGR parameters for this colour as a background. See [`Self::to_ansi_fg`].
    pub fn to_ansi_bg(self) -> Vec<u8> {
        match self {
            Color::Rgb(r, g, b) => vec![48, 2, r, g, b],
            Color::Reset => vec![49],
            other => match other.palette_index() {
                Some(i @ 0..=7) => vec![40 + i],
                Some(i @ 8..=15) => vec![100 + (i - 8)],
                Some(i) => vec![48, 5, i],
                None => vec![49],
            },
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
        let no_color = std::env::var("NO_COLOR").is_ok();
        self.resolve_inner(is_dark, no_color)
    }

    fn resolve_inner(self, is_dark: bool, no_color: bool) -> Color {
        if no_color {
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
    detect_dark_from_colorfgbg(std::env::var("COLORFGBG").ok().as_deref())
}

fn detect_dark_from_colorfgbg(colorfgbg: Option<&str>) -> bool {
    match colorfgbg {
        Some(value) => {
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
        None => {
            // Default to dark if COLORFGBG is not set
            true
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Padding {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub top: u16,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub right: u16,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub bottom: u16,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
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

#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Single,
    Double,
    Rounded,
    Heavy,
    /// An arbitrary character set, including asymmetric ones.
    ///
    /// Distinct top and bottom edges are not a hypothetical: the messages drawer
    /// draws `▀` above and `▄` below, which a shared `horizontal` could not
    /// express. That is why [`BorderChars`] carries eight edges rather than the
    /// four corners plus two shared runs it started with.
    Custom(BorderChars),
}

impl Border {
    pub fn chars(&self) -> BorderChars {
        match self {
            Border::Single => BorderChars::uniform('┌', '─', '┐', '│', '┘', '─', '└', '│'),
            Border::Double => BorderChars::uniform('╔', '═', '╗', '║', '╝', '═', '╚', '║'),
            Border::Rounded => BorderChars::uniform('╭', '─', '╮', '│', '╯', '─', '╰', '│'),
            Border::Heavy => BorderChars::uniform('┏', '━', '┓', '┃', '┛', '━', '┗', '┃'),
            Border::Custom(chars) => *chars,
        }
    }
}

/// The eight characters of a border, clockwise from the top-left corner.
///
/// Each is optional: `None` means that edge is **absent and occupies no cell**,
/// which is distinct from `Some(' ')` — a blank edge that still consumes one.
/// Layout honours the difference, so a surface can have a top rule and no sides
/// without paying for margins it does not draw.
///
/// The order matches Neovim's `nvim_open_win` border list, so a character set
/// can be copied between the two without reshuffling.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderChars {
    pub top_left: Option<char>,
    pub top: Option<char>,
    pub top_right: Option<char>,
    pub right: Option<char>,
    pub bottom_right: Option<char>,
    pub bottom: Option<char>,
    pub bottom_left: Option<char>,
    pub left: Option<char>,
}

impl BorderChars {
    /// Build a set where every position is present, clockwise from top-left.
    #[allow(clippy::too_many_arguments)]
    pub const fn uniform(
        top_left: char,
        top: char,
        top_right: char,
        right: char,
        bottom_right: char,
        bottom: char,
        bottom_left: char,
        left: char,
    ) -> Self {
        Self {
            top_left: Some(top_left),
            top: Some(top),
            top_right: Some(top_right),
            right: Some(right),
            bottom_right: Some(bottom_right),
            bottom: Some(bottom),
            bottom_left: Some(bottom_left),
            left: Some(left),
        }
    }

    /// Build from a clockwise-from-top-left list, repeating to fill eight.
    ///
    /// Matches Neovim: a list shorter than eight repeats modularly, so one entry
    /// fills every position, two alternate corner/edge, and four cycle. An empty
    /// string means that position is absent. A length that does not divide eight
    /// still works — position `i` takes `items[i % len]` — rather than being
    /// rejected, because a theme is not a gate.
    pub fn from_list(items: &[Option<char>]) -> Option<Self> {
        if items.is_empty() {
            return None;
        }
        let at = |i: usize| items[i % items.len()];
        Some(Self {
            top_left: at(0),
            top: at(1),
            top_right: at(2),
            right: at(3),
            bottom_right: at(4),
            bottom: at(5),
            bottom_left: at(6),
            left: at(7),
        })
    }

    /// Whether the left edge occupies a cell.
    pub fn has_left(&self) -> bool {
        self.left.is_some() || self.top_left.is_some() || self.bottom_left.is_some()
    }

    /// Whether the right edge occupies a cell.
    pub fn has_right(&self) -> bool {
        self.right.is_some() || self.top_right.is_some() || self.bottom_right.is_some()
    }

    /// Whether the top edge occupies a row.
    pub fn has_top(&self) -> bool {
        self.top.is_some() || self.top_left.is_some() || self.top_right.is_some()
    }

    /// Whether the bottom edge occupies a row.
    pub fn has_bottom(&self) -> bool {
        self.bottom.is_some() || self.bottom_left.is_some() || self.bottom_right.is_some()
    }
}

#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
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

#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gap {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub row: u16,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
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

    /// The frame's colour path and the markdown path must name the same
    /// palette slot.
    ///
    /// `to_crossterm` is what every rendered node goes through
    /// (`tree_render` → `ansi::apply_style`); `to_ansi_fg`/`palette_index` is
    /// what the markdown code-block path uses. crossterm's `Color::Red` is
    /// *bright* red (slot 9) and its `DarkRed` is normal red (slot 1) — the
    /// opposite of these names — so mapping name-to-name sent every chromatic
    /// colour to the wrong slot, and `bright_*` to the normal one. Asserted
    /// against the SGR crossterm actually emits, not against the variant
    /// names, because the names are exactly what misled.
    #[test]
    fn named_colors_render_to_the_palette_slot_they_name() {
        use crossterm::style::Colored;

        let named = [
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::White,
            Color::Gray,
            Color::DarkGray,
            Color::BrightRed,
            Color::BrightGreen,
            Color::BrightYellow,
            Color::BrightBlue,
            Color::BrightMagenta,
            Color::BrightCyan,
            Color::BrightWhite,
            Color::Indexed(2),
            Color::Indexed(200),
        ];

        for color in named {
            let expected = color
                .palette_index()
                .expect("every colour under test is a palette entry");
            let emitted = format!("{}", Colored::ForegroundColor(color.to_crossterm()));
            assert_eq!(
                emitted,
                format!("38;5;{expected}"),
                "{color:?} must render as palette slot {expected}"
            );
        }
    }

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

    /// Palette entries emit the classic SGR codes, so they resolve against the
    /// terminal's own colours rather than a fixed 256-colour approximation.
    #[test]
    fn palette_colors_emit_terminal_codes_not_256_color() {
        assert_eq!(Color::Blue.to_ansi_fg(), vec![34]);
        assert_eq!(Color::Blue.to_ansi_bg(), vec![44]);
        assert_eq!(Color::Indexed(4).to_ansi_fg(), vec![34]);
    }

    #[test]
    fn bright_colors_emit_the_90_series() {
        assert_eq!(Color::BrightBlue.to_ansi_fg(), vec![94]);
        assert_eq!(Color::BrightBlue.to_ansi_bg(), vec![104]);
        assert_eq!(Color::Indexed(12).to_ansi_fg(), vec![94]);
    }

    /// The regression this replaces: these two were pinned to 250/240 in the
    /// ANSI path while `to_crossterm` mapped them to the palette, so the same
    /// colour rendered differently depending on which path drew it.
    #[test]
    fn gray_and_dark_gray_follow_the_terminal_palette() {
        assert_eq!(Color::Gray.to_ansi_fg(), vec![37]);
        assert_eq!(Color::DarkGray.to_ansi_fg(), vec![90]);
    }

    /// Above 15 there is no terminal palette to defer to, so the fixed cube is
    /// correct.
    #[test]
    fn indices_above_fifteen_use_the_256_color_form() {
        assert_eq!(Color::Indexed(250).to_ansi_fg(), vec![38, 5, 250]);
    }

    #[test]
    fn rgb_is_unaffected() {
        assert_eq!(Color::Rgb(1, 2, 3).to_ansi_fg(), vec![38, 2, 1, 2, 3]);
    }

    #[test]
    fn named_colors_and_their_indices_agree() {
        assert_eq!(Color::Blue.palette_index(), Some(4));
        assert_eq!(Color::DarkGray.palette_index(), Some(8));
        assert_eq!(Color::BrightWhite.palette_index(), Some(15));
        assert_eq!(Color::Rgb(0, 0, 0).palette_index(), None);
    }

    #[test]
    fn test_border_chars_single() {
        let chars = Border::Single.chars();
        assert_eq!(chars.top_left, Some('┌'));
        assert_eq!(chars.top, Some('─'));
        assert_eq!(chars.bottom, Some('─'));
    }

    #[test]
    fn test_border_chars_rounded() {
        let chars = Border::Rounded.chars();
        assert_eq!(chars.top_left, Some('╭'));
        assert_eq!(chars.top_right, Some('╮'));
    }

    /// Neovim's shorthand: a list shorter than eight repeats modularly.
    #[test]
    fn a_single_element_border_list_fills_every_position() {
        let chars = BorderChars::from_list(&[Some('─')]).unwrap();
        assert_eq!(chars.top_left, Some('─'));
        assert_eq!(chars.left, Some('─'));
        assert_eq!(chars.bottom_right, Some('─'));
    }

    #[test]
    fn a_two_element_border_list_alternates_corners_and_edges() {
        let chars = BorderChars::from_list(&[Some('+'), Some('-')]).unwrap();
        assert_eq!(chars.top_left, Some('+'));
        assert_eq!(chars.top, Some('-'));
        assert_eq!(chars.top_right, Some('+'));
        assert_eq!(chars.right, Some('-'));
    }

    #[test]
    fn an_eight_element_border_list_is_clockwise_from_top_left() {
        let items: Vec<Option<char>> = "╔═╗║╝═╚║".chars().map(Some).collect();
        let chars = BorderChars::from_list(&items).unwrap();
        assert_eq!(chars.top_left, Some('╔'));
        assert_eq!(chars.top, Some('═'));
        assert_eq!(chars.top_right, Some('╗'));
        assert_eq!(chars.right, Some('║'));
        assert_eq!(chars.bottom_right, Some('╝'));
        assert_eq!(chars.bottom, Some('═'));
        assert_eq!(chars.bottom_left, Some('╚'));
        assert_eq!(chars.left, Some('║'));
    }

    /// The drawer's shape: horizontal rules, no sides. An absent edge reports
    /// itself absent so layout can decline to reserve a cell for it.
    #[test]
    fn absent_edges_are_distinguishable_from_present_ones() {
        let chars =
            BorderChars::from_list(&[None, Some('▀'), None, None, None, Some('▄'), None, None])
                .unwrap();

        assert!(chars.has_top());
        assert!(chars.has_bottom());
        assert!(!chars.has_left(), "no left edge should be reserved");
        assert!(!chars.has_right(), "no right edge should be reserved");
    }

    /// A blank edge still occupies its cell — that is the difference between
    /// "no border here" and "an intentionally empty border here".
    #[test]
    fn a_space_edge_occupies_a_cell_unlike_an_absent_one() {
        let blank = BorderChars::from_list(&[Some(' ')]).unwrap();
        assert!(blank.has_left());

        let absent = BorderChars::from_list(&[None]).unwrap();
        assert!(!absent.has_left());
    }

    #[test]
    fn an_empty_border_list_is_rejected() {
        assert_eq!(BorderChars::from_list(&[]), None);
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
        assert_eq!(ac.resolve_inner(true, false), Color::Red);
    }

    #[test]
    fn test_adaptive_color_resolve_light() {
        let ac = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };
        assert_eq!(ac.resolve_inner(false, false), Color::Blue);
    }

    #[test]
    fn test_adaptive_color_no_color() {
        let ac = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };
        assert_eq!(ac.resolve_inner(true, true), Color::Reset);
        assert_eq!(ac.resolve_inner(false, true), Color::Reset);
    }

    #[test]
    fn test_detect_dark_terminal_dark_bg() {
        assert!(detect_dark_from_colorfgbg(Some("15;0")));
    }

    #[test]
    fn test_detect_dark_terminal_light_bg() {
        assert!(!detect_dark_from_colorfgbg(Some("0;15")));
    }

    #[test]
    fn test_detect_dark_terminal_not_set() {
        assert!(detect_dark_from_colorfgbg(None));
    }

    #[test]
    fn test_detect_dark_terminal_invalid_format() {
        assert!(detect_dark_from_colorfgbg(Some("invalid")));
    }

    #[test]
    fn test_detect_dark_terminal_boundary() {
        assert!(detect_dark_from_colorfgbg(Some("15;7")));
        assert!(!detect_dark_from_colorfgbg(Some("15;8")));
    }
}
