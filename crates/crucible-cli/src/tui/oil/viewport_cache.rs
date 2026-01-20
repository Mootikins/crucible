use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use textwrap::{wrap, Options, WordSplitter};

use super::chat_app::Role;
use crucible_oil::ContentSource;

/// Default maximum cached items in viewport (can be overridden via `with_max_items`)
pub const DEFAULT_MAX_CACHED_ITEMS: usize = 32;

#[derive(Debug, Clone)]
pub struct CachedMessage {
    pub id: String,
    pub role: Role,
    content: Arc<str>,
    wrapped: Option<(usize, Vec<String>)>,
}

impl CachedMessage {
    pub fn new(id: impl Into<String>, role: Role, content: impl AsRef<str>) -> Self {
        Self {
            id: id.into(),
            role,
            content: Arc::from(content.as_ref()),
            wrapped: None,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn wrapped_lines(&mut self, width: usize) -> &[String] {
        if self.wrapped.as_ref().map(|(w, _)| *w) != Some(width) {
            let lines = wrap_content(&self.content, width);
            self.wrapped = Some((width, lines));
        }
        &self.wrapped.as_ref().unwrap().1
    }

    pub fn invalidate_wrap(&mut self) {
        self.wrapped = None;
    }
}

#[derive(Debug, Clone)]
pub struct CachedToolCall {
    pub id: String,
    pub name: Arc<str>,
    pub args: Arc<str>,
    pub result: String,
    pub complete: bool,
}

impl CachedToolCall {
    pub fn new(id: impl Into<String>, name: impl AsRef<str>, args: impl AsRef<str>) -> Self {
        Self {
            id: id.into(),
            name: Arc::from(name.as_ref()),
            args: Arc::from(args.as_ref()),
            result: String::new(),
            complete: false,
        }
    }

    pub fn append_result(&mut self, delta: &str) {
        self.result.push_str(delta);
    }

    pub fn mark_complete(&mut self) {
        self.complete = true;
    }
}

#[derive(Debug, Clone)]
pub struct CachedShellExecution {
    pub id: String,
    pub command: Arc<str>,
    pub exit_code: i32,
    pub output_tail: Vec<Arc<str>>,
    pub output_path: Option<PathBuf>,
}

impl CachedShellExecution {
    pub fn new(
        id: impl Into<String>,
        command: impl AsRef<str>,
        exit_code: i32,
        output_tail: Vec<String>,
        output_path: Option<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            command: Arc::from(command.as_ref()),
            exit_code,
            output_tail: output_tail
                .into_iter()
                .map(|s| Arc::from(s.as_str()))
                .collect(),
            output_path,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CachedChatItem {
    Message(CachedMessage),
    ToolCall(CachedToolCall),
    ShellExecution(CachedShellExecution),
}

impl CachedChatItem {
    pub fn id(&self) -> &str {
        match self {
            CachedChatItem::Message(m) => &m.id,
            CachedChatItem::ToolCall(t) => &t.id,
            CachedChatItem::ShellExecution(s) => &s.id,
        }
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            CachedChatItem::Message(m) => Some(m.content()),
            _ => None,
        }
    }

    pub fn as_message(&self) -> Option<&CachedMessage> {
        match self {
            CachedChatItem::Message(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_message_mut(&mut self) -> Option<&mut CachedMessage> {
        match self {
            CachedChatItem::Message(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_tool_call(&self) -> Option<&CachedToolCall> {
        match self {
            CachedChatItem::ToolCall(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_tool_call_mut(&mut self) -> Option<&mut CachedToolCall> {
        match self {
            CachedChatItem::ToolCall(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_shell_execution(&self) -> Option<&CachedShellExecution> {
        match self {
            CachedChatItem::ShellExecution(s) => Some(s),
            _ => None,
        }
    }
}

pub struct ViewportCache {
    items: VecDeque<CachedChatItem>,
    max_items: usize,
    streaming: Option<StreamingBuffer>,
    anchor: Option<ViewportAnchor>,
}

#[derive(Debug, Clone)]
pub struct ViewportAnchor {
    pub message_id: String,
    pub line_offset: usize,
}

impl Default for ViewportCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportCache {
    pub fn new() -> Self {
        Self::with_max_items(DEFAULT_MAX_CACHED_ITEMS)
    }

    pub fn with_max_items(max_items: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(max_items),
            max_items,
            streaming: None,
            anchor: None,
        }
    }

    pub fn max_items(&self) -> usize {
        self.max_items
    }

    pub fn push_item(&mut self, item: CachedChatItem) {
        if self.items.len() >= self.max_items {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    pub fn push_message(&mut self, msg: CachedMessage) {
        self.push_item(CachedChatItem::Message(msg));
    }

    pub fn push_tool_call(&mut self, id: String, name: &str, args: &str) {
        self.push_item(CachedChatItem::ToolCall(CachedToolCall::new(
            id, name, args,
        )));
    }

    pub fn push_shell_execution(
        &mut self,
        id: String,
        command: &str,
        exit_code: i32,
        output_tail: Vec<String>,
        output_path: Option<PathBuf>,
    ) {
        self.push_item(CachedChatItem::ShellExecution(CachedShellExecution::new(
            id,
            command,
            exit_code,
            output_tail,
            output_path,
        )));
    }

    pub fn items(&self) -> impl Iterator<Item = &CachedChatItem> {
        self.items.iter()
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn get_item(&self, id: &str) -> Option<&CachedChatItem> {
        self.items.iter().find(|item| item.id() == id)
    }

    pub fn get_item_mut(&mut self, id: &str) -> Option<&mut CachedChatItem> {
        self.items.iter_mut().find(|item| item.id() == id)
    }

    pub fn find_tool_mut(&mut self, name: &str) -> Option<&mut CachedToolCall> {
        self.items.iter_mut().rev().find_map(|item| match item {
            CachedChatItem::ToolCall(t) if t.name.as_ref() == name => Some(t),
            _ => None,
        })
    }

    pub fn append_tool_result(&mut self, name: &str, delta: &str) {
        if let Some(tool) = self.find_tool_mut(name) {
            tool.append_result(delta);
        }
    }

    pub fn complete_tool(&mut self, name: &str) {
        if let Some(tool) = self.find_tool_mut(name) {
            tool.mark_complete();
        }
    }

    pub fn get_content(&self, id: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|item| item.id() == id)
            .and_then(|item| item.content())
    }

    pub fn get_message(&self, id: &str) -> Option<&CachedMessage> {
        self.items
            .iter()
            .find_map(|item| item.as_message().filter(|m| m.id == id))
    }

    pub fn get_message_mut(&mut self, id: &str) -> Option<&mut CachedMessage> {
        self.items
            .iter_mut()
            .find_map(|item| item.as_message_mut().filter(|m| m.id == id))
    }

    pub fn messages(&self) -> impl Iterator<Item = &CachedMessage> {
        self.items.iter().filter_map(|item| item.as_message())
    }

    pub fn message_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.as_message().is_some())
            .count()
    }

    pub fn start_streaming(&mut self) {
        self.streaming = Some(StreamingBuffer::new());
    }

    pub fn append_streaming(&mut self, delta: &str) {
        if let Some(ref mut buf) = self.streaming {
            buf.append(delta);
        }
    }

    pub fn streaming_content(&self) -> Option<&str> {
        self.streaming.as_ref().map(|b| b.content())
    }

    pub fn streaming_graduated_content(&self) -> Option<&str> {
        self.streaming.as_ref().map(|b| b.graduated_content())
    }

    pub fn streaming_graduated_blocks(&self) -> Option<&[String]> {
        self.streaming.as_ref().map(|b| b.graduated_blocks())
    }

    pub fn streaming_in_progress_content(&self) -> Option<&str> {
        self.streaming.as_ref().map(|b| b.in_progress_content())
    }

    pub fn streaming_graduated_block_count(&self) -> usize {
        self.streaming
            .as_ref()
            .map(|b| b.graduated_block_count())
            .unwrap_or(0)
    }

    pub fn has_streaming_graduated_content(&self) -> bool {
        self.streaming
            .as_ref()
            .map(|b| b.has_graduated_content())
            .unwrap_or(false)
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming.is_some()
    }

    pub fn complete_streaming(&mut self, id: String, role: Role) {
        if let Some(buf) = self.streaming.take() {
            let content = buf.into_content();
            self.push_message(CachedMessage::new(id, role, content));
        }
    }

    pub fn cancel_streaming(&mut self) {
        self.streaming = None;
    }

    pub fn set_anchor(&mut self, anchor: ViewportAnchor) {
        self.anchor = Some(anchor);
    }

    pub fn anchor(&self) -> Option<&ViewportAnchor> {
        self.anchor.as_ref()
    }

    pub fn clear_anchor(&mut self) {
        self.anchor = None;
    }

    pub fn invalidate_all_wraps(&mut self) {
        for item in &mut self.items {
            if let Some(msg) = item.as_message_mut() {
                msg.invalidate_wrap();
            }
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.streaming = None;
        self.anchor = None;
    }
}

impl ContentSource for ViewportCache {
    fn get_content(&self, id: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|item| item.id() == id)
            .and_then(|item| item.content())
    }
}

pub struct StreamingBuffer {
    graduated_blocks: Vec<String>,
    in_progress: String,
}

impl StreamingBuffer {
    pub fn new() -> Self {
        Self {
            graduated_blocks: Vec::new(),
            in_progress: String::new(),
        }
    }

    pub fn append(&mut self, delta: &str) {
        self.in_progress.push_str(delta);
        self.try_graduate_blocks();
    }

    pub fn content(&self) -> &str {
        &self.in_progress
    }

    pub fn all_content(&self) -> String {
        let graduated: String = self.graduated_blocks.concat();
        if graduated.is_empty() {
            self.in_progress.clone()
        } else if self.in_progress.is_empty() {
            graduated
        } else {
            format!("{}{}", graduated, self.in_progress)
        }
    }

    pub fn graduated_content(&self) -> &str {
        if self.graduated_blocks.is_empty() {
            ""
        } else if self.graduated_blocks.len() == 1 {
            &self.graduated_blocks[0]
        } else {
            ""
        }
    }

    pub fn graduated_blocks(&self) -> &[String] {
        &self.graduated_blocks
    }

    pub fn in_progress_content(&self) -> &str {
        &self.in_progress
    }

    pub fn graduated_block_count(&self) -> usize {
        self.graduated_blocks.len()
    }

    pub fn has_graduated_content(&self) -> bool {
        !self.graduated_blocks.is_empty()
    }

    pub fn into_content(self) -> String {
        self.all_content()
    }

    pub fn len(&self) -> usize {
        self.graduated_blocks.iter().map(|b| b.len()).sum::<usize>() + self.in_progress.len()
    }

    pub fn is_empty(&self) -> bool {
        self.graduated_blocks.is_empty() && self.in_progress.is_empty()
    }

    fn try_graduate_blocks(&mut self) {
        if let Some(split_pos) = self.find_graduation_point() {
            let to_graduate = self.in_progress[..split_pos].to_string();
            let remaining = self.in_progress[split_pos..].to_string();

            if !to_graduate.is_empty() {
                self.graduated_blocks.push(to_graduate);
                self.in_progress = remaining;
            }
        }
    }

    /// Finds byte offset where content can graduate. Graduates at blank lines (\n\n)
    /// but never inside unclosed code blocks (```)
    fn find_graduation_point(&self) -> Option<usize> {
        let content = &self.in_progress;

        if content.len() < 4 {
            return None;
        }

        let fence_count = content.matches("```").count();
        if fence_count % 2 != 0 {
            return None;
        }

        let mut last_valid_split = None;
        let mut in_code_block = false;
        let mut i = 0;
        let bytes = content.as_bytes();

        while i < bytes.len() {
            if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"```" {
                in_code_block = !in_code_block;
                i += 3;
                continue;
            }

            if !in_code_block && i + 2 <= bytes.len() && &bytes[i..i + 2] == b"\n\n" {
                last_valid_split = Some(i + 2);
            }

            i += 1;
        }

        last_valid_split
    }
}

impl Default for StreamingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

fn wrap_content(content: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![content.to_string()];
    }

    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);

    content
        .lines()
        .flat_map(|line| {
            if line.is_empty() {
                vec![String::new()]
            } else {
                wrap(line, &options)
                    .into_iter()
                    .map(|cow| cow.into_owned())
                    .collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_cache_bounds_messages() {
        let mut cache = ViewportCache::new();

        for i in 0..50 {
            cache.push_message(CachedMessage::new(
                format!("msg-{}", i),
                Role::User,
                format!("Content {}", i),
            ));
        }

        assert!(cache.message_count() <= DEFAULT_MAX_CACHED_ITEMS);
        assert!(cache.get_content("msg-0").is_none());
        assert!(cache.get_content("msg-49").is_some());
    }

    #[test]
    fn viewport_cache_streaming_flow() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("Hello ");
        cache.append_streaming("World");
        cache.complete_streaming("msg-1".to_string(), Role::Assistant);

        assert_eq!(cache.get_content("msg-1"), Some("Hello World"));
    }

    #[test]
    fn cached_message_wrapping() {
        let mut msg = CachedMessage::new(
            "test",
            Role::User,
            "This is a longer message that will need wrapping when displayed",
        );

        let lines_20 = msg.wrapped_lines(20);
        assert!(lines_20.len() > 1);

        let lines_80 = msg.wrapped_lines(80);
        assert_eq!(lines_80.len(), 1);
    }

    #[test]
    fn cached_message_wrap_cache_invalidation() {
        let mut msg = CachedMessage::new("test", Role::User, "Short content");

        let _ = msg.wrapped_lines(20);
        assert!(msg.wrapped.is_some());

        msg.invalidate_wrap();
        assert!(msg.wrapped.is_none());
    }

    #[test]
    fn viewport_cache_streaming_content_accessible() {
        let mut cache = ViewportCache::new();

        assert!(!cache.is_streaming());
        cache.start_streaming();
        assert!(cache.is_streaming());

        cache.append_streaming("partial");
        assert_eq!(cache.streaming_content(), Some("partial"));
    }

    #[test]
    fn viewport_cache_cancel_streaming() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("will be discarded");
        cache.cancel_streaming();

        assert!(!cache.is_streaming());
        assert!(cache.streaming_content().is_none());
    }

    #[test]
    fn viewport_anchor_operations() {
        let mut cache = ViewportCache::new();

        assert!(cache.anchor().is_none());

        cache.set_anchor(ViewportAnchor {
            message_id: "msg-5".to_string(),
            line_offset: 3,
        });

        let anchor = cache.anchor().unwrap();
        assert_eq!(anchor.message_id, "msg-5");
        assert_eq!(anchor.line_offset, 3);

        cache.clear_anchor();
        assert!(cache.anchor().is_none());
    }

    #[test]
    fn wrap_content_preserves_empty_lines() {
        let wrapped = wrap_content("line1\n\nline3", 80);
        assert_eq!(wrapped, vec!["line1", "", "line3"]);
    }

    #[test]
    fn wrap_content_handles_long_lines() {
        let wrapped = wrap_content("word word word word", 10);
        assert!(wrapped.len() > 1);
    }

    #[test]
    fn streaming_buffer_length() {
        let mut buf = StreamingBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);

        buf.append("hello");
        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn viewport_cache_clear() {
        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "content"));
        cache.start_streaming();
        cache.set_anchor(ViewportAnchor {
            message_id: "msg-1".to_string(),
            line_offset: 0,
        });

        cache.clear();

        assert_eq!(cache.message_count(), 0);
        assert!(!cache.is_streaming());
        assert!(cache.anchor().is_none());
    }

    #[test]
    fn viewport_cache_invalidate_all_wraps() {
        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "content 1"));
        cache.push_message(CachedMessage::new("msg-2", Role::User, "content 2"));

        if let Some(msg) = cache.get_message_mut("msg-1") {
            let _ = msg.wrapped_lines(80);
        }
        if let Some(msg) = cache.get_message_mut("msg-2") {
            let _ = msg.wrapped_lines(80);
        }

        cache.invalidate_all_wraps();

        for msg in cache.messages() {
            assert!(msg.wrapped.is_none());
        }
    }

    #[test]
    fn cached_message_content_is_arc() {
        let msg = CachedMessage::new("test", Role::User, "content");
        let cloned = msg.clone();

        assert!(Arc::ptr_eq(
            &(msg.content as Arc<str>),
            &(cloned.content as Arc<str>)
        ));
    }

    #[test]
    fn viewport_cache_implements_content_source() {
        use crucible_oil::{Compositor, Style};

        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "Hello World"));
        cache.push_message(CachedMessage::new("msg-2", Role::Assistant, "Response"));

        let mut comp = Compositor::new(&cache, 80);
        assert!(comp.render_message("msg-1", Style::new()));
        assert!(comp.render_message("msg-2", Style::new().bold()));
        assert!(!comp.render_message("nonexistent", Style::new()));

        let lines = comp.finish();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].text, "Hello World");
        assert_eq!(lines[1].spans[0].text, "Response");
    }

    #[test]
    fn compositor_with_viewport_cache_multiline() {
        use crucible_oil::{Compositor, Style};

        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new(
            "msg-1",
            Role::User,
            "Line 1\nLine 2\nLine 3",
        ));

        let mut comp = Compositor::new(&cache, 80);
        comp.render_message("msg-1", Style::new());

        let lines = comp.finish();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn resize_preserves_anchor() {
        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new(
            "msg-1",
            Role::User,
            "A long message that will wrap at 80 columns but differently at 40",
        ));

        cache.set_anchor(ViewportAnchor {
            message_id: "msg-1".to_string(),
            line_offset: 0,
        });

        cache.invalidate_all_wraps();

        assert!(cache.anchor().is_some());
        assert_eq!(cache.anchor().unwrap().message_id, "msg-1");
    }

    #[test]
    fn resize_invalidates_wrapping() {
        let mut cache = ViewportCache::new();
        let mut msg = CachedMessage::new("test", Role::User, "Content that wraps");

        let _ = msg.wrapped_lines(80);
        assert!(msg.wrapped.is_some());

        cache.push_message(msg);
        cache.invalidate_all_wraps();

        for msg in cache.messages() {
            assert!(msg.wrapped.is_none());
        }
    }

    #[test]
    fn line_buffer_resize_with_anchor_workflow() {
        use crucible_oil::LineBuffer;

        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "Message 1"));
        cache.push_message(CachedMessage::new("msg-2", Role::User, "Message 2"));
        cache.push_message(CachedMessage::new("msg-3", Role::User, "Message 3"));

        let mut line_buffer = LineBuffer::new(80, 24);
        let mut prev_line_buffer = LineBuffer::new(80, 24);

        cache.set_anchor(ViewportAnchor {
            message_id: "msg-2".to_string(),
            line_offset: 0,
        });

        line_buffer.resize(40, 12);
        prev_line_buffer.resize(40, 12);
        cache.invalidate_all_wraps();

        assert_eq!(line_buffer.width(), 40);
        assert_eq!(line_buffer.capacity(), 11);
        assert!(cache.anchor().is_some());
        assert_eq!(cache.anchor().unwrap().message_id, "msg-2");
    }

    #[test]
    fn anchor_workflow_for_resize() {
        let mut cache = ViewportCache::new();

        for i in 0..10 {
            cache.push_message(CachedMessage::new(
                format!("msg-{}", i),
                Role::User,
                format!("Content for message {}", i),
            ));
        }

        cache.set_anchor(ViewportAnchor {
            message_id: "msg-5".to_string(),
            line_offset: 2,
        });

        cache.invalidate_all_wraps();

        let anchor = cache.anchor().unwrap();
        assert_eq!(anchor.message_id, "msg-5");
        assert_eq!(anchor.line_offset, 2);

        assert!(cache.get_content("msg-5").is_some());
    }

    #[test]
    fn tool_call_creation_and_streaming() {
        let mut cache = ViewportCache::new();

        cache.push_tool_call("tool-1".to_string(), "read_file", r#"{"path":"test.rs"}"#);
        assert_eq!(cache.item_count(), 1);

        let tool = cache.find_tool_mut("read_file").unwrap();
        assert!(!tool.complete);
        assert!(tool.result.is_empty());

        cache.append_tool_result("read_file", "line 1\n");
        cache.append_tool_result("read_file", "line 2\n");

        let tool = cache.find_tool_mut("read_file").unwrap();
        assert_eq!(tool.result, "line 1\nline 2\n");

        cache.complete_tool("read_file");
        let tool = cache.find_tool_mut("read_file").unwrap();
        assert!(tool.complete);
    }

    #[test]
    fn tool_call_arc_sharing() {
        let tool = CachedToolCall::new("t1", "read_file", r#"{"path":"test.rs"}"#);
        let cloned = tool.clone();

        assert!(Arc::ptr_eq(&tool.name, &cloned.name));
        assert!(Arc::ptr_eq(&tool.args, &cloned.args));
    }

    #[test]
    fn shell_execution_creation() {
        let mut cache = ViewportCache::new();

        cache.push_shell_execution(
            "shell-1".to_string(),
            "ls -la",
            0,
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
            Some(PathBuf::from("/tmp/output.txt")),
        );

        assert_eq!(cache.item_count(), 1);
        let item = cache.get_item("shell-1").unwrap();
        let shell = item.as_shell_execution().unwrap();
        assert_eq!(shell.command.as_ref(), "ls -la");
        assert_eq!(shell.exit_code, 0);
        assert_eq!(shell.output_tail.len(), 2);
    }

    #[test]
    fn mixed_item_types() {
        let mut cache = ViewportCache::new();

        cache.push_message(CachedMessage::new("msg-1", Role::User, "Hello"));
        cache.push_tool_call("tool-1".to_string(), "search", "{}");
        cache.push_message(CachedMessage::new("msg-2", Role::Assistant, "Response"));
        cache.push_shell_execution("shell-1".to_string(), "pwd", 0, vec![], None);

        assert_eq!(cache.item_count(), 4);
        assert_eq!(cache.message_count(), 2);

        let items: Vec<_> = cache.items().collect();
        assert!(items[0].as_message().is_some());
        assert!(items[1].as_tool_call().is_some());
        assert!(items[2].as_message().is_some());
        assert!(items[3].as_shell_execution().is_some());
    }

    #[test]
    fn cached_chat_item_id() {
        let msg = CachedChatItem::Message(CachedMessage::new("msg-1", Role::User, "test"));
        let tool = CachedChatItem::ToolCall(CachedToolCall::new("tool-1", "test", "{}"));
        let shell = CachedChatItem::ShellExecution(CachedShellExecution::new(
            "shell-1",
            "ls",
            0,
            vec![],
            None,
        ));

        assert_eq!(msg.id(), "msg-1");
        assert_eq!(tool.id(), "tool-1");
        assert_eq!(shell.id(), "shell-1");
    }

    #[test]
    fn content_source_only_returns_message_content() {
        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "Hello"));
        cache.push_tool_call("tool-1".to_string(), "test", "{}");

        assert_eq!(cache.get_content("msg-1"), Some("Hello"));
        assert_eq!(cache.get_content("tool-1"), None);
    }

    #[test]
    fn item_count_vs_message_count() {
        let mut cache = ViewportCache::new();
        cache.push_message(CachedMessage::new("msg-1", Role::User, "Hello"));
        cache.push_tool_call("tool-1".to_string(), "test", "{}");
        cache.push_tool_call("tool-2".to_string(), "test2", "{}");

        assert_eq!(cache.item_count(), 3);
        assert_eq!(cache.message_count(), 1);
    }

    #[test]
    fn find_tool_mut_finds_most_recent() {
        let mut cache = ViewportCache::new();
        cache.push_tool_call("tool-1".to_string(), "read", "{}");
        cache.push_tool_call("tool-2".to_string(), "read", "{}");

        cache.append_tool_result("read", "result");

        let tool1 = cache.get_item("tool-1").unwrap().as_tool_call().unwrap();
        let tool2 = cache.get_item("tool-2").unwrap().as_tool_call().unwrap();

        assert!(tool1.result.is_empty());
        assert_eq!(tool2.result, "result");
    }

    #[test]
    fn streaming_graduation_with_blank_line() {
        let mut buf = StreamingBuffer::new();

        buf.append("First paragraph.\n\n");
        assert!(buf.has_graduated_content());
        assert_eq!(buf.graduated_content(), "First paragraph.\n\n");
        assert!(buf.in_progress_content().is_empty());

        buf.append("Second para");
        assert_eq!(buf.graduated_content(), "First paragraph.\n\n");
        assert_eq!(buf.in_progress_content(), "Second para");
    }

    #[test]
    fn streaming_graduation_no_graduation_without_blank_line() {
        let mut buf = StreamingBuffer::new();

        buf.append("Incomplete paragraph");
        assert!(!buf.has_graduated_content());
        assert_eq!(buf.in_progress_content(), "Incomplete paragraph");
    }

    #[test]
    fn streaming_graduation_preserves_code_blocks() {
        let mut buf = StreamingBuffer::new();

        buf.append("```rust\nfn main() {\n\n    println!(\"hello\");\n}\n```\n\n");
        assert!(buf.has_graduated_content());
        assert_eq!(
            buf.graduated_content(),
            "```rust\nfn main() {\n\n    println!(\"hello\");\n}\n```\n\n"
        );
    }

    #[test]
    fn streaming_graduation_blocks_unclosed_code_fence() {
        let mut buf = StreamingBuffer::new();

        buf.append("Text\n\n```rust\ncode here\n\nmore code");
        assert!(!buf.has_graduated_content());
        assert_eq!(
            buf.in_progress_content(),
            "Text\n\n```rust\ncode here\n\nmore code"
        );
    }

    #[test]
    fn streaming_graduation_multiple_blocks() {
        let mut buf = StreamingBuffer::new();

        buf.append("Block 1\n\nBlock 2\n\nBlock ");
        assert!(buf.has_graduated_content());
        assert_eq!(buf.graduated_content(), "Block 1\n\nBlock 2\n\n");
        assert_eq!(buf.in_progress_content(), "Block ");
        assert_eq!(buf.graduated_block_count(), 1);
    }

    #[test]
    fn streaming_all_content_combines_graduated_and_in_progress() {
        let mut buf = StreamingBuffer::new();

        buf.append("Graduated\n\nIn progress");
        assert_eq!(buf.all_content(), "Graduated\n\nIn progress");
    }

    #[test]
    fn streaming_into_content_combines_all() {
        let mut buf = StreamingBuffer::new();
        buf.append("Part 1\n\nPart 2");

        let content = buf.into_content();
        assert_eq!(content, "Part 1\n\nPart 2");
    }

    #[test]
    fn viewport_cache_streaming_graduation_methods() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("First block\n\nSecond ");

        assert!(cache.has_streaming_graduated_content());
        assert_eq!(cache.streaming_graduated_content(), Some("First block\n\n"));
        assert_eq!(cache.streaming_in_progress_content(), Some("Second "));
        assert_eq!(cache.streaming_graduated_block_count(), 1);
    }
}
