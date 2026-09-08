//! `settings.json`: the machine-written config layer.
//!
//! Two authors write one store. A human writes `init.lua`; a machine — the
//! settings UI, and every `config.save` behind it — writes this file. The
//! permission prompt's "always allow" is NOT one of those machines: that grant
//! goes to the daemon's pattern store, under the whitelists directory.
//! Crucible owns it completely: it rewrites the whole file, sorts every key,
//! and says so in the file itself. A hand edit survives a rewrite, because
//! the rewrite merges over what the file already holds.
//!
//! The file lives beside `init.lua`, in the config root, and not under the
//! data root: `data_home` is itself a config key, so config that names the
//! data root cannot live inside it.
//!
//! The marker key is written first and read never. It is the one line of
//! documentation a person finds when they open the file by hand.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Map, Value};

use super::merge::deep_merge;

/// The file name, in the config root beside `init.lua`.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// The key that carries the file's own instructions.
///
/// `_` sorts before every config key, so the note is the first line a person
/// reads. The loader drops it: it is documentation, not a config leaf, and a
/// store that merged it would grow a `_` leaf with a `Settings` provenance
/// row.
const MARKER_KEY: &str = "_";

/// What the marker key says.
const MARKER_TEXT: &str = "Crucible owns this file and rewrites it whole, with sorted keys. \
     Hand edits survive, but prefer init.lua.";

/// Where the settings file sits for a given config root.
pub fn settings_path(config_root: &Path) -> PathBuf {
    config_root.join(SETTINGS_FILE_NAME)
}

/// Read the settings layer, or `None` when the file does not exist.
///
/// An absent file is the normal case — nothing has saved yet — and never an
/// error. A file that does not parse, or that holds something other than an
/// object, IS an error: silently ignoring it would drop every saved
/// preference with nothing to show for it.
pub fn load_settings(config_root: &Path) -> anyhow::Result<Option<Value>> {
    let path = settings_path(config_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let mut value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    let Value::Object(map) = &mut value else {
        anyhow::bail!("{} must hold a JSON object", path.display());
    };
    map.shift_remove(MARKER_KEY);
    Ok(Some(value))
}

/// Merge `delta` into the settings file and rewrite it whole.
///
/// `delta` is the part of a `config.save` that was accepted — never the whole
/// store. Writing the store back would record every `init.lua` leaf as a
/// `Settings` leaf, and at the next boot those leaves would load as settings
/// and outrank nothing: the refusal that protects a human's line would be
/// bypassed permanently, and silently.
///
/// The merge is [`deep_merge`], the same one the store layers with, so a
/// partial `chat` table keeps its siblings here exactly as it does there.
pub fn save_settings_delta(config_root: &Path, delta: Value) -> anyhow::Result<()> {
    let path = settings_path(config_root);
    let mut current = load_settings(config_root)?.unwrap_or_else(|| Value::Object(Map::new()));
    deep_merge(&mut current, delta);

    let mut out = Map::new();
    out.insert(
        MARKER_KEY.to_string(),
        Value::String(MARKER_TEXT.to_string()),
    );
    if let Value::Object(map) = sorted(current) {
        out.extend(map);
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(out))
        .context("serializing the settings file")?;
    text.push('\n');

    // `write_private` writes a sibling temporary file and renames it, so a
    // reader never sees a half-written file, and it creates the file
    // owner-only — this layer carries provider endpoints and permission
    // rules, which are nobody else's business on a shared machine.
    crate::fs::write_private(&path, text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

/// Rebuild every object with its keys in order.
///
/// `serde_json` runs with `preserve_order`, so a map keeps insertion order
/// and sorting has to be done rather than assumed. An array keeps its own
/// order: it is one value, and reordering it would change what it means.
fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, child)| (key, sorted(child)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn root() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("a temp config root")
    }

    /// Nothing has saved yet. That is the state every fresh install is in, so
    /// it cannot be an error.
    #[test]
    fn an_absent_file_loads_as_no_settings() {
        let dir = root();
        assert_eq!(load_settings(dir.path()).expect("absent is fine"), None);
    }

    #[test]
    fn a_saved_value_loads_back() {
        let dir = root();
        save_settings_delta(dir.path(), json!({"chat": {"model": "sonnet"}})).expect("save");
        assert_eq!(
            load_settings(dir.path()).expect("load"),
            Some(json!({"chat": {"model": "sonnet"}}))
        );
    }

    /// The file says what it is. A person who opens it by hand is told who
    /// owns it and which file to prefer.
    #[test]
    fn the_file_leads_with_its_own_instructions() {
        let dir = root();
        save_settings_delta(dir.path(), json!({"chat": {"model": "sonnet"}})).expect("save");
        let text = std::fs::read_to_string(settings_path(dir.path())).expect("read");
        let first_key = text
            .lines()
            .nth(1)
            .expect("the object opens on line 1, so the first key is on line 2");
        assert!(
            first_key.trim_start().starts_with("\"_\":"),
            "the marker must be the first key: {text}"
        );
        assert!(text.contains("init.lua"), "{text}");
    }

    /// The marker is documentation. A store that merged it would carry a `_`
    /// leaf, and `config.effective` would serve it to every client.
    #[test]
    fn the_loader_drops_the_marker_key() {
        let dir = root();
        std::fs::write(
            settings_path(dir.path()),
            r#"{"_": "a note", "chat": {"model": "sonnet"}}"#,
        )
        .expect("write");
        assert_eq!(
            load_settings(dir.path()).expect("load"),
            Some(json!({"chat": {"model": "sonnet"}}))
        );
    }

    /// Crucible rewrites the file whole, so the order it writes has to be the
    /// order a diff stays quiet in: sorted, at every depth.
    #[test]
    fn the_file_is_written_with_sorted_keys_at_every_depth() {
        let dir = root();
        save_settings_delta(
            dir.path(),
            json!({"zulu": 1, "alpha": {"zeta": 1, "beta": 2}}),
        )
        .expect("save");
        let text = std::fs::read_to_string(settings_path(dir.path())).expect("read");
        let order: Vec<&str> = ["\"zulu\"", "\"alpha\"", "\"zeta\"", "\"beta\""]
            .into_iter()
            .filter(|key| text.contains(key))
            .collect();
        let position = |needle: &str| text.find(needle).expect("the key was written");
        assert_eq!(order.len(), 4, "{text}");
        assert!(position("\"alpha\"") < position("\"zulu\""), "{text}");
        assert!(position("\"beta\"") < position("\"zeta\""), "{text}");
    }

    /// A save is a delta. The keys a previous save wrote are still there, and
    /// a partial table keeps the siblings it does not name.
    #[test]
    fn a_second_save_keeps_what_the_first_one_wrote() {
        let dir = root();
        save_settings_delta(
            dir.path(),
            json!({"chat": {"model": "sonnet", "show_diffs": true}}),
        )
        .expect("first save");
        save_settings_delta(dir.path(), json!({"chat": {"model": "opus"}})).expect("second save");
        assert_eq!(
            load_settings(dir.path()).expect("load"),
            Some(json!({"chat": {"model": "opus", "show_diffs": true}}))
        );
    }

    /// A hand edit is allowed, and the next save must not throw it away.
    #[test]
    fn a_hand_written_key_survives_the_next_save() {
        let dir = root();
        std::fs::write(settings_path(dir.path()), r#"{"default_kiln": "notes"}"#).expect("write");
        save_settings_delta(dir.path(), json!({"chat": {"model": "opus"}})).expect("save");
        assert_eq!(
            load_settings(dir.path()).expect("load"),
            Some(json!({"default_kiln": "notes", "chat": {"model": "opus"}}))
        );
    }

    /// A file that does not parse holds preferences nobody can read. Ignoring
    /// it would drop every one of them without a word.
    #[test]
    fn a_file_that_is_not_json_is_an_error_that_names_the_file() {
        let dir = root();
        std::fs::write(settings_path(dir.path()), "{ not json").expect("write");
        let error = load_settings(dir.path())
            .expect_err("a broken file must not read as no settings")
            .to_string();
        assert!(error.contains(SETTINGS_FILE_NAME), "{error}");
    }

    #[test]
    fn a_file_that_holds_no_object_is_an_error_that_names_the_file() {
        let dir = root();
        std::fs::write(settings_path(dir.path()), "[1, 2]").expect("write");
        let error = load_settings(dir.path())
            .expect_err("an array is not a config layer")
            .to_string();
        assert!(error.contains(SETTINGS_FILE_NAME), "{error}");
    }
}
