use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A named kiln entry in global config.
///
/// Supports shorthand (just a path string) and full form (table with options).
/// Shorthand: `vault = "~/vault"`
/// Full: `[kilns.work]\npath = "~/work/notes"\nlazy = true`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KilnEntry {
    /// Shorthand: just a path string.
    Path(PathBuf),
    /// Full form: table with path and optional lazy flag.
    Config {
        /// Filesystem path to the kiln root.
        path: PathBuf,
        /// If true, kiln is not opened until explicitly requested.
        #[serde(default)]
        lazy: bool,
    },
}

impl KilnEntry {
    /// Returns the filesystem path for this kiln entry.
    pub fn path(&self) -> PathBuf {
        match self {
            KilnEntry::Path(p) => p.clone(),
            KilnEntry::Config { path, .. } => path.clone(),
        }
    }

    /// Returns whether this kiln should be lazily opened.
    pub fn lazy(&self) -> bool {
        match self {
            KilnEntry::Path(_) => false,
            KilnEntry::Config { lazy, .. } => *lazy,
        }
    }
}

/// A registered project in global config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Filesystem path to the project root.
    pub path: PathBuf,
    /// Named kilns this project uses (resolved from `[kilns]` section).
    #[serde(default)]
    pub kilns: Vec<String>,
    /// Which kiln is primary (session storage, tool default).
    #[serde(default)]
    pub default_kiln: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn kiln_entry_shorthand_deserializes_from_string() {
        let toml_str = r#"vault = "~/vault""#;
        let map: std::collections::HashMap<String, KilnEntry> = toml::from_str(toml_str).unwrap();
        assert_eq!(map["vault"].path(), PathBuf::from("~/vault"));
        assert!(!map["vault"].lazy());
    }

    #[test]
    fn kiln_entry_full_deserializes_from_table() {
        let toml_str = r#"
[work]
path = "~/work/notes"
lazy = true
"#;
        let map: std::collections::HashMap<String, KilnEntry> = toml::from_str(toml_str).unwrap();
        assert_eq!(map["work"].path(), PathBuf::from("~/work/notes"));
        assert!(map["work"].lazy());
    }

    #[test]
    fn project_entry_deserializes() {
        let toml_str = r#"
[crucible]
path = "~/crucible"
kilns = ["docs", "vault"]
default_kiln = "vault"
"#;
        let map: std::collections::HashMap<String, ProjectEntry> = toml::from_str(toml_str).unwrap();
        let entry = &map["crucible"];
        assert_eq!(entry.path, PathBuf::from("~/crucible"));
        assert_eq!(entry.kilns, vec!["docs", "vault"]);
        assert_eq!(entry.default_kiln.as_deref(), Some("vault"));
    }

    #[test]
    fn kiln_entry_roundtrips_through_toml() {
        // TOML requires a table at the root, so roundtrip through a map
        // (matches real usage: `[kilns]` is always a table in config)
        let mut map = std::collections::HashMap::new();
        map.insert("vault".to_string(), KilnEntry::Path(PathBuf::from("~/vault")));
        let serialized = toml::to_string(&map).unwrap();
        let deserialized: std::collections::HashMap<String, KilnEntry> =
            toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized["vault"].path(), PathBuf::from("~/vault"));
    }
}
