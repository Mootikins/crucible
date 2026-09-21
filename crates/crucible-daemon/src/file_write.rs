//! The write critical section shared by browser RPC and agent note tools.
use crucible_core::config::{read_project_config, ProjectFileAccess};
use crucible_core::file_write::{ExpectedBase, FileChange, FileWriteRequest};
use crucible_core::note_edit::{apply_anchored_edits, disk_hash, AnchoredEdit, EditOutcome};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
static LOCKS: LazyLock<dashmap::DashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>> =
    LazyLock::new(dashmap::DashMap::new);

/// Lock a contained target before reading its current contents. All daemon instances in
/// this process share the map; separate processes must use this daemon's RPC.
pub(crate) async fn lock(path: &Path) -> tokio::sync::OwnedMutexGuard<()> {
    let key = path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|p| p.canonicalize().ok())
            .and_then(|p| path.file_name().map(|name| p.join(name)))
            .unwrap_or_else(|| path.to_path_buf())
    });
    let mutex = LOCKS.entry(key).or_default().clone();
    mutex.lock_owned().await
}

fn failure(kind: &str, message: impl std::fmt::Display) -> Value {
    json!({"ok": false, "failure": kind, "message": message.to_string()})
}

/// One whole-text write in a set that `write_many_for_roots` applies together.
#[derive(Debug, Clone)]
pub struct CheckedPut {
    pub path: String,
    pub content: String,
    pub base: ExpectedBase,
}

/// The change that `write_locked` applies after its base check.
#[derive(Debug, Clone)]
pub(crate) enum LockedChange {
    Put(String),
    Patch(Vec<AnchoredEdit>),
}

/// Check that `raw` is inside a writable root. Return the target path with its
/// nearest existing ancestor resolved, so that the lock key and the write agree.
fn admit(
    raw: &str,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Result<PathBuf, Value> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() || raw.contains("..") || raw.contains('\0') {
        return Err(failure("invalid", "Invalid path: traversal not allowed"));
    }
    let contains = |root: &PathBuf| {
        root.canonicalize()
            .ok()
            .filter(|canonical| path.starts_with(root) || path.starts_with(canonical))
    };
    let root = kilns
        .iter()
        .find_map(contains)
        .map(|p| (p, ProjectFileAccess::ReadWrite))
        .or_else(|| {
            projects
                .iter()
                .find_map(|(p, policy)| contains(p).map(|p| (p, *policy)))
        });
    let Some((root, policy)) = root else {
        return Err(failure(
            "not_found",
            "File not within any open kiln or registered project",
        ));
    };
    if !policy.can_write() {
        return Err(failure(
            if policy.can_read() {
                "forbidden"
            } else {
                "not_found"
            },
            "Project files are read-only",
        ));
    }
    // Resolve the nearest existing ancestor before creating directories, including
    // the final component when it exists, to contain symlinks.
    let mut ancestor = path.as_path();
    while ancestor.symlink_metadata().is_err() {
        let Some(parent) = ancestor.parent() else {
            return Err(failure("invalid", "Path has no parent"));
        };
        ancestor = parent;
    }
    let canonical = match ancestor.canonicalize() {
        Ok(p) if p.starts_with(&root) => p,
        _ => return Err(failure("invalid", "Path escapes kiln directory")),
    };
    let suffix = path.strip_prefix(ancestor).expect("ancestor of target");
    Ok(if suffix.as_os_str().is_empty() {
        canonical
    } else {
        canonical.join(suffix)
    })
}

fn io_failure(e: std::io::Error) -> Value {
    failure(
        if e.kind() == std::io::ErrorKind::NotFound {
            "not_found"
        } else {
            "io"
        },
        e,
    )
}

/// Map the `fs.write` wire fields to a change and a base. The wire format does
/// not change: a text with no hash stays invalid, as it was before `ExpectedBase`.
fn from_wire(change: FileChange) -> Result<(LockedChange, ExpectedBase), Value> {
    Ok(match change {
        FileChange::Put {
            content,
            base_hash,
            base_text,
        } => {
            if base_hash.is_none() && base_text.is_some() {
                return Err(failure("invalid", "base_text does not hash to base_hash"));
            }
            (LockedChange::Put(content), (base_hash, base_text).into())
        }
        FileChange::Patch { edits, base_hash } => {
            (LockedChange::Patch(edits), (base_hash, None).into())
        }
    })
}

/// Resolve registered roots and perform a text write. HTTP contract fixtures can
/// supply isolated roots while exercising the production writer.
pub async fn write_for_roots(
    req: FileWriteRequest,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Value {
    let path = match admit(&req.path, kilns, projects) {
        Ok(path) => path,
        Err(refused) => return refused,
    };
    let (change, base) = match from_wire(req.change) {
        Ok(mapped) => mapped,
        Err(refused) => return refused,
    };
    let _guard = lock(&path).await;
    write_locked(&path, change, base)
        .await
        .unwrap_or_else(io_failure)
}

/// Write several files as one set. Every path must be admitted before any write.
///
/// The function takes the per-path locks in sorted order, so two sets that share
/// paths cannot deadlock. It keeps the disk bytes of each path before it writes.
/// If one write fails, it puts back the kept bytes of every path that it touched,
/// and then releases the locks. The answer is `{"ok": true, "writes": [...]}` in
/// request order, or the failed answer with its `path`.
pub async fn write_many_for_roots(
    requests: Vec<CheckedPut>,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Value {
    let mut paths = Vec::with_capacity(requests.len());
    for request in &requests {
        match admit(&request.path, kilns, projects) {
            Ok(path) => paths.push(path),
            Err(mut refused) => {
                refused["path"] = json!(request.path);
                return refused;
            }
        }
    }
    let mut order: Vec<&PathBuf> = paths.iter().collect();
    order.sort();
    if order.windows(2).any(|pair| pair[0] == pair[1]) {
        // The lock is not reentrant, so a second lock on one path never returns.
        return failure("invalid", "The same path occurs twice in one write set");
    }
    let mut guards = Vec::with_capacity(order.len());
    for path in order {
        guards.push(lock(path).await);
    }
    let mut kept: Vec<Option<Vec<u8>>> = Vec::with_capacity(paths.len());
    for path in &paths {
        match tokio::fs::read(path).await {
            Ok(bytes) => kept.push(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => kept.push(None),
            Err(e) => return io_failure(e),
        }
    }
    let mut answers = Vec::with_capacity(paths.len());
    for (index, (path, request)) in paths.iter().zip(requests).enumerate() {
        let answer = write_locked(path, LockedChange::Put(request.content), request.base)
            .await
            .unwrap_or_else(io_failure);
        if answer["ok"] != true {
            // The failed path is in the restore set too: an I/O error can stop
            // a write after it truncates the file.
            restore(&paths[..=index], &kept[..=index]).await;
            let mut answer = answer;
            answer["path"] = json!(request.path);
            return answer;
        }
        answers.push(answer);
    }
    drop(guards);
    json!({"ok": true, "writes": answers})
}

/// Put back the kept bytes of each path. A path that had no file loses its file.
async fn restore(paths: &[PathBuf], kept: &[Option<Vec<u8>>]) {
    for (path, bytes) in paths.iter().zip(kept) {
        let result = match bytes {
            Some(bytes) => tokio::fs::write(path, bytes).await,
            None => match tokio::fs::remove_file(path).await {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        if let Err(e) = result {
            tracing::error!(path = %path.display(), error = %e, "could not restore a file after a failed write set");
        }
    }
}

/// Check `base` against the disk and apply `change`. The caller holds `lock(path)`.
pub(crate) async fn write_locked(
    path: &Path,
    change: LockedChange,
    base: ExpectedBase,
) -> std::io::Result<Value> {
    let original = match tokio::fs::read_to_string(path).await {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return Ok(failure("unsupported", "File is not UTF-8 text"))
        }
        Err(e) => return Err(e),
    };
    let current_hash = original.as_deref().map(disk_hash).unwrap_or_default();
    if let ExpectedBase::Text { text, hash } = &base {
        if &disk_hash(text) != hash {
            return Ok(failure("invalid", "base_text does not hash to base_hash"));
        }
    }
    // An absent file hashes to `""`. `Absent` expects exactly that, so an empty
    // file does not satisfy it.
    let stale = match &base {
        ExpectedBase::Unchecked => false,
        ExpectedBase::Absent => original.is_some(),
        ExpectedBase::Hash { hash } | ExpectedBase::Text { hash, .. } => hash != &current_hash,
    };
    let mut answer = json!({"ok": true});
    let content = match change {
        LockedChange::Put(content) => {
            if content.len() > MAX_CONTENT_SIZE {
                return Ok(failure("invalid", "Content too large"));
            }
            answer["merged"] = json!(false);
            if stale {
                let (merge_base, always_conflict) = match base {
                    ExpectedBase::Text { text, .. } => (text, false),
                    // The file must not exist, so any file on disk is a conflict.
                    // The merge with an empty base still shows the regions.
                    ExpectedBase::Absent => (String::new(), true),
                    _ => return Ok(json!({"ok": false, "current_hash": current_hash})),
                };
                let disk = original.unwrap_or_default();
                let merge = crucible_core::note_merge::merge3(&merge_base, &content, &disk);
                if always_conflict || !merge.regions.is_empty() {
                    return Ok(
                        json!({"ok": false, "current_hash": current_hash, "current_content": disk,
                        "merged_content": merge.text, "regions": merge.regions, "stale_base": true}),
                    );
                }
                answer["merged"] = json!(true);
                answer["content"] = json!(merge.text);
                merge.text
            } else {
                content
            }
        }
        LockedChange::Patch(edits) => {
            if edits.is_empty() {
                return Ok(failure("invalid", "No edits given"));
            }
            let Some(original) = original else {
                return Ok(failure("not_found", "File not found"));
            };
            if stale {
                return Ok(
                    json!({"ok": false, "current_hash": current_hash, "failed": [], "stale_base": true}),
                );
            }
            match apply_anchored_edits(&original, &edits) {
                EditOutcome::Applied(text) => text,
                EditOutcome::Refused(failed) => {
                    return Ok(
                        json!({"ok": false, "current_hash": current_hash, "failed": failed, "stale_base": false}),
                    )
                }
            }
        }
    };
    if content.len() > MAX_CONTENT_SIZE {
        return Ok(failure("invalid", "Content too large"));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, &content).await?;
    answer["content_hash"] = json!(disk_hash(&content));
    Ok(answer)
}

pub(crate) async fn handle(
    req: crate::protocol::Request,
    km: &crate::kiln_manager::KilnManager,
    pm: &crate::project_manager::ProjectManager,
    sessions: &crate::session_manager::SessionManager,
) -> crate::protocol::Response {
    let params = match crate::rpc_helpers::typed_params::<FileWriteRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    // A project-less session's own workspace folder is writable like a
    // project: it is where that session's work goes. Read-write, because no
    // `.crucible/project.toml` can exist there to say otherwise.
    let session_folder = sessions
        .session_workspace_containing(std::path::Path::new(&params.path))
        .await
        .map(|folder| (folder, ProjectFileAccess::ReadWrite));
    // Registered, not merely open. A restart closes every kiln, and a write
    // admitted against the open set alone answered 404 for a kiln the user
    // can see in `kiln.list`. `admit_kiln_root` also OPENS the one this write
    // lands in, so the bytes it is about to add are watched and indexed.
    let kilns = km.admissible_kiln_roots().await;
    let _opened = km.admit_kiln_root(std::path::Path::new(&params.path)).await;
    let projects = pm
        .list()
        .into_iter()
        .map(|p| {
            let policy = read_project_config(&p.path)
                .map(|c| c.security.project_files)
                .unwrap_or_default();
            (p.path, policy)
        })
        .chain(session_folder)
        .collect::<Vec<_>>();
    crate::protocol::Response::success(req.id, write_for_roots(params, &kilns, &projects).await)
}
