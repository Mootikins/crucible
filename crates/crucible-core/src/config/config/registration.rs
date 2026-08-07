//! Writers that persist wizard/`cru init` answers into the global config.
//!
//! Split from `cli_app.rs` for size; these are the only functions that
//! *write* the global config file, and [`edit_config_in_place`] is the
//! contract they share: never destroy what the user hand-wrote.

use super::cli_app::CliAppConfig;

/// Register a project in the global config file.
///
/// Reads the existing config (or creates a default), inserts a `ProjectEntry`
/// under `[projects.<name>]`, and writes the file back with `toml::to_string_pretty`.
pub fn register_project_in_config(
    config_path: &std::path::Path,
    name: &str,
    project_path: &std::path::Path,
    kilns: &[&str],
    default_kiln: Option<&str>,
) -> anyhow::Result<()> {
    let mut config: CliAppConfig = if config_path.exists() {
        let contents = std::fs::read_to_string(config_path)?;
        toml::from_str(&contents)?
    } else {
        CliAppConfig::default()
    };

    config.projects.insert(
        name.to_string(),
        crate::config::config::registry::ProjectEntry {
            path: project_path.to_path_buf(),
            kilns: kilns.iter().map(|s| s.to_string()).collect(),
            default_kiln: default_kiln.map(|s| s.to_string()),
        },
    );

    let contents = toml::to_string_pretty(&config)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, contents)?;
    Ok(())
}

/// Persist a kiln path to the global config file.
///
/// The counterpart to [`register_project_in_config`]. Without this, a kiln
/// the user supplies at a prompt lives only in the in-memory config and the
/// prompt returns on every subsequent run.
///
/// Writes both `kiln_path` and a `[kilns]` entry: the former is what
/// `ensure_valid_kiln` checks directly, the latter is what `resolved_kilns`
/// and the daemon use. Keeping them in step here is what stops the two
/// halves of the config disagreeing about where the kiln is.
pub fn register_kiln_in_config(
    config_path: &std::path::Path,
    name: &str,
    kiln_path: &std::path::Path,
    make_default: bool,
) -> anyhow::Result<()> {
    // Absolute, always. This is the *global* config: a relative path like "."
    // — which is what `cru init` in the current directory hands us — would
    // resolve against whatever directory the next command runs from.
    let kiln_path = kiln_path
        .canonicalize()
        .unwrap_or_else(|_| kiln_path.to_path_buf());
    let kiln_str = kiln_path.to_string_lossy().to_string();

    edit_config_in_place(config_path, |doc| {
        doc["kiln_path"] = toml_edit::value(kiln_str.clone());
        if make_default || doc.get("default_kiln").is_none() {
            doc["default_kiln"] = toml_edit::value(name);
        }
        ensure_table(doc.as_table_mut(), "kilns").insert(name, toml_edit::value(kiln_str.clone()));
    })
}

/// Get or create a child table, as a real `[section]` rather than an inline
/// one.
///
/// Indexing a `DocumentMut` into a missing key materialises an *inline* table,
/// so a fresh config would come out as `kilns = { a = "…" }` — valid TOML, but
/// unreadable in a file people hand-edit, and unlike every other section in
/// the shipped example config.
/// Returns a `TableLike` so an existing *inline* table keeps the user's chosen
/// style — rewriting it would be gratuitous churn in their file — while a
/// missing one is created as a real section.
fn ensure_table<'a>(
    parent: &'a mut toml_edit::Table,
    key: &str,
) -> &'a mut dyn toml_edit::TableLike {
    parent
        .entry(key)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .expect("config section must be a table")
}

/// Read, mutate, and write a config file **without** destroying it.
///
/// A serde round-trip (`from_str` → mutate → `to_string_pretty`) silently
/// drops every comment and every key the struct does not model, and freezes
/// all defaults into the file. That is unacceptable for a file a user
/// hand-edits — and it is not hypothetical: an early version of this feature
/// rewrote a working `~/.config/crucible/config.toml`, losing its comments and
/// leaving junk entries behind.
///
/// `toml_edit` preserves formatting, comments, and unknown keys, so we only
/// touch the keys we mean to.
///
/// Note `register_project_in_config` still uses the serde round-trip. It has
/// the same hazard, and predates this helper.
fn edit_config_in_place(
    config_path: &std::path::Path,
    edit: impl FnOnce(&mut toml_edit::DocumentMut),
) -> anyhow::Result<()> {
    let mut doc: toml_edit::DocumentMut = if config_path.exists() {
        std::fs::read_to_string(config_path)?.parse()?
    } else {
        toml_edit::DocumentMut::new()
    };

    edit(&mut doc);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, doc.to_string())?;
    Ok(())
}

/// Persist an LLM provider selection to the global config file.
///
/// `cru init` and the setup wizard both ask which provider to use. Writing
/// that answer only into the kiln directory meant it was never read — the
/// user's choice was displayed back to them and then ignored.
///
/// Registers the provider under `[llm.providers.<name>]` and makes it the
/// default when no default is set.
pub fn register_llm_provider_in_config(
    config_path: &std::path::Path,
    provider: &str,
    model: &str,
) -> anyhow::Result<()> {
    use crate::config::BackendType;

    // Validate before touching the file: an unknown provider should fail
    // rather than write a config the loader will reject.
    let provider_type: BackendType = provider
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown provider type: {provider}"))?;
    let type_str = provider_type.as_str().to_string();

    edit_config_in_place(config_path, |doc| {
        let llm = ensure_table(doc.as_table_mut(), "llm");
        if llm.get("default").is_none() {
            llm.insert("default", toml_edit::value(provider));
        }
        let providers = llm
            .entry("providers")
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_like_mut()
            .expect("[llm.providers] must be a table");
        let entry = providers
            .entry(provider)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_like_mut()
            .expect("a provider entry must be a table");
        entry.insert("type", toml_edit::value(type_str.clone()));
        entry.insert("default_model", toml_edit::value(model));
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_project_writes_to_config_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "kiln_path = \"~/vault\"\n\n[kilns]\nvault = \"~/vault\"\n",
        )
        .unwrap();

        register_project_in_config(
            &config_path,
            "myproject",
            tmp.path(),
            &["vault"],
            Some("vault"),
        )
        .unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects["myproject"].kilns, vec!["vault"]);
        assert_eq!(
            config.projects["myproject"].default_kiln.as_deref(),
            Some("vault")
        );
        assert_eq!(config.projects["myproject"].path, tmp.path().to_path_buf());
    }

    /// A kiln the user names at a prompt has to survive the process, or the
    /// prompt fires again on the next run — forever.
    #[test]
    fn a_registered_kiln_survives_a_config_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let kiln = tmp.path().join("my-kiln");

        register_kiln_in_config(&config_path, "default", &kiln, true).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();

        // Both halves must agree: `kiln_path` is what the preflight check
        // reads, `[kilns]` is what the daemon and `resolved_kilns` use.
        assert_eq!(config.kiln_path, kiln);
        assert_eq!(config.default_kiln.as_deref(), Some("default"));
        assert!(config.kilns.contains_key("default"));
        assert_eq!(
            config.resolved_kilns()["default"],
            crate::config::config::registry::KilnEntry::Path(kiln)
        );
    }

    /// The global config is read from arbitrary working directories, so a
    /// relative path stored in it points somewhere different every time.
    /// `cru init` with no argument hands us exactly that: ".".
    ///
    /// `#[serial]` because the cwd is process-global, exactly like the env:
    /// changing it under a parallel run corrupts unrelated tests.
    #[test]
    #[serial_test::serial]
    fn a_registered_kiln_path_is_stored_absolute() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let kiln = tmp.path().join("relative-kiln");
        std::fs::create_dir_all(&kiln).unwrap();

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&kiln).unwrap();
        let result = register_kiln_in_config(&config_path, "here", std::path::Path::new("."), true);
        std::env::set_current_dir(previous).unwrap();
        result.unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert!(
            config.kiln_path.is_absolute(),
            "stored kiln path must be absolute, got {}",
            config.kiln_path.display()
        );
    }

    /// The global config is hand-edited. Rewriting it through serde drops
    /// every comment and every key the struct does not model — which is how a
    /// working config got mangled while this feature was being built.
    #[test]
    fn registering_a_kiln_preserves_comments_and_unknown_keys() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "# my careful notes\nkiln_path = \"/old\"\n\n\
             [llm]\ndefault = \"zai-coding\"\n\n\
             [some_future_section]\nkey = \"value\"\n",
        )
        .unwrap();

        register_kiln_in_config(&config_path, "new", tmp.path(), false).unwrap();

        let after = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            after.contains("# my careful notes"),
            "comments must survive: {after}"
        );
        assert!(
            after.contains("[some_future_section]"),
            "unknown sections must survive: {after}"
        );
        assert!(
            after.contains("zai-coding"),
            "unrelated settings must survive: {after}"
        );
        // And the edit actually landed. Assert on the parsed value, not the
        // text: toml_edit may write an inline table (`kilns = { new = ... }`)
        // rather than a `[kilns]` section, which is the same config.
        let parsed: CliAppConfig = toml::from_str(&after).unwrap();
        assert!(parsed.kilns.contains_key("new"));
        assert_eq!(parsed.kiln_path, tmp.path().canonicalize().unwrap());
    }

    /// Same guarantee for the provider writer.
    #[test]
    fn registering_a_provider_preserves_comments_and_does_not_steal_the_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "# keep me\n[llm]\ndefault = \"existing\"\n\n\
             [llm.providers.existing]\ntype = \"ollama\"\n",
        )
        .unwrap();

        register_llm_provider_in_config(&config_path, "anthropic", "claude-x").unwrap();

        let after = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            after.contains("# keep me"),
            "comments must survive: {after}"
        );
        assert!(
            after.contains("existing"),
            "the pre-existing provider must survive: {after}"
        );

        let parsed: CliAppConfig = toml::from_str(&after).unwrap();
        assert_eq!(
            parsed.llm.default.as_deref(),
            Some("existing"),
            "an explicit default must not be overwritten"
        );
        assert_eq!(
            parsed.llm.providers["anthropic"].default_model.as_deref(),
            Some("claude-x")
        );
    }

    /// An unknown provider must fail before the file is touched.
    #[test]
    fn registering_an_unknown_provider_leaves_the_file_untouched() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "# original\n").unwrap();

        assert!(register_llm_provider_in_config(&config_path, "not-a-provider", "m").is_err());
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            "# original\n"
        );
    }

    /// Registering a second kiln must not silently steal the default.
    #[test]
    fn registering_a_second_kiln_leaves_the_existing_default_alone() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        register_kiln_in_config(&config_path, "first", &tmp.path().join("a"), true).unwrap();
        register_kiln_in_config(&config_path, "second", &tmp.path().join("b"), false).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.default_kiln.as_deref(), Some("first"));
        assert_eq!(config.kilns.len(), 2);
    }

    #[test]
    fn register_project_creates_config_if_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("subdir").join("config.toml");

        register_project_in_config(&config_path, "newproj", tmp.path(), &[], None).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert!(config.projects.contains_key("newproj"));
        assert!(config.projects["newproj"].kilns.is_empty());
        assert!(config.projects["newproj"].default_kiln.is_none());
    }

    #[test]
    fn register_project_preserves_existing_projects() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "kiln_path = \"~/vault\"\n").unwrap();

        register_project_in_config(&config_path, "proj1", tmp.path(), &["vault"], None).unwrap();
        register_project_in_config(
            &config_path,
            "proj2",
            &tmp.path().join("other"),
            &["docs"],
            Some("docs"),
        )
        .unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 2);
        assert!(config.projects.contains_key("proj1"));
        assert!(config.projects.contains_key("proj2"));
    }
}
