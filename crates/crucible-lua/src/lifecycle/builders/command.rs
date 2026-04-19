use crate::annotations::{DiscoveredCommand, DiscoveredParam};

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    name: String,
    description: String,
    params: Vec<DiscoveredParam>,
    input_hint: Option<String>,
    source_path: String,
    handler_fn: String,
    is_fennel: bool,
}

impl CommandBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            handler_fn: name.clone(),
            name,
            description: String::new(),
            params: Vec::new(),
            input_hint: None,
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.input_hint = Some(hint.into());
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = handler.into();
        self
    }

    pub fn param(mut self, name: impl Into<String>, param_type: impl Into<String>) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            optional: false,
        });
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredCommand {
        DiscoveredCommand {
            name: self.name,
            description: self.description,
            params: self.params,
            input_hint: self.input_hint,
            source_path: self.source_path,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}
