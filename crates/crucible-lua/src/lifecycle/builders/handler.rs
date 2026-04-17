use crate::annotations::DiscoveredHandler;

#[derive(Debug, Clone)]
pub struct HandlerBuilder {
    name: String,
    event_type: String,
    pattern: String,
    priority: i64,
    description: String,
    source_path: String,
    handler_fn: String,
    is_fennel: bool,
}

impl HandlerBuilder {
    pub fn new(name: impl Into<String>, event_type: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            handler_fn: name.clone(),
            name,
            event_type: event_type.into(),
            pattern: "*".to_string(),
            priority: 100,
            description: String::new(),
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }

    pub fn priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = handler.into();
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredHandler {
        DiscoveredHandler {
            name: self.name,
            event_type: self.event_type,
            pattern: self.pattern,
            priority: self.priority,
            description: self.description,
            source_path: self.source_path,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}
