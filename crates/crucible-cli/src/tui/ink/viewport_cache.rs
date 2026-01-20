use std::collections::VecDeque;
use std::sync::Arc;
use textwrap::{wrap, Options, WordSplitter};

use super::chat_app::Role;

const MAX_CACHED_MESSAGES: usize = 32;

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

pub struct ViewportCache {
    messages: VecDeque<CachedMessage>,
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
        Self {
            messages: VecDeque::with_capacity(MAX_CACHED_MESSAGES),
            streaming: None,
            anchor: None,
        }
    }

    pub fn push_message(&mut self, msg: CachedMessage) {
        if self.messages.len() >= MAX_CACHED_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
    }

    pub fn get_content(&self, id: &str) -> Option<&str> {
        self.messages
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.content())
    }

    pub fn get_message(&self, id: &str) -> Option<&CachedMessage> {
        self.messages.iter().find(|m| m.id == id)
    }

    pub fn get_message_mut(&mut self, id: &str) -> Option<&mut CachedMessage> {
        self.messages.iter_mut().find(|m| m.id == id)
    }

    pub fn messages(&self) -> impl Iterator<Item = &CachedMessage> {
        self.messages.iter()
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
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
        for msg in &mut self.messages {
            msg.invalidate_wrap();
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.streaming = None;
        self.anchor = None;
    }
}

pub struct StreamingBuffer {
    content: String,
}

impl StreamingBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn into_content(self) -> String {
        self.content
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
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

        assert!(cache.message_count() <= MAX_CACHED_MESSAGES);
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
}
