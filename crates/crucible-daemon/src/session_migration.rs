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

use crucible_core::session::SessionId;
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

/// The id a legacy session directory may be published under, or `None`.
///
/// Migration is the one path that moves a `meta.json` written by *somewhere
/// else* into the daemon's sessions root — a kiln can be shared, synced, or
/// restored from another machine's backup, which is the portability the kiln
/// design is for. Two strings have to be checked, not one:
///
/// - the **directory name**, which becomes `{sessions_root}/{name}`;
/// - the **`id` field inside `meta.json`**, which every later handler resolves
///   against the sessions root and which `session.cleanup` hands to
///   `remove_dir_all`. On the non-collision path this file was published byte
///   for byte, so an id of `"../keys"` arrived intact.
///
/// Either one failing means the session stays in the kiln. Not deleted, not
/// rewritten into something plausible: findable, where its owner left it, with
/// a line in the log saying why.
async fn publishable_id(source: &Path, session_id: &str) -> Option<SessionId> {
    let dir_id = SessionId::parse(session_id)
        .inspect_err(|e| {
            warn!(error = %e, source = %source.display(),
                  "Legacy session directory is not named like a session; leaving it in the kiln");
        })
        .ok()?;

    for name in ["meta.json", "session.json"] {
        let Ok(raw) = tokio::fs::read_to_string(source.join(name)).await else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(persisted) = meta.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Err(e) = SessionId::parse(persisted) {
            warn!(error = %e, source = %source.display(), file = name,
                  "Legacy session metadata carries an id that is not a session id; leaving it in the kiln");
            return None;
        }
    }

    Some(dir_id)
}

async fn migrate_one(
    source: &Path,
    sessions_root: &Path,
    session_id: &str,
    kiln: &Path,
    report: &mut MigrationReport,
) {
    let Some(validated) = publishable_id(source, session_id).await else {
        report.skipped += 1;
        return;
    };
    let session_id = validated.as_str();
    let dest = validated.dir_under(sessions_root);
    if !dest.exists() {
        match relocate(source, &dest, session_id, kiln).await {
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
    // Still an id, so still validated: `kiln_tag` is hex and the base was
    // checked above, but the invariant is that nothing reaches the sessions
    // root except through `SessionId`, and a *derived* name is exactly where an
    // invariant like that gets quietly dropped.
    let Ok(suffixed) = SessionId::parse(&format!("{session_id}--{}", kiln_tag(kiln))) else {
        warn!(session_id = %session_id, kiln = %kiln.display(),
              "Disambiguated name is not a usable session id; leaving the session in the kiln");
        report.skipped += 1;
        return;
    };
    let dest = suffixed.dir_under(sessions_root);
    if dest.exists() {
        warn!(session_id = %session_id, kiln = %kiln.display(),
              "Session id already present under both its own and its disambiguated name; leaving it in the kiln");
        report.skipped += 1;
        return;
    }
    match relocate(source, &dest, suffixed.as_str(), kiln).await {
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
/// `id` and `kiln` are stamped into the session's metadata before the move
/// completes — see [`stamp_published_session`] for why both, and why
/// unconditionally.
///
/// `dest` only ever appears complete. The rename path is atomic already; the
/// copy path stages under [`staging_root`] and renames into place, so a process
/// killed mid-copy leaves the source intact and nothing at `dest` — where a
/// partial `dest` would look like a real session to the next pass and push the
/// intact original onto the collision path.
async fn relocate(source: &Path, dest: &Path, id: &str, kiln: &Path) -> std::io::Result<()> {
    let Some(parent) = dest.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} has no parent directory", dest.display()),
        ));
    };
    tokio::fs::create_dir_all(parent).await?;

    // Stamped in the source, before anything moves. Crash after this and the
    // next pass derives the same values from the same directory and kiln and
    // writes them again — idempotent, and never a window where the metadata has
    // been updated for a directory that does not exist yet.
    stamp_published_session(source, id, kiln).await?;

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

/// Overwrite the two things a migrated session may not bring with it: the id it
/// answers to, and the roots its tools run against.
///
/// Migration is the one path that imports a `meta.json` the daemon did not
/// write — a kiln can be shared, synced, or restored from someone else's
/// backup. `publishable_id` rejects the values that are not *usable*; this
/// function replaces the ones that are usable but are not the daemon's to
/// believe. Both fields are stamped unconditionally, on every publish, because
/// "valid" and "true" are different questions and only the second one matters
/// here:
///
/// - **`id`.** `FileSessionStorage` resolves a load by directory name and every
///   write by `session.id`. A crafted directory `chat-evil` whose `meta.json`
///   says `chat-victim` passes every id check ever written — both strings are
///   perfectly good session ids — and then resuming it writes into the victim's
///   directory and carves the victim's transcript out of the read denial. The
///   id is not validated into agreement with the directory, it is *set* to it,
///   which is also what the collision path needed when it filed a session under
///   a suffixed name.
/// - **`kilns` / `workspace`.** These are not identifiers, so no id check would
///   ever have looked at them, but `agent_manager::scope::session_containment`
///   reads them verbatim as the session's default-deny ALLOWLIST. An imported
///   `meta.json` naming `"/"` therefore chose the containment for a session
///   that looks perfectly ordinary in a listing. Migration knows exactly one
///   root about this session — the kiln it just took it out of — so that is
///   what it writes. A stale workspace is a resumed session opening in the
///   wrong directory; an imported one is an allowlist the attacker picked.
///
/// Edited as raw JSON so that fields this build does not know about survive.
async fn stamp_published_session(dir: &Path, id: &str, kiln: &Path) -> std::io::Result<()> {
    // Not `json!(kiln)`: that macro unwraps, and `Path`'s `Serialize` fails on
    // a path that is not valid UTF-8 — which would panic the daemon inside the
    // migration every start does, rather than skipping one session. Converted
    // once, as a value, so the failure joins the ordinary "leave it in the
    // kiln" path.
    let kiln = serde_json::to_value(kiln).map_err(std::io::Error::other)?;
    let mut stamped = 0;
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
        fields.insert("id".to_string(), serde_json::Value::String(id.into()));
        fields.insert(
            "kilns".to_string(),
            serde_json::Value::Array(vec![kiln.clone()]),
        );
        fields.insert("workspace".to_string(), kiln.clone());
        // Overwriting `kilns` is not enough. This function edits raw JSON so
        // that fields this build does not know about survive — and the
        // PRE-flatten `kiln`/`connected_kilns` keys are exactly such fields.
        // `Session::absorb_legacy_kilns` folds them back in *ahead* of `kilns`,
        // so a shared kiln carrying `"kiln": "/"` would out-rank the value we
        // just stamped and choose the containment allowlist. Strip them: the
        // whole point of stamping is that the daemon, not the file, decides
        // what this session reaches.
        fields.remove("kiln");
        fields.remove("connected_kilns");
        tokio::fs::write(&path, serde_json::to_string_pretty(&meta)?).await?;
        stamped += 1;
    }
    if stamped == 0 {
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
mod tests;
