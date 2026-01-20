use crate::ansi::visible_width;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    pub content: String,
    pub width: usize,
}

impl RenderedLine {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let width = visible_width(&content);
        Self { content, width }
    }

    pub fn blank() -> Self {
        Self {
            content: String::new(),
            width: 0,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.content.is_empty() || self.content.chars().all(|c| c.is_whitespace())
    }
}

impl Default for RenderedLine {
    fn default() -> Self {
        Self::blank()
    }
}

pub struct LineBuffer {
    lines: VecDeque<RenderedLine>,
    capacity: usize,
    width: usize,
}

impl LineBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let capacity = height.saturating_sub(1);
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
            width,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.capacity = height.saturating_sub(1);

        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }

    pub fn set_lines(&mut self, lines: impl IntoIterator<Item = RenderedLine>) {
        self.lines.clear();
        for line in lines {
            if self.lines.len() < self.capacity {
                self.lines.push_back(line);
            }
        }
    }

    pub fn set_from_strings(&mut self, lines: impl IntoIterator<Item = String>) {
        self.set_lines(lines.into_iter().map(RenderedLine::new));
    }

    pub fn push(&mut self, line: RenderedLine) {
        if self.lines.len() >= self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn diff(&self, other: &LineBuffer) -> LineDiff {
        let mut ops = Vec::new();

        let max_lines = self.lines.len().max(other.lines.len());
        for i in 0..max_lines {
            let old = self.lines.get(i);
            let new = other.lines.get(i);

            match (old, new) {
                (Some(o), Some(n)) if o != n => {
                    ops.push(DiffOp::Replace(i, n.clone()));
                }
                (None, Some(n)) => {
                    ops.push(DiffOp::Insert(i, n.clone()));
                }
                (Some(_), None) => {
                    ops.push(DiffOp::Clear(i));
                }
                _ => {}
            }
        }

        LineDiff { ops }
    }

    pub fn get(&self, index: usize) -> Option<&RenderedLine> {
        self.lines.get(index)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RenderedLine> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn to_string_vec(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.content.clone()).collect()
    }

    pub fn clone_into(&self, other: &mut LineBuffer) {
        other.lines.clear();
        other.lines.extend(self.lines.iter().cloned());
        other.capacity = self.capacity;
        other.width = self.width;
    }
}

impl Clone for LineBuffer {
    fn clone(&self) -> Self {
        Self {
            lines: self.lines.clone(),
            capacity: self.capacity,
            width: self.width,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    Replace(usize, RenderedLine),
    Insert(usize, RenderedLine),
    Clear(usize),
}

#[derive(Debug, Clone, Default)]
pub struct LineDiff {
    pub ops: Vec<DiffOp>,
}

impl LineDiff {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_line_calculates_width() {
        let line = RenderedLine::new("hello");
        assert_eq!(line.width, 5);
    }

    #[test]
    fn rendered_line_handles_ansi() {
        let line = RenderedLine::new("\x1b[1mhello\x1b[0m");
        assert_eq!(line.width, 5);
    }

    #[test]
    fn rendered_line_blank() {
        let line = RenderedLine::blank();
        assert!(line.is_blank());
        assert_eq!(line.width, 0);
    }

    #[test]
    fn line_buffer_respects_capacity() {
        let mut buf = LineBuffer::new(80, 10);

        let lines: Vec<_> = (0..20)
            .map(|i| RenderedLine::new(format!("Line {}", i)))
            .collect();

        buf.set_lines(lines);

        assert_eq!(buf.len(), 9);
    }

    #[test]
    fn line_buffer_diff_detects_changes() {
        let mut old = LineBuffer::new(80, 10);
        old.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Line 1"),
        ]);

        let mut new = LineBuffer::new(80, 10);
        new.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Changed"),
        ]);

        let diff = old.diff(&new);

        assert_eq!(diff.ops.len(), 1);
        assert!(matches!(diff.ops[0], DiffOp::Replace(1, _)));
    }

    #[test]
    fn line_buffer_diff_detects_insertions() {
        let mut old = LineBuffer::new(80, 10);
        old.set_lines(vec![RenderedLine::new("Line 0")]);

        let mut new = LineBuffer::new(80, 10);
        new.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Line 1"),
        ]);

        let diff = old.diff(&new);

        assert_eq!(diff.ops.len(), 1);
        assert!(matches!(diff.ops[0], DiffOp::Insert(1, _)));
    }

    #[test]
    fn line_buffer_diff_detects_clears() {
        let mut old = LineBuffer::new(80, 10);
        old.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Line 1"),
        ]);

        let mut new = LineBuffer::new(80, 10);
        new.set_lines(vec![RenderedLine::new("Line 0")]);

        let diff = old.diff(&new);

        assert_eq!(diff.ops.len(), 1);
        assert!(matches!(diff.ops[0], DiffOp::Clear(1)));
    }

    #[test]
    fn line_buffer_diff_empty_when_same() {
        let mut buf1 = LineBuffer::new(80, 10);
        buf1.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Line 1"),
        ]);

        let mut buf2 = LineBuffer::new(80, 10);
        buf2.set_lines(vec![
            RenderedLine::new("Line 0"),
            RenderedLine::new("Line 1"),
        ]);

        let diff = buf1.diff(&buf2);
        assert!(diff.is_empty());
    }

    #[test]
    fn line_buffer_resize_truncates() {
        let mut buf = LineBuffer::new(80, 20);
        buf.set_lines((0..15).map(|i| RenderedLine::new(format!("L{}", i))));

        assert_eq!(buf.len(), 15);

        buf.resize(80, 10);

        assert_eq!(buf.len(), 9);
        assert!(buf.get(0).unwrap().content.contains("L6"));
    }

    #[test]
    fn line_buffer_push_evicts_oldest() {
        let mut buf = LineBuffer::new(80, 4);

        buf.push(RenderedLine::new("L0"));
        buf.push(RenderedLine::new("L1"));
        buf.push(RenderedLine::new("L2"));
        assert_eq!(buf.len(), 3);

        buf.push(RenderedLine::new("L3"));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.get(0).unwrap().content, "L1");
    }

    #[test]
    fn line_buffer_set_from_strings() {
        let mut buf = LineBuffer::new(80, 10);
        buf.set_from_strings(vec!["line1".to_string(), "line2".to_string()]);

        assert_eq!(buf.len(), 2);
        assert_eq!(buf.get(0).unwrap().content, "line1");
    }

    #[test]
    fn line_buffer_to_string_vec() {
        let mut buf = LineBuffer::new(80, 10);
        buf.set_lines(vec![RenderedLine::new("line1"), RenderedLine::new("line2")]);

        let strings = buf.to_string_vec();
        assert_eq!(strings, vec!["line1", "line2"]);
    }

    #[test]
    fn line_buffer_clone_into() {
        let mut buf1 = LineBuffer::new(80, 10);
        buf1.set_lines(vec![RenderedLine::new("line1"), RenderedLine::new("line2")]);

        let mut buf2 = LineBuffer::new(40, 5);
        buf1.clone_into(&mut buf2);

        assert_eq!(buf2.len(), 2);
        assert_eq!(buf2.width(), 80);
        assert_eq!(buf2.get(0).unwrap().content, "line1");
    }

    #[test]
    fn line_buffer_clear() {
        let mut buf = LineBuffer::new(80, 10);
        buf.set_lines(vec![RenderedLine::new("line1")]);

        buf.clear();

        assert!(buf.is_empty());
    }

    #[test]
    fn line_diff_len() {
        let diff = LineDiff {
            ops: vec![DiffOp::Replace(0, RenderedLine::new("a")), DiffOp::Clear(1)],
        };
        assert_eq!(diff.len(), 2);
    }
}
