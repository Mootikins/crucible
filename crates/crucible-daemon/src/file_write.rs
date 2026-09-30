//! The file read and the write critical section shared by browser RPC and
//! agent note tools. One enclosing-root rule admits both.
use base64::Engine as _;
use crucible_core::config::{read_project_config, ProjectFileAccess};
use crucible_core::file_write::{
    ExpectedBase, FileChange, FileContent, FileEncoding, FileReadReply, FileReadRequest,
    FileWriteRequest,
};
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
    /// The whole text. Ignored when `remove` is set.
    pub content: String,
    pub base: ExpectedBase,
    /// Delete the file instead of writing `content`.
    pub remove: bool,
}

/// The change that `write_locked` applies after its base check.
#[derive(Debug, Clone)]
pub(crate) enum LockedChange {
    Put(String),
    Patch(Vec<AnchoredEdit>),
    /// Delete the file. A deletion cannot merge, so a stale base conflicts.
    Remove,
}

/// The innermost of `roots` that holds `path`, as its canonical form and its
/// item. A root matches in its given form or in its canonical form, because
/// clients send either. Roots can nest (a kiln inside a kiln), and the
/// innermost root is the narrower claim, so it wins in each order of `roots`.
pub(crate) fn innermost_root<T>(
    path: &Path,
    roots: impl IntoIterator<Item = T>,
    dir: impl Fn(&T) -> &Path,
) -> Option<(PathBuf, T)> {
    roots
        .into_iter()
        .filter_map(|item| {
            let root = dir(&item);
            let canonical = root.canonicalize().ok()?;
            (path.starts_with(root) || path.starts_with(&canonical)).then_some((canonical, item))
        })
        .max_by_key(|(canonical, _)| canonical.components().count())
}

/// Find the root that holds `raw`, and what that root lets a caller do.
///
/// This is the one enclosing-root rule for every file RPC. A kiln wins over a
/// project, so a kiln inside a project stays read-write. Inside each kind, the
/// innermost root wins.
fn enclosing_root(
    raw: &str,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Result<(PathBuf, ProjectFileAccess), Value> {
    let path = Path::new(raw);
    if !path.is_absolute() || raw.contains("..") || raw.contains('\0') {
        return Err(failure("invalid", "Invalid path: traversal not allowed"));
    }
    innermost_root(path, kilns, |kiln| kiln.as_path())
        .map(|(root, _)| (root, ProjectFileAccess::ReadWrite))
        .or_else(|| {
            innermost_root(path, projects, |(project, _)| project.as_path())
                .map(|(root, (_, policy))| (root, *policy))
        })
        .ok_or_else(|| {
            failure(
                "not_found",
                "File not within any open kiln or registered project",
            )
        })
}

/// The nearest ancestor of `path` that exists, `path` included.
fn nearest_existing(path: &Path) -> Option<&Path> {
    path.ancestors()
        .find(|ancestor| ancestor.symlink_metadata().is_ok())
}

/// Resolve the nearest existing ancestor of `path`, including the final
/// component when it exists, and check that the result stays in `root`. The
/// answer is the target path in that resolved form, so that a symlink cannot
/// take a read or a write out of its root.
pub(crate) fn contain(path: &Path, root: &Path) -> Result<PathBuf, Value> {
    let Some(ancestor) = nearest_existing(path) else {
        return Err(failure("invalid", "Path has no parent"));
    };
    let canonical = match ancestor.canonicalize() {
        Ok(p) if p.starts_with(root) => p,
        _ => return Err(failure("invalid", "Path escapes kiln directory")),
    };
    let suffix = path.strip_prefix(ancestor).expect("ancestor of target");
    Ok(if suffix.as_os_str().is_empty() {
        canonical
    } else {
        canonical.join(suffix)
    })
}

/// Check that `raw` is inside a writable root. Return the target path with its
/// nearest existing ancestor resolved, so that the lock key and the write agree.
/// The contained target path, and the root that contains it — a caller that
/// only needs the path may discard the second element.
fn admit(
    raw: &str,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Result<(PathBuf, PathBuf), Value> {
    let (root, policy) = enclosing_root(raw, kilns, projects)?;
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
    let path = contain(Path::new(raw), &root)?;
    Ok((path, root))
}

/// A `.canvas` write refuses wholesale if a reference in `content` escapes
/// `root` — the same rule [`crucible_core::canvas::containment`] documents,
/// applied here so every caller of `fs.write` gets it, not only the web
/// route that used to run it alone. Content that does not even parse as a
/// canvas is left to the write itself; only a well-formed canvas can name a
/// reference to check.
fn canvas_containment_refusal(path: &Path, content: &str, root: &Path) -> Option<Value> {
    if path.extension().and_then(|e| e.to_str()) != Some("canvas") {
        return None;
    }
    let canvas = crucible_core::canvas::Canvas::parse(content).ok()?;
    let rejected = crucible_core::canvas::containment::validate_canvas(&canvas, root);
    if rejected.is_empty() {
        return None;
    }
    Some(failure(
        "forbidden",
        format!(
            "Canvas references {} file(s) outside the kiln: {}",
            rejected.len(),
            rejected
                .iter()
                .map(|r| format!("node `{}` → {} ({})", r.node_id, r.reference, r.reason))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    ))
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

/// Perform a text write within `kilns` and `projects`. The mock daemon of the
/// web route tests calls it with no root, so it gives the refusal of the real
/// daemon for a path outside every root.
pub async fn write_for_roots(
    req: FileWriteRequest,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Value {
    let (path, root) = match admit(&req.path, kilns, projects) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    if let FileChange::Put { content, .. } = &req.change {
        if let Some(refused) = canvas_containment_refusal(&path, content, &root) {
            return refused;
        }
    }
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
            Ok((path, _root)) => paths.push(path),
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
    let answer = write_many_locked(&paths, requests).await;
    drop(guards);
    answer
}

/// Apply `requests` to `paths`, which are admitted and in the same order, as
/// one set: when one fails, put back the kept bytes of every path touched.
/// The caller holds the lock of every path.
pub(crate) async fn write_many_locked(paths: &[PathBuf], requests: Vec<CheckedPut>) -> Value {
    let mut kept: Vec<Option<Vec<u8>>> = Vec::with_capacity(paths.len());
    for path in paths {
        match tokio::fs::read(path).await {
            Ok(bytes) => kept.push(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => kept.push(None),
            Err(e) => return io_failure(e),
        }
    }
    let mut answers = Vec::with_capacity(paths.len());
    for (index, (path, request)) in paths.iter().zip(requests).enumerate() {
        let path_text = request.path.clone();
        let change = if request.remove {
            LockedChange::Remove
        } else {
            LockedChange::Put(request.content)
        };
        let answer = write_locked(path, change, request.base)
            .await
            .unwrap_or_else(io_failure);
        if answer["ok"] != true {
            // The failed path is in the restore set too: an I/O error can stop
            // a write after it truncates the file.
            restore(&paths[..=index], &kept[..=index]).await;
            let mut answer = answer;
            answer["path"] = json!(path_text);
            return answer;
        }
        answers.push(answer);
    }
    json!({"ok": true, "writes": answers})
}

/// Put back the kept bytes of each path. A path that had no file loses its file.
async fn restore(paths: &[PathBuf], kept: &[Option<Vec<u8>>]) {
    for (path, bytes) in paths.iter().zip(kept) {
        let result = match bytes {
            Some(bytes) => tokio::fs::write(path, bytes).await.map(|()| {
                let hash = disk_hash(&String::from_utf8_lossy(bytes));
                crate::kiln_manager::landed(
                    path,
                    crate::kiln_manager::Landed::Written {
                        hash: &hash,
                        created: false,
                    },
                );
            }),
            None => match tokio::fs::remove_file(path).await {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other.map(|()| {
                    crate::kiln_manager::landed(path, crate::kiln_manager::Landed::Removed)
                }),
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
    let created = original.is_none();
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
        LockedChange::Remove => {
            let Some(disk) = original else {
                return Ok(if stale {
                    json!({"ok": false, "current_hash": current_hash, "stale_base": true})
                } else {
                    failure("not_found", "File not found")
                });
            };
            if stale {
                return Ok(json!({"ok": false, "current_hash": current_hash,
                    "current_content": disk, "merged_content": disk, "regions": [],
                    "stale_base": true}));
            }
            tokio::fs::remove_file(path).await?;
            crate::kiln_manager::landed(path, crate::kiln_manager::Landed::Removed);
            answer["removed"] = json!(true);
            return Ok(answer);
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
    let content_hash = disk_hash(&content);
    // Under the caller's lock, so the index owner gets the writes of one file
    // in the order they landed, without the watcher's echo.
    crate::kiln_manager::landed(
        path,
        crate::kiln_manager::Landed::Written {
            hash: &content_hash,
            created,
        },
    );
    answer["content_hash"] = json!(content_hash);
    Ok(answer)
}

/// The roots that can hold `path`: the admissible kilns, the registered
/// projects with their policies, and the folder of the session that owns
/// `path`.
///
/// Registered kilns, not merely open ones. A restart closes every kiln, and a
/// request admitted against the open set alone answered 404 for a kiln that
/// the user can see in `kiln.list`.
async fn roots_for(
    path: &Path,
    km: &crate::kiln_manager::KilnManager,
    pm: &crate::project_manager::ProjectManager,
    sessions: &crate::session_manager::SessionManager,
) -> (Vec<PathBuf>, Vec<(PathBuf, ProjectFileAccess)>) {
    // A project-less session's own workspace folder is a root like a
    // project: it is where that session's work goes. Read-write, because no
    // `.crucible/project.toml` can exist there to say otherwise. The folder
    // is looked up from the nearest existing ancestor, so that a new file in
    // the folder has a root too.
    let session_folder = match nearest_existing(path) {
        Some(existing) => sessions.session_workspace_containing(existing).await,
        None => None,
    }
    .map(|folder| (folder, ProjectFileAccess::ReadWrite));
    let kilns = km.admissible_kiln_roots().await;
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
    (kilns, projects)
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
    let path = Path::new(&params.path);
    let (kilns, projects) = roots_for(path, km, pm, sessions).await;
    // `admit_kiln_root` OPENS the kiln this write lands in, so the bytes it is
    // about to add are watched and indexed.
    let _opened = km.admit_kiln_root(path).await;
    crate::protocol::Response::success(req.id, write_for_roots(params, &kilns, &projects).await)
}

/// The largest file that `fs.read` answers. The bytes travel in one JSON-RPC
/// line, and base64 makes them a third larger.
const MAX_READ_SIZE: u64 = 64 * 1024 * 1024;

/// Read the file at `path` in `encoding`. `None` when no regular file is there.
async fn read_content(path: &Path, encoding: FileEncoding) -> Result<Option<FileContent>, Value> {
    match tokio::fs::metadata(path).await {
        Ok(meta) if !meta.is_file() => return Ok(None),
        Ok(meta) if meta.len() > MAX_READ_SIZE => {
            return Err(failure("invalid", "File too large to read"))
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_failure(e)),
    }
    let bytes = tokio::fs::read(path).await.map_err(io_failure)?;
    Ok(Some(match encoding {
        FileEncoding::Text => {
            let text = String::from_utf8(bytes)
                .map_err(|_| failure("unsupported", "File is not UTF-8 text"))?;
            FileContent::Text {
                content_hash: disk_hash(&text),
                text,
            }
        }
        FileEncoding::Base64 => FileContent::Base64 {
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        },
    }))
}

/// Read one file through the same enclosing-root rule as the writes. The mock
/// daemon of the web route tests calls it with no root, so it gives the
/// refusal of the real daemon for a path outside every root.
///
/// A root whose policy refuses reads answers `not_found`, as a path in no
/// root does.
pub async fn read_for_roots(
    req: FileReadRequest,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Value {
    let (root, access) = match enclosing_root(&req.path, kilns, projects) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    if !access.can_read() {
        return failure("not_found", "Project files are not served");
    }
    let path = match contain(Path::new(&req.path), &root) {
        Ok(path) => path,
        Err(refused) => return refused,
    };
    let content = match read_content(&path, req.encoding).await {
        Ok(content) => content,
        Err(refused) => return refused,
    };
    let reply = FileReadReply {
        root,
        access,
        path,
        content,
    };
    match serde_json::to_value(reply) {
        Ok(mut answer) => {
            answer["ok"] = json!(true);
            answer
        }
        Err(e) => failure("io", e),
    }
}

/// Handle `fs.read`.
pub(crate) async fn handle_read(
    req: crate::protocol::Request,
    km: &crate::kiln_manager::KilnManager,
    pm: &crate::project_manager::ProjectManager,
    sessions: &crate::session_manager::SessionManager,
) -> crate::protocol::Response {
    let params = match crate::rpc_helpers::typed_params::<FileReadRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (kilns, projects) = roots_for(Path::new(&params.path), km, pm, sessions).await;
    crate::protocol::Response::success(req.id, read_for_roots(params, &kilns, &projects).await)
}
