//! Types representing justfile structure from `just --dump --dump-format json`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Justfile {
    pub aliases: HashMap<String, Alias>,
    pub assignments: HashMap<String, Assignment>,
    pub first: String,
    pub doc: Option<String>,
    pub groups: Vec<String>,
    pub modules: HashMap<String, serde_json::Value>,
    pub recipes: HashMap<String, Recipe>,
    pub settings: Settings,
    pub source: String,
    pub unexports: Vec<String>,
    pub warnings: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub attributes: Vec<serde_json::Value>,
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub export: bool,
    pub name: String,
    pub value: Vec<Fragment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub attributes: Vec<serde_json::Value>,
    pub body: Vec<Vec<Fragment>>,
    pub dependencies: Vec<Dependency>,
    pub doc: Option<String>,
    pub name: String,
    pub namepath: String,
    pub parameters: Vec<Parameter>,
    pub priors: u32,
    pub private: bool,
    pub quiet: bool,
    pub shebang: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Fragment {
    Text(String),
    Variable(Vec<Vec<String>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub arguments: Vec<serde_json::Value>,
    pub recipe: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub default: Option<serde_json::Value>,
    pub export: bool,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub allow_duplicate_recipes: bool,
    pub allow_duplicate_variables: bool,
    pub dotenv_filename: Option<String>,
    pub dotenv_load: bool,
    pub dotenv_override: bool,
    pub dotenv_path: Option<String>,
    pub dotenv_required: bool,
    pub export: bool,
    pub fallback: bool,
    pub ignore_comments: bool,
    pub no_exit_message: bool,
    pub positional_arguments: bool,
    pub quiet: bool,
    pub shell: Option<serde_json::Value>,
    pub tempdir: Option<String>,
    pub unstable: bool,
    pub windows_powershell: bool,
    pub windows_shell: Option<serde_json::Value>,
    pub working_directory: Option<String>,
}

impl Justfile {
    /// Parse justfile JSON from `just --dump --dump-format json`
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Get public recipes (non-private)
    pub fn public_recipes(&self) -> Vec<&Recipe> {
        self.recipes.values().filter(|r| !r.private).collect()
    }
}

impl Recipe {
    /// Generate signature string like "deploy ENV *ARGS"
    pub fn signature(&self) -> String {
        if self.parameters.is_empty() {
            self.name.clone()
        } else {
            let params: Vec<String> = self
                .parameters
                .iter()
                .map(|p| match p.kind.as_str() {
                    "star" => format!("*{}", p.name),
                    "plus" => format!("+{}", p.name),
                    _ => p.name.clone(),
                })
                .collect();
            format!("{} {}", self.name, params.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSON: &str = r#"{
        "aliases": {},
        "assignments": {},
        "first": "default",
        "doc": null,
        "groups": [],
        "modules": {},
        "recipes": {
            "test": {
                "attributes": [],
                "body": [["cargo test"]],
                "dependencies": [],
                "doc": "Run all tests",
                "name": "test",
                "namepath": "test",
                "parameters": [],
                "priors": 0,
                "private": false,
                "quiet": false,
                "shebang": false
            },
            "test-crate": {
                "attributes": [],
                "body": [["cargo test -p ", [["variable", "CRATE"]]]],
                "dependencies": [],
                "doc": "Run tests for a specific crate",
                "name": "test-crate",
                "namepath": "test-crate",
                "parameters": [{"default": null, "export": false, "kind": "singular", "name": "CRATE"}],
                "priors": 0,
                "private": false,
                "quiet": false,
                "shebang": false
            }
        },
        "settings": {
            "allow_duplicate_recipes": false,
            "allow_duplicate_variables": false,
            "dotenv_filename": null,
            "dotenv_load": false,
            "dotenv_override": false,
            "dotenv_path": null,
            "dotenv_required": false,
            "export": false,
            "fallback": false,
            "ignore_comments": false,
            "no_exit_message": false,
            "positional_arguments": false,
            "quiet": false,
            "shell": null,
            "tempdir": null,
            "unstable": false,
            "windows_powershell": false,
            "windows_shell": null,
            "working_directory": null
        },
        "source": "justfile",
        "unexports": [],
        "warnings": []
    }"#;

    #[test]
    fn test_parse_justfile() {
        let jf = Justfile::from_json(SAMPLE_JSON).unwrap();
        assert_eq!(jf.first, "default");
        assert_eq!(jf.recipes.len(), 2);
    }

    #[test]
    fn test_recipe_signature() {
        let jf = Justfile::from_json(SAMPLE_JSON).unwrap();
        let recipe = jf.recipes.get("test-crate").unwrap();
        assert_eq!(recipe.signature(), "test-crate CRATE");
    }

    #[test]
    fn test_public_recipes() {
        let jf = Justfile::from_json(SAMPLE_JSON).unwrap();
        let public = jf.public_recipes();
        assert_eq!(public.len(), 2);
    }
}
