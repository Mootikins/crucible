use crate::annotations::{DiscoveredParam, DiscoveredTool};

#[derive(Debug, Clone)]
pub struct ToolBuilder {
    name: String,
    description: String,
    params: Vec<DiscoveredParam>,
    return_type: Option<String>,
    source_path: String,
    is_fennel: bool,
}

impl ToolBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            params: Vec::new(),
            return_type: None,
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
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

    pub fn param_optional(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
    ) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            optional: true,
        });
        self
    }

    pub fn param_full(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
        optional: bool,
    ) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            optional,
        });
        self
    }

    pub fn returns(mut self, return_type: impl Into<String>) -> Self {
        self.return_type = Some(return_type.into());
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn fennel(mut self) -> Self {
        self.is_fennel = true;
        self
    }

    pub fn build(self) -> DiscoveredTool {
        DiscoveredTool {
            name: self.name,
            description: self.description,
            params: self.params,
            return_type: self.return_type,
            source_path: self.source_path,
            is_fennel: self.is_fennel,
        }
    }
}
