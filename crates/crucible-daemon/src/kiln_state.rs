//! `<data_home>/kilns.json` — the kiln registrations the daemon was told about.
//!
//! # Registration is state, not config
//!
//! A registered kiln is a fact the daemon was told, not a preference the user
//! authored. `projects.json` already proves the pattern: the daemon writes it,
//! the user never hand-edits it, and no config reload can lose it. `kilns.json`
//! is the same file for the same reason — a machine-written registry beside a
//! hand-written config, never inside it.
//!
//! # The two layers, and which one wins
//!
//! A kiln name can exist in the config layer (a `[kilns]` entry today) and in
//! this state layer. **The config layer wins on a name conflict**
//! ([`crucible_core::config::overlay_registrations`]). The shadowed state
//! entry stays in this file; the overlay only decides which layer answers the
//! name. No command removes an entry yet, so removal is a hand edit of this
//! file today.
//!
//! # Absence is not intent
//!
//! A kiln absent from the config but present here stays registered. A config
//! reload must never delete from this file: the daemon cannot tell "the user
//! deleted the line" from "the branch did not run", and a config language with
//! conditionals makes that undecidable. So absence never deletes.
//!
//! # The never-re-point rule lives here
//!
//! A name already pointed at one directory is never re-pointed at another. A
//! session that persisted `notes` yesterday must not open a different corpus
//! today. The check runs inside the store's locked read-modify-write, so a
//! second `cru` cannot slip between the check and the write.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use crucible_core::config::{KilnName, Registration};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::registry_store::RegistryStore;

/// The schema version this daemon writes and understands.
pub const KILN_STATE_VERSION: u32 = 1;

/// The file name under the daemon data root.
pub const KILN_STATE_FILE: &str = "kilns.json";

/// One registered kiln.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KilnStateEntry {
    /// Absolute and canonical. The writer normalizes before it compares or
    /// stores; a relative entry here would point somewhere different on every
    /// command.
    pub path: PathBuf,
    /// Crucible derived this entry; the user did not name it. Same meaning it
    /// has in a `[kilns]` table entry.
    #[serde(default)]
    pub auto: bool,
    /// RFC 3339, UTC. Provenance for the human reading the file.
    pub registered_at: String,
}

/// The whole of `kilns.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KilnStateFile {
    /// Gates future shape changes. A reader refuses a higher number rather
    /// than guessing at a schema it does not know.
    pub version: u32,
    /// The kiln every command uses when none is named. The machine sets it:
    /// the chat preflight registers with `make_default`. The config layer may
    /// also set `default_kiln`, and the config layer wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_kiln: Option<String>,
    /// Name → entry.
    #[serde(default)]
    pub kilns: BTreeMap<String, KilnStateEntry>,
}

impl Default for KilnStateFile {
    fn default() -> Self {
        Self {
            version: KILN_STATE_VERSION,
            default_kiln: None,
            kilns: BTreeMap::new(),
        }
    }
}

/// What a registration did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// A new entry landed.
    Added,
    /// The same name already pointed at the same directory. Re-running the
    /// command is not an error.
    AlreadyPresent,
}

/// The registrations store: `<data_home>/kilns.json`, and the rules over it.
#[derive(Debug, Clone)]
pub struct KilnStateStore {
    store: RegistryStore<KilnStateFile>,
}

impl KilnStateStore {
    /// The store under a daemon data root.
    pub fn new(data_home: &Path) -> Self {
        Self {
            store: RegistryStore::new(data_home.join(KILN_STATE_FILE)),
        }
    }

    /// The file this store owns. Refusals name it, so the user knows where the
    /// contested entry lives.
    pub fn path(&self) -> &Path {
        self.store.path()
    }

    /// Read the file, refusing a version this daemon does not understand.
    pub fn read(&self) -> Result<KilnStateFile> {
        gate_version(self.store.read()?, self.path())
    }

    /// The state layer as overlay input, in name order.
    ///
    /// An unreadable file yields no registrations and a warning rather than an
    /// error: the daemon still starts, with the config layer alone, and the
    /// user sees why in the log. A daemon that refuses to bind because a state
    /// file drifted is a daemon the user cannot run the repair command from.
    pub fn registrations(&self) -> Vec<Registration> {
        match self.read() {
            Ok(state) => state
                .kilns
                .into_iter()
                .map(|(name, entry)| {
                    Registration::registered(name, entry.path).with_auto(entry.auto)
                })
                .collect(),
            Err(e) => {
                warn!(
                    path = %self.path().display(),
                    error = %e,
                    "Could not read the kiln registrations; only configured kilns are known"
                );
                Vec::new()
            }
        }
    }

    /// The `default_kiln` the state layer names, if any.
    pub fn default_kiln(&self) -> Option<String> {
        self.read().ok().and_then(|state| state.default_kiln)
    }

    /// Persist one registration.
    ///
    /// `path` must already be absolute and normalized — the registry is what
    /// normalizes, and a caller that skips it stores a second entry for a
    /// directory the user has one name for.
    ///
    /// Refuses a name this file already points somewhere else. The check and
    /// the write happen under one lock, so a second writer cannot land between
    /// them.
    pub fn register(
        &self,
        name: &KilnName,
        path: &Path,
        auto: bool,
        make_default: bool,
    ) -> Result<RegisterOutcome> {
        anyhow::ensure!(
            path.is_absolute(),
            "refusing to register kiln '{name}' at the relative path '{}': \
             a registration must be absolute",
            path.display()
        );
        let file = self.path().to_path_buf();

        self.store.update(|state| {
            *state = gate_version(std::mem::take(state), &file)?;

            let outcome = match state.kilns.get(name.as_str()) {
                Some(existing) if existing.path == path => RegisterOutcome::AlreadyPresent,
                Some(existing) => bail!(
                    "the kiln name '{name}' is already registered to '{}' in {}. \
                     Choose another name, or remove that entry first.",
                    existing.path.display(),
                    file.display()
                ),
                None => {
                    state.kilns.insert(
                        name.to_string(),
                        KilnStateEntry {
                            path: path.to_path_buf(),
                            auto,
                            registered_at: chrono::Utc::now().to_rfc3339(),
                        },
                    );
                    RegisterOutcome::Added
                }
            };

            if make_default || state.default_kiln.is_none() {
                state.default_kiln = Some(name.to_string());
            }
            Ok(outcome)
        })
    }
}

/// Refuse a file this daemon is too old to read.
///
/// Fail closed: a higher version means keys this build does not model, and
/// writing the file back would erase them.
fn gate_version(state: KilnStateFile, path: &Path) -> Result<KilnStateFile> {
    anyhow::ensure!(
        state.version <= KILN_STATE_VERSION,
        "{} is version {}; this daemon understands version {KILN_STATE_VERSION}. \
         The daemon is older than the file — upgrade Crucible.",
        path.display(),
        state.version
    );
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn name(s: &str) -> KilnName {
        KilnName::parse(s).expect("test name must be valid")
    }

    #[test]
    fn a_registration_survives_a_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());

        assert_eq!(
            store
                .register(&name("notes"), Path::new("/a/notes"), false, false)
                .unwrap(),
            RegisterOutcome::Added
        );

        let state = store.read().unwrap();
        assert_eq!(state.version, KILN_STATE_VERSION);
        assert_eq!(state.kilns["notes"].path, PathBuf::from("/a/notes"));
        assert!(!state.kilns["notes"].auto);
        assert!(
            !state.kilns["notes"].registered_at.is_empty(),
            "the entry must carry when it was written"
        );
    }

    /// The never-re-point rule. A session that persisted `notes` yesterday must
    /// not open a different corpus today.
    #[test]
    fn a_name_is_never_re_pointed_at_another_directory() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());
        store
            .register(&name("notes"), Path::new("/first/notes"), false, false)
            .unwrap();

        let err = store
            .register(&name("notes"), Path::new("/second/notes"), false, false)
            .expect_err("an existing entry must not be silently re-pointed");
        assert!(err.to_string().contains("/first/notes"), "{err}");
        assert!(
            err.to_string().contains("kilns.json"),
            "the refusal must name the file: {err}"
        );

        assert_eq!(
            store.read().unwrap().kilns["notes"].path,
            PathBuf::from("/first/notes"),
            "the refused write must not have landed"
        );
    }

    #[test]
    fn re_registering_the_same_name_and_path_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());
        store
            .register(&name("notes"), Path::new("/a/notes"), false, false)
            .unwrap();

        assert_eq!(
            store
                .register(&name("notes"), Path::new("/a/notes"), false, false)
                .unwrap(),
            RegisterOutcome::AlreadyPresent
        );
        assert_eq!(store.read().unwrap().kilns.len(), 1);
    }

    /// The first registration claims the default; a later one does not steal
    /// it. `make_default` is the chat preflight's answer to a different
    /// question, and only it may re-point the default.
    #[test]
    fn a_second_registration_leaves_the_existing_default_alone() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());

        store
            .register(&name("first"), Path::new("/a"), false, false)
            .unwrap();
        store
            .register(&name("second"), Path::new("/b"), false, false)
            .unwrap();
        assert_eq!(store.read().unwrap().default_kiln.as_deref(), Some("first"));

        store
            .register(&name("third"), Path::new("/c"), false, true)
            .unwrap();
        assert_eq!(store.read().unwrap().default_kiln.as_deref(), Some("third"));
    }

    #[test]
    fn a_relative_path_is_refused_before_the_file_is_touched() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());

        assert!(store
            .register(&name("notes"), Path::new("relative/notes"), false, false)
            .is_err());
        assert!(!store.path().exists(), "no file may have been created");
    }

    /// A file from a newer Crucible is refused rather than rewritten: writing
    /// it back through this build's struct would erase the keys this build does
    /// not model.
    #[test]
    fn a_newer_version_is_refused_on_read_and_on_write() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());
        std::fs::write(
            store.path(),
            r#"{"version": 99, "kilns": {"notes": {"path": "/a", "registered_at": "x"}}}"#,
        )
        .unwrap();

        let err = store.read().expect_err("a newer file must be refused");
        assert!(err.to_string().contains("version 99"), "{err}");

        assert!(store
            .register(&name("other"), Path::new("/b"), false, false)
            .is_err());
        let after = std::fs::read_to_string(store.path()).unwrap();
        assert!(
            after.contains("\"version\": 99"),
            "the refused write must not have rewritten the file: {after}"
        );
    }

    /// An unreadable file must not take the daemon down: it starts with the
    /// config layer alone and says why.
    #[test]
    fn an_unreadable_file_yields_no_registrations_rather_than_an_error() {
        let tmp = TempDir::new().unwrap();
        let store = KilnStateStore::new(tmp.path());
        std::fs::write(store.path(), "{ not json").unwrap();

        assert!(store.registrations().is_empty());
    }
}
