use crate::annotations::DiscoveredView;

#[derive(Debug, Clone)]
pub struct ViewBuilder {
    name: String,
    description: String,
    source_path: String,
    view_fn: String,
    handler_fn: Option<String>,
    is_fennel: bool,
}

impl ViewBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            view_fn: name.clone(),
            name,
            description: String::new(),
            source_path: "<programmatic>".to_string(),
            handler_fn: None,
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn view_fn(mut self, view: impl Into<String>) -> Self {
        self.view_fn = view.into();
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = Some(handler.into());
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredView {
        DiscoveredView {
            name: self.name,
            description: self.description,
            source_path: self.source_path,
            view_fn: self.view_fn,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}
