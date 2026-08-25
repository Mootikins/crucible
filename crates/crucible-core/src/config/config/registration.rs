//! Writers that persist wizard/`cru init` answers into the global config.
//!
//! Split from `cli_app.rs` for size; these are the only functions that
//! *write* the global config file, and [`edit_config_in_place`] is the
//! contract they share: never destroy what the user hand-wrote.

/// Add one `[kilns]` entry, and change nothing else.
///
/// The last config writer. It is called when a `--kiln <path>` names a
/// directory that has no entry yet — which says nothing about which kiln every
/// *future* command should use, so it never claims `default_kiln`.
///
/// The wizard's writer used to live beside this one and also set `kiln_path`.
/// It is gone: the wizard's answer is a registration, and registrations belong
/// to the daemon.
///
/// `auto` records that Crucible wrote the entry rather than the user, and is
/// the difference between the two shapes written here: an entry the user named
/// gets the shorthand every hand-written config uses, an entry we derived gets
/// the table form carrying the marker.
///
/// **Never re-points an existing name.** The registry checks this first and
/// against a better answer (it compares resolved paths, this compares the text
/// in the file), but the file is what outlives the process and may have changed
/// since the registry read it — a second `cru` running concurrently, or a hand
/// edit between the two steps. Re-pointing a name is how a session that
/// persisted `notes` yesterday opens a different corpus today, so the fail-
/// closed answer belongs at both layers.
pub fn register_kiln_entry_in_config(
    config_path: &std::path::Path,
    name: &str,
    kiln_path: &std::path::Path,
    auto: bool,
) -> anyhow::Result<()> {
    // The global config is read from arbitrary working directories, so a
    // relative entry points somewhere different on every invocation. Callers
    // absolutize through the kiln registry, which is also where the floor runs;
    // a relative path arriving here means that step was skipped.
    anyhow::ensure!(
        kiln_path.is_absolute(),
        "refusing to register kiln '{name}' at the relative path '{}': \
         a `[kilns]` entry must be absolute",
        kiln_path.display()
    );
    let kiln_str = kiln_path.to_string_lossy().to_string();

    edit_config_in_place(config_path, |doc| {
        let kilns = ensure_table(doc.as_table_mut(), "kilns");

        if let Some(existing) = kilns.get(name) {
            let existing_path = existing
                .as_str()
                .map(str::to_string)
                .or_else(|| {
                    existing
                        .as_table_like()
                        .and_then(|t| t.get("path"))
                        .and_then(|p| p.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            anyhow::ensure!(
                existing_path == kiln_str,
                "the kiln name '{name}' is already registered to '{existing_path}' in {}. \
                 Choose another name, or remove that entry first.",
                config_path.display()
            );
            return Ok(());
        }

        kilns.insert(
            name,
            if auto {
                let mut entry = toml_edit::Table::new();
                entry.insert("path", toml_edit::value(kiln_str.clone()));
                entry.insert("auto", toml_edit::value(true));
                toml_edit::Item::Table(entry)
            } else {
                toml_edit::value(kiln_str.clone())
            },
        );
        Ok(())
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
/// The edit returns a `Result` so an edit that decides it must not proceed —
/// [`register_kiln_entry_in_config`] finding the name already claimed —
/// leaves the file untouched rather than writing a half-applied document.
///
/// Note `register_project_in_config` still uses the serde round-trip. It has
/// the same hazard, and predates this helper.
fn edit_config_in_place(
    config_path: &std::path::Path,
    edit: impl FnOnce(&mut toml_edit::DocumentMut) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut doc: toml_edit::DocumentMut = if config_path.exists() {
        std::fs::read_to_string(config_path)?.parse()?
    } else {
        toml_edit::DocumentMut::new()
    };

    edit(&mut doc)?;

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CliAppConfig;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// `--kiln <path>` says "attach this corpus to this session", not "make it
    /// the kiln every future command uses", so it adds one `[kilns]` entry and
    /// touches nothing else — `kiln_path` and `default_kiln` are the user's
    /// answers to a different question.
    #[test]
    fn an_auto_registered_kiln_adds_an_entry_without_claiming_the_default() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "kiln_path = \"/the/users/vault\"\ndefault_kiln = \"vault\"\n\n\
             [kilns]\nvault = \"/the/users/vault\"\n",
        )
        .unwrap();
        let notes = tmp.path().join("notes");

        register_kiln_entry_in_config(&config_path, "notes", &notes, true).unwrap();

        let config: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(config.kilns["notes"].path(), notes);
        assert_eq!(
            config.kiln_path,
            PathBuf::from("/the/users/vault"),
            "auto-registration must not repoint the configured kiln"
        );
        assert_eq!(
            config.default_kiln.as_deref(),
            Some("vault"),
            "auto-registration must not steal the default"
        );
        assert!(
            config.kilns.contains_key("vault"),
            "the existing entry must survive"
        );
    }

    /// The `auto` marker survives a read. It exists so the human can tell an
    /// entry Crucible wrote from one they typed; an `auto` that vanishes on
    /// the next write says the opposite.
    #[test]
    fn the_auto_marker_survives_a_config_round_trip() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let notes = tmp.path().join("notes");

        register_kiln_entry_in_config(&config_path, "notes", &notes, true).unwrap();
        // Driven through the serde round-trip directly. There used to be a
        // WRITER that round-tripped the whole document through `CliAppConfig`
        // — `register_project_in_config` — and it was the thing that erased
        // what the struct does not model. That writer is gone, so the round
        // trip is spelled here rather than borrowed from it, and the property
        // it guards is unchanged: a marker Crucible wrote must survive one.
        let loaded: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        std::fs::write(&config_path, toml::to_string_pretty(&loaded).unwrap()).unwrap();

        let config: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(
            config.kilns["notes"],
            crate::config::config::registry::KilnEntry::Config {
                path: notes,
                lazy: false,
                auto: true,
            }
        );
    }

    /// A name the user typed is written as the shorthand every hand-written
    /// config uses — `auto` is a claim about provenance, and `cru kiln
    /// register` is the user's own hand.
    #[test]
    fn an_explicitly_named_kiln_is_written_as_the_plain_shorthand() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let notes = tmp.path().join("notes");

        register_kiln_entry_in_config(&config_path, "notes", &notes, false).unwrap();

        let config: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(
            config.kilns["notes"],
            crate::config::config::registry::KilnEntry::Path(notes)
        );
    }

    /// Re-pointing an existing entry is refused at the writer too, not only at
    /// the registry that called it. The registry holds the in-memory answer;
    /// this holds the file, and a file the registry never saw (edited between
    /// the two steps, or a second `cru` running concurrently) is exactly the
    /// case where the two disagree. A session that persisted `notes` yesterday
    /// must not open a different corpus today.
    #[test]
    fn registering_a_name_over_a_different_directory_is_refused() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[kilns]\nnotes = \"/first/notes\"\n").unwrap();

        let err = register_kiln_entry_in_config(
            &config_path,
            "notes",
            std::path::Path::new("/second/notes"),
            true,
        )
        .expect_err("an existing entry must not be silently repointed");
        assert!(err.to_string().contains("notes"), "{err}");

        let config: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(config.kilns["notes"].path(), PathBuf::from("/first/notes"));
    }

    /// Re-registering the identical pair is a no-op, so re-running the command
    /// (or the same `--kiln` flag twice) is not an error.
    #[test]
    fn registering_the_same_name_and_path_again_succeeds() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let notes = tmp.path().join("notes");

        register_kiln_entry_in_config(&config_path, "notes", &notes, true).unwrap();
        register_kiln_entry_in_config(&config_path, "notes", &notes, true).unwrap();

        let config: CliAppConfig =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(config.kilns.len(), 1);
    }

    /// The file is hand-edited, so the same guarantee the other writers give.
    #[test]
    fn registering_a_kiln_entry_preserves_comments_and_unknown_keys() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "# my careful notes\n[llm]\ndefault = \"zai-coding\"\n\n\
             [some_future_section]\nkey = \"value\"\n",
        )
        .unwrap();

        register_kiln_entry_in_config(&config_path, "notes", tmp.path(), true).unwrap();

        let after = std::fs::read_to_string(&config_path).unwrap();
        assert!(after.contains("# my careful notes"), "{after}");
        assert!(after.contains("[some_future_section]"), "{after}");
        assert!(after.contains("zai-coding"), "{after}");
    }
}
