use crate::span::{Span, SpanLine};
use crate::style::Style;

pub trait ContentSource {
    fn get_content(&self, id: &str) -> Option<&str>;
}

pub struct Compositor<'a> {
    content_source: &'a dyn ContentSource,
    width: usize,
    lines: Vec<SpanLine<'a>>,
}

impl<'a> Compositor<'a> {
    pub fn new(source: &'a dyn ContentSource, width: usize) -> Self {
        Self {
            content_source: source,
            width,
            lines: Vec::new(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn push_empty(&mut self) {
        self.lines.push(SpanLine::empty());
    }

    pub fn push_text(&mut self, text: &'a str) {
        self.lines.push(SpanLine::single(Span::plain(text)));
    }

    pub fn push_styled(&mut self, text: &'a str, style: Style) {
        self.lines.push(SpanLine::single(Span::new(text, style)));
    }

    pub fn push_spans(&mut self, spans: Vec<Span<'a>>) {
        self.lines.push(SpanLine::new(spans));
    }

    pub fn push_line(&mut self, line: SpanLine<'a>) {
        self.lines.push(line);
    }

    pub fn render_message(&mut self, id: &str, style: Style) -> bool {
        if let Some(content) = self.content_source.get_content(id) {
            for line in content.lines() {
                self.push_styled(line, style);
            }
            true
        } else {
            false
        }
    }

    pub fn render_message_with_prefix(
        &mut self,
        id: &str,
        prefix: Span<'a>,
        content_style: Style,
    ) -> bool {
        if let Some(content) = self.content_source.get_content(id) {
            let mut first = true;
            for line in content.lines() {
                if first {
                    self.push_spans(vec![prefix, Span::new(line, content_style)]);
                    first = false;
                } else {
                    self.push_styled(line, content_style);
                }
            }
            true
        } else {
            false
        }
    }

    pub fn finish(self) -> Vec<SpanLine<'a>> {
        self.lines
    }

    pub fn to_ansi_lines(&self) -> Vec<String> {
        self.lines.iter().map(|line| line.to_ansi()).collect()
    }
}

pub struct StaticCompositor {
    width: usize,
    lines: Vec<String>,
}

impl StaticCompositor {
    pub fn new(width: usize) -> Self {
        Self {
            width,
            lines: Vec::new(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn push_empty(&mut self) {
        self.lines.push(String::new());
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        self.lines.push(text.into());
    }

    pub fn push_styled(&mut self, text: &str, style: Style) {
        if style == Style::default() {
            self.lines.push(text.to_string());
        } else {
            self.lines
                .push(format!("{}{}\x1b[0m", style.to_ansi_codes(), text));
        }
    }

    pub fn push_line(&mut self, line: String) {
        self.lines.push(line);
    }

    pub fn finish(self) -> Vec<String> {
        self.lines
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    struct TestSource {
        content: String,
    }

    impl ContentSource for TestSource {
        fn get_content(&self, _id: &str) -> Option<&str> {
            Some(&self.content)
        }
    }

    struct EmptySource;

    impl ContentSource for EmptySource {
        fn get_content(&self, _id: &str) -> Option<&str> {
            None
        }
    }

    #[test]
    fn compositor_borrows_from_source() {
        let source = TestSource {
            content: "Hello World".to_string(),
        };

        let mut comp = Compositor::new(&source, 80);
        assert!(comp.render_message("any", Style::new()));

        let lines = comp.finish();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].text, "Hello World");
    }

    #[test]
    fn compositor_returns_false_for_missing_content() {
        let source = EmptySource;

        let mut comp = Compositor::new(&source, 80);
        assert!(!comp.render_message("nonexistent", Style::new()));
        assert_eq!(comp.line_count(), 0);
    }

    #[test]
    fn compositor_push_text() {
        let source = EmptySource;
        let mut comp = Compositor::new(&source, 80);

        comp.push_text("line1");
        comp.push_text("line2");

        assert_eq!(comp.line_count(), 2);
        let lines = comp.finish();
        assert_eq!(lines[0].to_ansi(), "line1");
        assert_eq!(lines[1].to_ansi(), "line2");
    }

    #[test]
    fn compositor_push_styled() {
        let source = EmptySource;
        let mut comp = Compositor::new(&source, 80);

        comp.push_styled("bold text", Style::new().bold());

        let lines = comp.finish();
        let ansi = lines[0].to_ansi();
        assert!(ansi.contains("\x1b[1m"));
        assert!(ansi.contains("bold text"));
    }

    #[test]
    fn compositor_push_spans() {
        let source = EmptySource;
        let mut comp = Compositor::new(&source, 80);

        comp.push_spans(vec![
            Span::new("red ", Style::new().fg(Color::Red)),
            Span::new("blue", Style::new().fg(Color::Blue)),
        ]);

        let lines = comp.finish();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 2);
    }

    #[test]
    fn compositor_multiline_content() {
        let source = TestSource {
            content: "line1\nline2\nline3".to_string(),
        };

        let mut comp = Compositor::new(&source, 80);
        comp.render_message("id", Style::new());

        let lines = comp.finish();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn compositor_to_ansi_lines() {
        let source = EmptySource;
        let mut comp = Compositor::new(&source, 80);

        comp.push_text("plain");
        comp.push_styled("bold", Style::new().bold());

        let ansi = comp.to_ansi_lines();
        assert_eq!(ansi.len(), 2);
        assert_eq!(ansi[0], "plain");
        assert!(ansi[1].contains("\x1b[1m"));
    }

    #[test]
    fn compositor_render_with_prefix() {
        let source = TestSource {
            content: "message content".to_string(),
        };

        let mut comp = Compositor::new(&source, 80);
        comp.render_message_with_prefix(
            "id",
            Span::new("User: ", Style::new().bold()),
            Style::new(),
        );

        let lines = comp.finish();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 2);
        assert_eq!(lines[0].spans[0].text, "User: ");
    }

    #[test]
    fn static_compositor_push_text() {
        let mut comp = StaticCompositor::new(80);
        comp.push_text("hello");
        comp.push_text(String::from("world"));

        assert_eq!(comp.line_count(), 2);
        assert_eq!(comp.lines()[0], "hello");
        assert_eq!(comp.lines()[1], "world");
    }

    #[test]
    fn static_compositor_push_styled() {
        let mut comp = StaticCompositor::new(80);
        comp.push_styled("bold", Style::new().bold());

        let lines = comp.finish();
        assert!(lines[0].contains("\x1b[1m"));
    }

    #[test]
    fn compositor_lifetime_scoped_to_render() {
        fn render_pass(source: &impl ContentSource) -> Vec<String> {
            let mut comp = Compositor::new(source, 80);
            comp.push_text("test");
            comp.to_ansi_lines()
        }

        let result = render_pass(&EmptySource);
        assert!(!result.is_empty());
    }

    #[test]
    fn compositor_empty_line() {
        let source = EmptySource;
        let mut comp = Compositor::new(&source, 80);

        comp.push_empty();

        let lines = comp.finish();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].is_empty());
    }
}
