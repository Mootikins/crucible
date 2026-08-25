//! Locked, atomic read-modify-write over one JSON state file.
//!
//! The daemon owns two registries that outlive the process — `projects.json`
//! and `kilns.json` — and both need the same four steps: take a lock, read,
//! check, replace atomically. This is that shape, once.
//!
//! The lock is a `<file>.lock` sidecar, the pattern
//! [`crate::plugin_ops`] already uses for `plugins.toml`, and for the same
//! reason: the data file is *renamed* on every write, so locking it would let
//! two writers hold locks on two different inodes. The sidecar is never
//! renamed, so lock identity is stable, and an unlocked reader only ever sees
//! a complete file.
//!
//! The daemon is the only writer in normal operation. The lock exists for the
//! one writer that must stay possible without a daemon — a migration seeding
//! the file — and for two daemons racing at boot.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// One JSON state file, read and written under a sidecar lock.
///
/// `T` is the whole file, not one entry: the store never merges, it replaces.
#[derive(Debug, Clone)]
pub struct RegistryStore<T> {
    path: PathBuf,
    _marker: PhantomData<fn() -> T>,
}

impl<T> RegistryStore<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    /// A store over `path`. Nothing is read or created until a call.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            _marker: PhantomData,
        }
    }

    /// The file this store owns. Named in refusals, so the user knows which
    /// file to edit.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the file. A missing file is `T::default()`, not an error: an empty
    /// registry is a real state, and the first registration is what creates
    /// the file.
    pub fn read(&self) -> Result<T> {
        let _lock = self.lock()?;
        self.read_unlocked()
    }

    /// Read, mutate, write — all under one lock.
    ///
    /// `mutate` returning `Err` leaves the file untouched. That is what makes
    /// a refusal (a name already pointed somewhere else) safe: the check and
    /// the write cannot be separated by another writer.
    pub fn update<R>(&self, mutate: impl FnOnce(&mut T) -> Result<R>) -> Result<R> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let _lock = self.lock()?;
        let mut value = self.read_unlocked()?;
        let result = mutate(&mut value)?;
        self.write_unlocked(&value)?;
        Ok(result)
    }

    fn read_unlocked(&self) -> Result<T> {
        if !self.path.exists() {
            return Ok(T::default());
        }
        let content = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        if content.trim().is_empty() {
            return Ok(T::default());
        }
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", self.path.display()))
    }

    /// Write beside, then rename over. A reader without the lock sees the old
    /// file or the new one, never half of either.
    fn write_unlocked(&self, value: &T) -> Result<()> {
        let serialized =
            serde_json::to_string_pretty(value).context("failed to serialize registry state")?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("failed to create a temp file in {}", parent.display()))?;
        {
            use std::io::Write as _;
            tmp.as_file_mut()
                .write_all(serialized.as_bytes())
                .context("failed to write the registry state")?;
            tmp.as_file_mut()
                .sync_all()
                .context("failed to flush the registry state")?;
        }
        // `NamedTempFile` creates 0600 and `persist` keeps it. These files hold
        // paths, not credentials, so restore the conventional 0644 the rest of
        // the data root uses.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o644))
                .context("failed to set the registry state permissions")?;
        }
        tmp.persist(&self.path)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }

    /// Open (creating if needed) and exclusively lock the `<file>.lock`
    /// sidecar. The returned handle holds the lock until it drops.
    ///
    /// `lock_exclusive`, not `try_lock_exclusive`: a registration is a single
    /// short read-modify-write, and the contending writer is a second `cru`
    /// doing the same thing. Failing fast here would turn "two commands at
    /// once" into an error the user has to understand.
    fn lock(&self) -> Result<std::fs::File> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut lock_path = self.path.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock_path = PathBuf::from(lock_path);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock {}", lock_path.display()))?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    type Map = BTreeMap<String, String>;

    #[test]
    fn a_missing_file_reads_as_the_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store: RegistryStore<Map> = RegistryStore::new(tmp.path().join("sub").join("s.json"));
        assert!(store.read().unwrap().is_empty());
    }

    #[test]
    fn an_update_round_trips() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store: RegistryStore<Map> = RegistryStore::new(tmp.path().join("s.json"));

        store
            .update(|map| {
                map.insert("a".into(), "1".into());
                Ok(())
            })
            .unwrap();

        assert_eq!(store.read().unwrap()["a"], "1");
    }

    /// The refusal contract: an `Err` from the mutation must leave the file as
    /// it was. Every never-re-point check depends on it.
    #[test]
    fn a_refused_update_leaves_the_file_untouched() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store: RegistryStore<Map> = RegistryStore::new(tmp.path().join("s.json"));
        store
            .update(|map| {
                map.insert("a".into(), "1".into());
                Ok(())
            })
            .unwrap();

        let err = store.update(|map: &mut Map| -> Result<()> {
            map.insert("a".into(), "2".into());
            anyhow::bail!("no")
        });

        assert!(err.is_err());
        assert_eq!(store.read().unwrap()["a"], "1");
    }

    /// Two writers, no lost update. The lock is what makes read-check-write a
    /// single step; without it the second writer's read predates the first
    /// writer's write and one entry disappears.
    #[test]
    fn concurrent_writers_do_not_lose_an_entry() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("s.json");

        let handles: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let store: RegistryStore<Map> = RegistryStore::new(path);
                    store
                        .update(|map| {
                            map.insert(format!("k{n}"), n.to_string());
                            Ok(())
                        })
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let store: RegistryStore<Map> = RegistryStore::new(path);
        let map = store.read().unwrap();
        assert_eq!(map.len(), 8, "every writer's entry must survive: {map:?}");
    }
}
