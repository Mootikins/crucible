//! Relocate pre-flatten session directories into the daemon's sessions root.
//!
//! Sessions used to be written inside their owning kiln at
//! `{kiln}/.crucible/sessions/{id}`, with one exception: when the kiln *was*
//! the crucible home, they were written to `{home}/sessions/{id}` — which is
//! the layout everything now uses. So the move is "collect the first shape,
//! leave the second alone".
//!
//! This runs at every daemon start rather than once behind a marker file: a
//! kiln registered after the first run has sessions nobody has moved yet, and
//! the scan is a handful of `read_dir` calls against directories that are
//! normally absent.

use crucible_core::Project;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// What one migration pass did. Counts only — the interesting detail goes to
/// the log, where a user looking for a relocated session will actually find it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Sessions moved into the root under their own id.
    pub moved: usize,
    /// Sessions moved under a disambiguated id because their id was taken.
    pub disambiguated: usize,
    /// Sessions left where they were, because moving them would have
    /// destroyed something.
    pub skipped: usize,
    /// Configured kiln roots that could not be read at all. Counted separately
    /// because the per-session counters can only count sessions that were
    /// seen — a root that resolves to nothing hides an unknown number of them.
    pub unreadable_roots: usize,
}

impl MigrationReport {
    fn total(&self) -> usize {
        self.moved + self.disambiguated + self.skipped + self.unreadable_roots
    }
}

/// Kiln roots that may hold pre-relocation session directories.
///
/// Deliberately wide. `CliAppConfig::default_kiln_path()` is the process's
/// current directory, so under the old layout a session's storage kiln was
/// often just wherever `cru` happened to be invoked — and `kiln_path` as the
/// daemon sees it is one snapshot, from whichever invocation spawned it. No
/// single setting covers the set, so every directory the daemon has a record of
/// gets probed: the named kiln registry, the flat `kiln_path`/`session_kiln`
/// settings, every registered project root (config-side and registry-side), and
/// every kiln bound to one. Probing a directory that never held sessions costs
/// one `read_dir` that returns `ENOENT`, and migration runs on every boot, so a
/// project registered later is still rescued on the next start.
///
/// `data_home` is included because an injected data root (tests, `CRUCIBLE_HOME`
/// relocations) was treated as an ordinary kiln by the old layout and has
/// sessions under `{data_home}/.crucible/sessions`.
///
/// Paths are tilde-expanded: `vault = "~/vault"` is documented config, and a
/// literal `~` directory would silently probe nothing.
///
/// Sorted and deduplicated so a collision resolves the same way on every run.
pub fn known_kiln_roots(
    app_config: Option<&serde_json::Value>,
    projects: &[Project],
    data_home: &Path,
) -> Vec<PathBuf> {
    let mut roots = vec![data_home.to_path_buf()];

    if let Some(config) = app_config {
        for key in ["kiln_path", "session_kiln"] {
            if let Some(path) = config.get(key).and_then(|v| v.as_str()) {
                roots.push(PathBuf::from(path));
            }
        }
        // `KilnEntry` is untagged (a bare path string or a table with a `path`
        // key); `ProjectEntry` is always a table with a `path` key. Both read
        // the same way.
        for section in ["kilns", "projects"] {
            let Some(entries) = config.get(section).and_then(|v| v.as_object()) else {
                continue;
            };
            for entry in entries.values() {
                let path = entry
                    .as_str()
                    .or_else(|| entry.get("path").and_then(|p| p.as_str()));
                if let Some(path) = path {
                    roots.push(PathBuf::from(path));
                }
            }
        }
    }

    for project in projects {
        roots.push(project.path.clone());
        roots.extend(project.kilns.iter().map(|k| k.path.clone()));
    }

    roots = roots
        .iter()
        .map(|p| crate::kiln_manager::expand_tilde_path(p))
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Move every session found under a legacy kiln layout into `sessions_root`.
///
/// Failures are per-session and non-fatal: a session that cannot be moved is
/// left exactly where it is and reported, because the alternative to an
/// un-migrated session is a lost one.
pub async fn migrate_sessions(sessions_root: &Path, kilns: &[PathBuf]) -> MigrationReport {
    let mut report = MigrationReport::default();

    // Anything under the staging root was never published, so a copy left there
    // by a pass that died mid-flight is garbage by construction — its source is
    // still intact in the kiln. Clearing it first is what stops a truncated copy
    // from being mistaken for a real session on the next boot.
    let _ = tokio::fs::remove_dir_all(staging_root(sessions_root)).await;

    for kiln in kilns {
        let legacy_dir = kiln.join(".crucible").join("sessions");
        // Already in place: the home-kiln arm of the old layout wrote to the
        // destination itself, so there is nothing to move.
        if legacy_dir == sessions_root {
            continue;
        }
        let mut entries = match tokio::fs::read_dir(&legacy_dir).await {
            Ok(entries) => entries,
            // A kiln with no legacy sessions directory is the overwhelmingly
            // common case and says nothing. A kiln root that is not there at
            // all is different: it is a configured path whose sessions, however
            // many, nobody can reach — and staying quiet about it is how they
            // get orphaned.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if tokio::fs::metadata(kiln).await.is_err() {
                    warn!(root = %kiln.display(),
                          "A configured kiln root could not be reached; any sessions still filed under it were not relocated");
                    report.unreadable_roots += 1;
                }
                continue;
            }
            Err(e) => {
                warn!(root = %kiln.display(), error = %e,
                      "Could not read a kiln's legacy sessions directory; its sessions were not relocated");
                report.unreadable_roots += 1;
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let session_id = entry.file_name().to_string_lossy().into_owned();
            migrate_one(&entry.path(), sessions_root, &session_id, kiln, &mut report).await;
        }

        // Tidy up only if the move emptied it; a non-empty directory means
        // something was skipped and must stay findable.
        let _ = tokio::fs::remove_dir(&legacy_dir).await;
    }

    let _ = tokio::fs::remove_dir(staging_root(sessions_root)).await;

    if report.total() > 0 {
        info!(
            moved = report.moved,
            disambiguated = report.disambiguated,
            skipped = report.skipped,
            unreadable_roots = report.unreadable_roots,
            root = %sessions_root.display(),
            "Relocated sessions out of kiln storage"
        );
    }
    report
}

async fn migrate_one(
    source: &Path,
    sessions_root: &Path,
    session_id: &str,
    kiln: &Path,
    report: &mut MigrationReport,
) {
    let dest = sessions_root.join(session_id);
    if !dest.exists() {
        match relocate(source, &dest, None).await {
            Ok(()) => report.moved += 1,
            Err(e) => {
                warn!(session_id = %session_id, source = %source.display(), error = %e,
                      "Could not relocate session; leaving it in the kiln");
                report.skipped += 1;
            }
        }
        return;
    }

    // The id is taken. Ids embed a timestamp and a random suffix, so this is
    // not two sessions colliding by chance — it is the same session copied into
    // two kilns, or a previous pass that already moved this one. Overwriting
    // would destroy a real transcript either way, so the loser gets a suffix
    // derived from its kiln path: stable across runs (so a re-run is idempotent
    // rather than accumulating copies) and traceable back to where it came from.
    let suffixed = format!("{session_id}--{}", kiln_tag(kiln));
    let dest = sessions_root.join(&suffixed);
    if dest.exists() {
        warn!(session_id = %session_id, kiln = %kiln.display(),
              "Session id already present under both its own and its disambiguated name; leaving it in the kiln");
        report.skipped += 1;
        return;
    }
    match relocate(source, &dest, Some(&suffixed)).await {
        Ok(()) => {
            info!(session_id = %session_id, migrated_as = %suffixed, kiln = %kiln.display(),
                  "Session id collided during relocation; stored under a kiln-derived name");
            report.disambiguated += 1;
        }
        Err(e) => {
            warn!(session_id = %session_id, error = %e,
                  "Could not relocate session; leaving it in the kiln");
            report.skipped += 1;
        }
    }
}

/// Eight hex characters of the kiln path — enough to separate the handful of
/// kilns one machine has, short enough to leave the session id readable.
fn kiln_tag(kiln: &Path) -> String {
    blake3::hash(kiln.as_os_str().as_encoded_bytes()).to_hex()[..8].to_string()
}

/// Where in-flight copies live: one hidden directory beside the sessions, so a
/// leftover is a single entry `FileSessionStorage::list` cannot parse as a
/// session rather than one stray per interrupted move.
fn staging_root(sessions_root: &Path) -> PathBuf {
    sessions_root.join(".migrating")
}

/// Move `source` to `dest`, falling back to copy-then-delete when a rename
/// cannot cross whatever `dest` is mounted on (a kiln on another filesystem is
/// ordinary — an external drive, a synced folder).
///
/// `new_id`, when set, is written into the session's metadata before the move
/// completes: storage keys reads on the directory name and writes on
/// `session.id`, so a session relocated under any name but its own has to carry
/// that name inside it too.
///
/// `dest` only ever appears complete. The rename path is atomic already; the
/// copy path stages under [`staging_root`] and renames into place, so a process
/// killed mid-copy leaves the source intact and nothing at `dest` — where a
/// partial `dest` would look like a real session to the next pass and push the
/// intact original onto the collision path.
async fn relocate(source: &Path, dest: &Path, new_id: Option<&str>) -> std::io::Result<()> {
    let Some(parent) = dest.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} has no parent directory", dest.display()),
        ));
    };
    tokio::fs::create_dir_all(parent).await?;

    // Rewritten in the source, before anything moves. Crash after this and the
    // next pass re-derives the same suffix from the same kiln path and rewrites
    // the same value — idempotent, and never a window where the metadata has
    // been updated for a directory that does not exist yet.
    if let Some(new_id) = new_id {
        rewrite_session_id(source, new_id).await?;
    }

    if tokio::fs::rename(source, dest).await.is_ok() {
        return Ok(());
    }

    let staged = staging_root(parent).join(dest.file_name().unwrap_or_default());
    let _ = tokio::fs::remove_dir_all(&staged).await;
    if let Err(e) = copy_dir(source, &staged).await {
        let _ = tokio::fs::remove_dir_all(&staged).await;
        return Err(e);
    }
    if let Err(e) = tokio::fs::rename(&staged, dest).await {
        let _ = tokio::fs::remove_dir_all(&staged).await;
        return Err(e);
    }
    if let Err(e) = tokio::fs::remove_dir_all(source).await {
        // The session is migrated; only the original copy is still lying
        // around. Reporting failure here would count a successful move as
        // skipped and hand the next pass a collision to disambiguate.
        warn!(source = %source.display(), error = %e,
              "Relocated the session but could not remove its original copy");
    }
    Ok(())
}

/// Point a relocated session's persisted id at the directory it now lives in.
///
/// `FileSessionStorage` resolves a load by directory name and every write by
/// `session.id`. Let those disagree and the first save of a disambiguated
/// session lands in the winner's directory, overwriting its `meta.json` and
/// appending the loser's turns to its log — precisely the loss the suffix
/// exists to prevent.
///
/// Edited as raw JSON so that fields this build does not know about survive.
async fn rewrite_session_id(dir: &Path, new_id: &str) -> std::io::Result<()> {
    let mut rewritten = 0;
    // `load` prefers `meta.json` and falls back to the older `session.json`;
    // whichever are present must agree with the directory.
    for name in ["meta.json", "session.json"] {
        let path = dir.join(name);
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mut meta: serde_json::Value = serde_json::from_str(&raw)?;
        let Some(fields) = meta.as_object_mut() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} is not a JSON object", path.display()),
            ));
        };
        fields.insert("id".to_string(), serde_json::Value::String(new_id.into()));
        tokio::fs::write(&path, serde_json::to_string_pretty(&meta)?).await?;
        rewritten += 1;
    }
    if rewritten == 0 {
        // No metadata means nothing can carry the new id, and a directory whose
        // name and contents disagree is the bug this function exists to close.
        // Fail, so the caller leaves the session in the kiln where it is at
        // least findable.
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} holds no session metadata", dir.display()),
        ));
    }
    Ok(())
}

/// Recursive directory copy, iterative so the future stays `Sized`.
async fn copy_dir(source: &Path, dest: &Path) -> std::io::Result<()> {
    let mut pending = vec![(source.to_path_buf(), dest.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        tokio::fs::create_dir_all(&to).await?;
        let mut entries = tokio::fs::read_dir(&from).await?;
        while let Some(entry) = entries.next_entry().await? {
            let target = to.join(entry.file_name());
            if entry.file_type().await?.is_dir() {
                pending.push((entry.path(), target));
            } else {
                tokio::fs::copy(entry.path(), target).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_storage::{FileSessionStorage, SessionStorage};
    use crucible_core::session::{Session, SessionType};
    use tempfile::TempDir;

    async fn seed_session(kiln: &Path, id: &str, body: &str) {
        let dir = kiln.join(".crucible").join("sessions").join(id);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("meta.json"), body).await.unwrap();
    }

    async fn read_meta(dir: &Path) -> serde_json::Value {
        let raw = tokio::fs::read_to_string(dir.join("meta.json"))
            .await
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// A real `meta.json` body: the id-rewrite tests need something
    /// `FileSessionStorage::load` will actually parse.
    fn session_meta(id: &str, title: &str) -> String {
        let mut session = Session::new(SessionType::Chat, vec![PathBuf::from("/kiln")]);
        session.id = id.to_string();
        session.title = Some(title.to_string());
        serde_json::to_string_pretty(&session).unwrap()
    }

    #[tokio::test]
    async fn moves_sessions_out_of_kilns() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("notes");
        let root = tmp.path().join("home").join("sessions");
        seed_session(&kiln, "chat-1", "{}").await;

        let report = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

        assert_eq!(
            report,
            MigrationReport {
                moved: 1,
                ..Default::default()
            }
        );
        assert!(root.join("chat-1").join("meta.json").exists());
        assert!(!kiln.join(".crucible").join("sessions").exists());
    }

    #[tokio::test]
    async fn an_entry_already_in_the_root_is_a_no_op() {
        // Production's home kiln: `data_home` is `~/.crucible`, so if `~` is
        // itself registered as a kiln its legacy sessions directory IS the
        // destination. Scanning it would relocate every session onto itself.
        let tmp = TempDir::new().unwrap();
        let home_kiln = tmp.path().to_path_buf();
        let root = home_kiln.join(".crucible").join("sessions");
        seed_session(&home_kiln, "chat-1", r#"{"from":"root"}"#).await;

        let report = migrate_sessions(&root, std::slice::from_ref(&home_kiln)).await;

        assert_eq!(report, MigrationReport::default());
        assert_eq!(
            tokio::fs::read_to_string(root.join("chat-1").join("meta.json"))
                .await
                .unwrap(),
            r#"{"from":"root"}"#
        );
    }

    #[tokio::test]
    async fn a_kiln_with_no_legacy_directory_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("home").join("sessions");
        let kiln = tmp.path().join("empty-kiln");
        tokio::fs::create_dir_all(&kiln).await.unwrap();

        let report = migrate_sessions(&root, &[kiln]).await;

        assert_eq!(report, MigrationReport::default());
    }

    #[tokio::test]
    async fn a_configured_kiln_root_that_cannot_be_read_is_reported() {
        // The failure this guards: a root that resolves to nothing is skipped
        // by `read_dir` alone, so every session under it is orphaned without a
        // word in the log.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("home").join("sessions");

        let report = migrate_sessions(&root, &[tmp.path().join("not-there")]).await;

        assert_eq!(
            report,
            MigrationReport {
                unreadable_roots: 1,
                ..Default::default()
            }
        );
    }

    #[tokio::test]
    async fn colliding_ids_are_disambiguated_not_overwritten() {
        let tmp = TempDir::new().unwrap();
        let first = tmp.path().join("kiln-a");
        let second = tmp.path().join("kiln-b");
        let root = tmp.path().join("home").join("sessions");
        seed_session(&first, "chat-1", r#"{"id":"chat-1","from":"a"}"#).await;
        seed_session(&second, "chat-1", r#"{"id":"chat-1","from":"b"}"#).await;

        let report = migrate_sessions(&root, &[first.clone(), second.clone()]).await;

        assert_eq!(
            report,
            MigrationReport {
                moved: 1,
                disambiguated: 1,
                ..Default::default()
            }
        );
        let survivor = read_meta(&root.join("chat-1")).await;
        assert_eq!(survivor["from"], "a");
        assert_eq!(survivor["id"], "chat-1");
        let suffixed = format!("chat-1--{}", kiln_tag(&second));
        let renamed = read_meta(&root.join(&suffixed)).await;
        assert_eq!(renamed["from"], "b", "the loser's contents are preserved");
        assert_eq!(
            renamed["id"], suffixed,
            "only the id follows the directory it was filed under"
        );
    }

    #[tokio::test]
    async fn rerunning_after_a_collision_leaves_both_copies_alone() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln-a");
        let root = tmp.path().join("home").join("sessions");
        tokio::fs::create_dir_all(root.join("chat-1"))
            .await
            .unwrap();
        tokio::fs::write(root.join("chat-1").join("meta.json"), r#"{"from":"root"}"#)
            .await
            .unwrap();
        seed_session(&kiln, "chat-1", r#"{"from":"a"}"#).await;

        let first = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;
        seed_session(&kiln, "chat-1", r#"{"from":"a"}"#).await;
        let second = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

        assert_eq!(first.disambiguated, 1);
        // The disambiguated name is stable, so the second pass has nowhere
        // safe to put the re-seeded copy and refuses to touch it.
        assert_eq!(second.skipped, 1);
        assert!(kiln
            .join(".crucible")
            .join("sessions")
            .join("chat-1")
            .exists());
        assert_eq!(
            tokio::fs::read_to_string(root.join("chat-1").join("meta.json"))
                .await
                .unwrap(),
            r#"{"from":"root"}"#
        );
    }

    #[tokio::test]
    async fn a_disambiguated_session_writes_to_its_own_directory() {
        // Storage keys reads on the directory name and every write on
        // `session.id`. If the id does not follow the rename, the first save of
        // the disambiguated session lands in the winner's directory — the loss
        // the suffix exists to prevent.
        let tmp = TempDir::new().unwrap();
        let first = tmp.path().join("kiln-a");
        let second = tmp.path().join("kiln-b");
        let root = tmp.path().join("home").join("sessions");
        seed_session(&first, "chat-1", &session_meta("chat-1", "a")).await;
        seed_session(&second, "chat-1", &session_meta("chat-1", "b")).await;

        migrate_sessions(&root, &[first.clone(), second.clone()]).await;

        let suffixed = format!("chat-1--{}", kiln_tag(&second));
        let storage = FileSessionStorage::new(root.clone());
        let loser = storage.load(&suffixed).await.unwrap();
        assert_eq!(loser.id, suffixed, "persisted id must follow the directory");

        // Exactly what `resume_session_from_storage` does on the way back in.
        storage.save(&loser).await.unwrap();

        assert_eq!(
            storage.load("chat-1").await.unwrap().title.as_deref(),
            Some("a"),
            "resuming the disambiguated session overwrote the winner"
        );
        assert_eq!(
            storage.load(&suffixed).await.unwrap().title.as_deref(),
            Some("b")
        );
    }

    #[tokio::test]
    async fn a_staged_copy_left_by_an_interrupted_pass_never_becomes_a_session() {
        // A pass killed mid-copy leaves a partial directory behind. It must not
        // be published, and it must not push the intact original onto the
        // collision path.
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln-a");
        let root = tmp.path().join("home").join("sessions");
        let staged = staging_root(&root).join("chat-1");
        tokio::fs::create_dir_all(&staged).await.unwrap();
        tokio::fs::write(staged.join("meta.json"), r#"{"trunc"#)
            .await
            .unwrap();
        seed_session(&kiln, "chat-1", &session_meta("chat-1", "intact")).await;

        let report = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

        assert_eq!(
            report,
            MigrationReport {
                moved: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            FileSessionStorage::new(root.clone())
                .load("chat-1")
                .await
                .unwrap()
                .title
                .as_deref(),
            Some("intact")
        );
        assert!(!staging_root(&root).exists());
    }

    #[tokio::test]
    async fn a_failed_relocation_leaves_the_source_and_the_destination_alone() {
        // Forces the copy fallback (renaming a directory onto an existing file
        // is ENOTDIR) and then fails the publish for the same reason.
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("source");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::write(source.join("meta.json"), "{}")
            .await
            .unwrap();
        let dest = tmp.path().join("dest");
        tokio::fs::write(&dest, "not a directory").await.unwrap();

        assert!(relocate(&source, &dest, None).await.is_err());

        assert!(source.join("meta.json").exists());
        assert_eq!(
            tokio::fs::read_to_string(&dest).await.unwrap(),
            "not a directory"
        );
        assert!(!staging_root(tmp.path()).join("dest").exists());
    }

    #[test]
    fn known_roots_expand_a_leading_tilde() {
        let home = dirs::home_dir().expect("a home directory");
        let config = serde_json::json!({
            "kiln_path": "~/flat",
            "session_kiln": "~",
            "kilns": { "vault": "~/vault" },
        });

        let roots = known_kiln_roots(Some(&config), &[], Path::new("/data"));

        assert!(roots.contains(&home.join("flat")));
        assert!(roots.contains(&home.join("vault")));
        assert!(roots.contains(&home));
        assert!(!roots.iter().any(|r| r.starts_with("~")));
    }

    #[test]
    fn known_roots_include_project_roots_and_configured_projects() {
        // `kiln_path` defaulted to the invoking process's cwd, so a session's
        // storage kiln was whatever directory `cru` ran in. One snapshot cannot
        // cover that set; the project registry can.
        let config = serde_json::json!({
            "projects": { "crucible": { "path": "/code/crucible", "kilns": ["vault"] } },
        });
        let project = Project {
            path: PathBuf::from("/code/other"),
            name: "other".into(),
            kilns: vec![crucible_core::project::ProjectKiln {
                path: PathBuf::from("/kilns/bound"),
                name: None,
            }],
            last_accessed: chrono::Utc::now(),
            repository: None,
        };

        let roots = known_kiln_roots(Some(&config), &[project], Path::new("/data"));

        assert!(roots.contains(&PathBuf::from("/code/crucible")));
        assert!(roots.contains(&PathBuf::from("/code/other")));
        assert!(roots.contains(&PathBuf::from("/kilns/bound")));
    }

    #[test]
    fn known_roots_read_both_kiln_entry_shapes() {
        let config = serde_json::json!({
            "kiln_path": "/kilns/flat",
            "kilns": {
                "vault": "/kilns/vault",
                "work": { "path": "/kilns/work", "lazy": true },
            },
        });
        let roots = known_kiln_roots(Some(&config), &[], Path::new("/data"));
        assert!(roots.contains(&PathBuf::from("/kilns/flat")));
        assert!(roots.contains(&PathBuf::from("/kilns/vault")));
        assert!(roots.contains(&PathBuf::from("/kilns/work")));
        assert!(roots.contains(&PathBuf::from("/data")));
    }
}
