//! Schedule entries and plugin declarations from config.

use serde::{Deserialize, Serialize};

use crate::config::serde_helpers::default_true;

/// A declarative schedule entry from `[[schedules]]` in config.
///
/// Each entry runs a Lua snippet at a fixed interval via `cru.schedule`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleEntry {
    /// Human-readable name for logging.
    pub name: String,
    /// Interval string: "1h", "30m", "5s", "1d", or bare seconds.
    pub every: String,
    /// Lua code to execute, optionally prefixed with "lua:".
    pub action: String,
    /// Whether this schedule is active (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Parse a human-readable duration string like "1h", "30m", "5s", "1d".
///
/// Supports suffixes `d` (days), `h` (hours), `m` (minutes), `s` (seconds),
/// or a bare number treated as seconds.
pub fn parse_duration_string(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('d') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 86400));
    }
    if let Some(n) = s.strip_suffix('h') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 3600));
    }
    if let Some(n) = s.strip_suffix('m') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 60));
    }
    if let Some(n) = s.strip_suffix('s') {
        return n.parse::<u64>().ok().map(std::time::Duration::from_secs);
    }
    s.parse::<u64>().ok().map(std::time::Duration::from_secs)
}

#[cfg(test)]
mod duration_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parse_seconds_suffix() {
        assert_eq!(parse_duration_string("5s"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_minutes_suffix() {
        assert_eq!(
            parse_duration_string("30m"),
            Some(Duration::from_secs(1800))
        );
    }

    #[test]
    fn parse_hours_suffix() {
        assert_eq!(parse_duration_string("1h"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn parse_days_suffix() {
        assert_eq!(
            parse_duration_string("1d"),
            Some(Duration::from_secs(86400))
        );
    }

    #[test]
    fn parse_bare_number_as_seconds() {
        assert_eq!(parse_duration_string("120"), Some(Duration::from_secs(120)));
    }

    #[test]
    fn parse_with_whitespace() {
        assert_eq!(
            parse_duration_string("  2h  "),
            Some(Duration::from_secs(7200))
        );
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_duration_string("abc"), None);
        assert_eq!(parse_duration_string(""), None);
        assert_eq!(parse_duration_string("5x"), None);
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_duration_string("0s"), Some(Duration::from_secs(0)));
        assert_eq!(parse_duration_string("0"), Some(Duration::from_secs(0)));
    }
}

/// Standalone config for `~/.config/crucible/plugins.toml`.
///
/// This is NOT part of `Config` (crucible.toml). It lives in a separate file
/// so users can declare git-hosted plugins independently of the main config.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginsConfig {
    /// Declared plugins to bootstrap on daemon startup.
    #[serde(default)]
    pub plugin: Vec<PluginEntry>,
}

/// A single plugin declaration in `plugins.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginEntry {
    /// Git URL or GitHub shorthand (e.g. "user/repo").
    pub url: String,
    /// Branch to clone. Defaults to the repo's default branch.
    #[serde(default)]
    pub branch: Option<String>,
    /// Pin to a specific tag or commit hash after cloning.
    #[serde(default)]
    pub pin: Option<String>,
    /// Whether this plugin is enabled. Disabled plugins are skipped during bootstrap.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl PluginEntry {
    /// Derive the plugin's directory name from its URL — last path
    /// segment, sans trailing `.git`. Returns `None` if the result
    /// would be empty, `.`, or `..` (unsafe directory names).
    pub fn name(&self) -> Option<String> {
        plugin_name_from_url(&self.url)
    }
}

/// The reserved subkey of the `plugins` config table that holds the user's
/// plugin DECLARATIONS: `plugins.declare.<name>` is a git-hosted plugin to
/// bootstrap, while every other `plugins.<name>` is that plugin's options.
///
/// One constant, three consumers: `split_plugins_config` excludes it from
/// the option sections handed to `setup(cfg)`, plugin discovery refuses a
/// plugin actually named this, and [`declared_plugins`] reads it. A string
/// literal in any one of those is how the three drift.
pub const PLUGINS_DECLARE_KEY: &str = "declare";

/// Parse the `plugins.declare` table into named [`PluginEntry`]s.
///
/// Each entry is either a URL string (`reflection = "user/reflection"`) or a
/// `PluginEntry` table (`{ url = ..., branch = ..., pin = ..., enabled = ... }`).
/// Returns the entries plus one human-readable warning per entry that could
/// not be honoured — the caller logs them, because a silently dropped
/// declaration is a plugin that never loads with no visible reason.
///
/// The key must match the URL-derived directory name: the clone lands at the
/// URL's name, so a differing key would declare one name and produce
/// another. A mismatch is a warning and the entry is skipped.
pub fn declared_plugins(
    plugins: &std::collections::BTreeMap<String, serde_json::Value>,
) -> (Vec<(String, PluginEntry)>, Vec<String>) {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    let Some(declare) = plugins.get(PLUGINS_DECLARE_KEY) else {
        return (entries, warnings);
    };
    let Some(map) = declare.as_object() else {
        warnings.push(format!(
            "plugins.{PLUGINS_DECLARE_KEY} must be a table of name = url-or-entry; found {declare}"
        ));
        return (entries, warnings);
    };

    for (name, value) in map {
        let entry = match value {
            serde_json::Value::String(url) => PluginEntry {
                url: url.clone(),
                branch: None,
                pin: None,
                enabled: true,
            },
            serde_json::Value::Object(_) => {
                match serde_json::from_value::<PluginEntry>(value.clone()) {
                    Ok(entry) => entry,
                    Err(e) => {
                        warnings.push(format!(
                            "plugins.{PLUGINS_DECLARE_KEY}.{name} is not a plugin declaration \
                             (url, branch, pin, enabled): {e}"
                        ));
                        continue;
                    }
                }
            }
            other => {
                warnings.push(format!(
                    "plugins.{PLUGINS_DECLARE_KEY}.{name} must be a URL string or an entry \
                     table; found {other}"
                ));
                continue;
            }
        };

        match entry.name() {
            Some(derived) if derived == *name => entries.push((name.clone(), entry)),
            Some(derived) => warnings.push(format!(
                "plugins.{PLUGINS_DECLARE_KEY}.{name}: the URL '{}' names a plugin '{derived}', \
                 not '{name}'; rename the key or fix the URL (skipped)",
                entry.url
            )),
            None => warnings.push(format!(
                "plugins.{PLUGINS_DECLARE_KEY}.{name}: cannot derive a safe plugin name from \
                 URL '{}' (skipped)",
                entry.url
            )),
        }
    }

    (entries, warnings)
}

/// Extract a safe plugin directory name from a git URL.
///
/// Returns `None` when the derived name would be unsafe: empty, `.`,
/// `..`, starts with `-` (would be parsed as a CLI flag by tools we
/// later pass it to), or contains anything outside `[A-Za-z0-9._-]`.
/// The strict character set prevents log-spoofing, shell-quoting
/// hazards, and unsafe-path edge cases on filesystems that accept
/// odd characters.
pub fn plugin_name_from_url(url: &str) -> Option<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git")
        .to_string();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod declared_plugins_tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn plugins(declare: serde_json::Value) -> BTreeMap<String, serde_json::Value> {
        let mut map = BTreeMap::new();
        map.insert(PLUGINS_DECLARE_KEY.to_string(), declare);
        map.insert("reflection".to_string(), json!({ "model": "llama3.2" }));
        map
    }

    #[test]
    fn a_string_and_a_table_entry_both_declare() {
        let (entries, warnings) = declared_plugins(&plugins(json!({
            "greeter": "user/greeter",
            "review": { "url": "someone/review", "pin": "v1", "enabled": false },
        })));

        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(entries.len(), 2);
        let greeter = &entries.iter().find(|(n, _)| n == "greeter").unwrap().1;
        assert_eq!(greeter.url, "user/greeter");
        assert!(greeter.enabled);
        let review = &entries.iter().find(|(n, _)| n == "review").unwrap().1;
        assert_eq!(review.pin.as_deref(), Some("v1"));
        assert!(!review.enabled);
    }

    #[test]
    fn an_absent_declare_key_declares_nothing() {
        let mut map = BTreeMap::new();
        map.insert("reflection".to_string(), json!({ "model": "x" }));
        let (entries, warnings) = declared_plugins(&map);
        assert!(entries.is_empty());
        assert!(warnings.is_empty());
    }

    /// The clone lands at the URL-derived name, so a key that names something
    /// else would declare one plugin and produce another. Skipped, loudly.
    #[test]
    fn a_key_that_does_not_match_the_url_name_is_a_warning_not_an_entry() {
        let (entries, warnings) =
            declared_plugins(&plugins(json!({ "greeter": "user/other-name" })));
        assert!(entries.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("greeter") && warnings[0].contains("other-name"),
            "the warning must name both sides: {}",
            warnings[0]
        );
    }

    #[test]
    fn malformed_entries_warn_and_do_not_abort_the_rest() {
        let (entries, warnings) = declared_plugins(&plugins(json!({
            "greeter": "user/greeter",
            "broken": 7,
            "alsobad": { "pin": "v1" },
        })));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "greeter");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn a_non_table_declare_value_is_one_warning() {
        let (entries, warnings) = declared_plugins(&plugins(json!("user/repo")));
        assert!(entries.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains(PLUGINS_DECLARE_KEY));
    }
}
