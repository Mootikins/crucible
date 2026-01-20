use crate::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span<'a> {
    pub text: &'a str,
    pub style: Style,
}

impl<'a> Span<'a> {
    pub fn new(text: &'a str, style: Style) -> Self {
        Self { text, style }
    }

    pub fn plain(text: &'a str) -> Self {
        Self {
            text,
            style: Style::default(),
        }
    }

    pub fn width(&self) -> usize {
        self.text.width()
    }

    pub fn to_ansi(&self) -> String {
        if self.style == Style::default() {
            self.text.to_string()
        } else {
            format!("{}{}\x1b[0m", self.style.to_ansi_codes(), self.text)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl<'a> Default for Span<'a> {
    fn default() -> Self {
        Self::plain("")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanLine<'a> {
    pub spans: Vec<Span<'a>>,
}

impl<'a> SpanLine<'a> {
    pub fn new(spans: Vec<Span<'a>>) -> Self {
        Self { spans }
    }

    pub fn single(span: Span<'a>) -> Self {
        Self { spans: vec![span] }
    }

    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    pub fn to_ansi(&self) -> String {
        self.spans.iter().map(|s| s.to_ansi()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.is_empty())
    }

    pub fn push(&mut self, span: Span<'a>) {
        self.spans.push(span);
    }

    pub fn truncate_to_width(&self, max_width: usize) -> SpanLine<'a> {
        let mut result = Vec::new();
        let mut remaining = max_width;

        for span in &self.spans {
            if remaining == 0 {
                break;
            }

            let span_width = span.width();
            if span_width <= remaining {
                result.push(*span);
                remaining -= span_width;
            } else {
                let truncated = truncate_str_to_width(span.text, remaining);
                if !truncated.is_empty() {
                    result.push(Span::new(truncated, span.style));
                }
                break;
            }
        }

        SpanLine::new(result)
    }
}

pub struct OwnedSpanLine {
    pub spans: Vec<OwnedSpan>,
}

pub struct OwnedSpan {
    pub text: String,
    pub style: Style,
}

impl OwnedSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }

    pub fn width(&self) -> usize {
        self.text.width()
    }

    pub fn to_ansi(&self) -> String {
        if self.style == Style::default() {
            self.text.clone()
        } else {
            format!("{}{}\x1b[0m", self.style.to_ansi_codes(), self.text)
        }
    }

    pub fn as_span(&self) -> Span<'_> {
        Span::new(&self.text, self.style)
    }
}

impl OwnedSpanLine {
    pub fn new(spans: Vec<OwnedSpan>) -> Self {
        Self { spans }
    }

    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    pub fn to_ansi(&self) -> String {
        self.spans.iter().map(|s| s.to_ansi()).collect()
    }

    pub fn as_span_line(&self) -> SpanLine<'_> {
        SpanLine::new(self.spans.iter().map(|s| s.as_span()).collect())
    }

    pub fn pad_to_width(&mut self, target_width: usize, pad_style: Style) {
        let current_width = self.width();
        if current_width >= target_width {
            return;
        }

        let padding_needed = target_width - current_width;
        self.spans
            .push(OwnedSpan::new(" ".repeat(padding_needed), pad_style));
    }

    pub fn push(&mut self, span: OwnedSpan) {
        self.spans.push(span);
    }
}

fn truncate_str_to_width(s: &str, max_width: usize) -> &str {
    if s.width() <= max_width {
        return s;
    }

    let mut width = 0;
    let mut end_idx = 0;

    for (idx, c) in s.char_indices() {
        let char_width = c.width().unwrap_or(0);
        if width + char_width > max_width {
            break;
        }
        width += char_width;
        end_idx = idx + c.len_utf8();
    }

    &s[..end_idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn span_width_ascii() {
        let span = Span::new("hello", Style::new());
        assert_eq!(span.width(), 5);
    }

    #[test]
    fn span_width_cjk() {
        let span = Span::new("你好", Style::new());
        assert_eq!(span.width(), 4);
    }

    #[test]
    fn span_width_mixed() {
        let span = Span::new("hi你好", Style::new());
        assert_eq!(span.width(), 6);
    }

    #[test]
    fn span_to_ansi_plain() {
        let span = Span::plain("hello");
        assert_eq!(span.to_ansi(), "hello");
    }

    #[test]
    fn span_to_ansi_styled() {
        let span = Span::new("hello", Style::new().bold());
        let ansi = span.to_ansi();
        assert!(ansi.contains("\x1b[1m"), "should contain bold code");
        assert!(ansi.contains("hello"));
        assert!(ansi.contains("\x1b[0m"), "should contain reset");
    }

    #[test]
    fn span_to_ansi_with_color() {
        let span = Span::new("red", Style::new().fg(Color::Red));
        let ansi = span.to_ansi();
        assert!(ansi.contains("31"), "should contain red fg code");
    }

    #[test]
    fn span_line_width() {
        let line = SpanLine::new(vec![
            Span::new("Hello ", Style::new()),
            Span::new("World", Style::new().bold()),
        ]);
        assert_eq!(line.width(), 11);
    }

    #[test]
    fn span_line_to_ansi() {
        let line = SpanLine::new(vec![
            Span::plain("Hello "),
            Span::new("World", Style::new().bold()),
        ]);
        let ansi = line.to_ansi();
        assert!(ansi.contains("Hello "));
        assert!(ansi.contains("World"));
    }

    #[test]
    fn span_line_empty() {
        let line = SpanLine::empty();
        assert!(line.is_empty());
        assert_eq!(line.width(), 0);
    }

    #[test]
    fn span_line_truncate_exact() {
        let line = SpanLine::new(vec![Span::plain("hello")]);
        let truncated = line.truncate_to_width(5);
        assert_eq!(truncated.width(), 5);
    }

    #[test]
    fn span_line_truncate_shorter() {
        let line = SpanLine::new(vec![Span::plain("hello world")]);
        let truncated = line.truncate_to_width(5);
        assert_eq!(truncated.width(), 5);
        assert_eq!(truncated.to_ansi(), "hello");
    }

    #[test]
    fn span_line_truncate_multi_span() {
        let line = SpanLine::new(vec![Span::plain("abc"), Span::plain("def")]);
        let truncated = line.truncate_to_width(4);
        assert_eq!(truncated.width(), 4);
    }

    #[test]
    fn span_line_truncate_preserves_style() {
        let line = SpanLine::new(vec![Span::new("hello", Style::new().bold())]);
        let truncated = line.truncate_to_width(3);
        assert_eq!(truncated.spans.len(), 1);
        assert!(truncated.spans[0].style.bold);
    }

    #[test]
    fn truncate_str_to_width_ascii() {
        assert_eq!(truncate_str_to_width("hello", 3), "hel");
        assert_eq!(truncate_str_to_width("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_to_width_unicode() {
        assert_eq!(truncate_str_to_width("你好世界", 4), "你好");
        assert_eq!(truncate_str_to_width("你好世界", 3), "你");
    }

    #[test]
    fn span_default() {
        let span = Span::default();
        assert!(span.is_empty());
        assert_eq!(span.width(), 0);
    }

    #[test]
    fn span_line_single() {
        let line = SpanLine::single(Span::plain("test"));
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.width(), 4);
    }

    #[test]
    fn span_line_push() {
        let mut line = SpanLine::empty();
        line.push(Span::plain("a"));
        line.push(Span::plain("b"));
        assert_eq!(line.width(), 2);
        assert_eq!(line.spans.len(), 2);
    }
}
