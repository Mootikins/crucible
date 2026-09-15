//! The write critical section shared by browser RPC and agent note tools.
use crucible_core::config::{read_project_config, ProjectFileAccess};
use crucible_core::file_write::{FileChange, FileWriteRequest};
use crucible_core::note_edit::{apply_anchored_edits, disk_hash, EditOutcome};
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

/// Resolve registered roots and perform a text write. HTTP contract fixtures can
/// supply isolated roots while exercising the production writer.
pub async fn write_for_roots(
    req: FileWriteRequest,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Value {
    let path = PathBuf::from(&req.path);
    if !path.is_absolute() || req.path.contains("..") || req.path.contains('\0') {
        return failure("invalid", "Invalid path: traversal not allowed");
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
        return failure(
            "not_found",
            "File not within any open kiln or registered project",
        );
    };
    if !policy.can_write() {
        return failure(
            if policy.can_read() {
                "forbidden"
            } else {
                "not_found"
            },
            "Project files are read-only",
        );
    }
    // Resolve the nearest existing ancestor before creating directories, including
    // the final component when it exists, to contain symlinks.
    let mut ancestor = path.as_path();
    while ancestor.symlink_metadata().is_err() {
        let Some(parent) = ancestor.parent() else {
            return failure("invalid", "Path has no parent");
        };
        ancestor = parent;
    }
    let canonical = match ancestor.canonicalize() {
        Ok(p) if p.starts_with(&root) => p,
        _ => return failure("invalid", "Path escapes kiln directory"),
    };
    let suffix = path.strip_prefix(ancestor).expect("ancestor of target");
    let path = if suffix.as_os_str().is_empty() {
        canonical
    } else {
        canonical.join(suffix)
    };
    let _guard = lock(&path).await;
    match write_locked(&path, req.change).await {
        Ok(answer) => answer,
        Err(e) => failure(
            if e.kind() == std::io::ErrorKind::NotFound {
                "not_found"
            } else {
                "io"
            },
            e,
        ),
    }
}

async fn write_locked(path: &Path, change: FileChange) -> std::io::Result<Value> {
    let original = match tokio::fs::read_to_string(path).await {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return Ok(failure("unsupported", "File is not UTF-8 text"))
        }
        Err(e) => return Err(e),
    };
    let current_hash = original.as_deref().map(disk_hash).unwrap_or_default();
    let mut answer = json!({"ok": true});
    let content = match change {
        FileChange::Put {
            content,
            base_hash,
            base_text,
        } => {
            if content.len() > MAX_CONTENT_SIZE {
                return Ok(failure("invalid", "Content too large"));
            }
            if base_text
                .as_ref()
                .is_some_and(|text| Some(disk_hash(text)).as_ref() != base_hash.as_ref())
            {
                return Ok(failure("invalid", "base_text does not hash to base_hash"));
            }
            answer["merged"] = json!(false);
            if base_hash.as_ref().is_some_and(|base| base != &current_hash) {
                let Some(base) = base_text else {
                    return Ok(json!({"ok": false, "current_hash": current_hash}));
                };
                let disk = original.unwrap_or_default();
                let merge = crucible_core::note_merge::merge3(&base, &content, &disk);
                if !merge.regions.is_empty() {
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
        FileChange::Patch { edits, base_hash } => {
            if edits.is_empty() {
                return Ok(failure("invalid", "No edits given"));
            }
            let Some(original) = original else {
                return Ok(failure("not_found", "File not found"));
            };
            if base_hash.as_ref().is_some_and(|base| base != &current_hash) {
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
) -> crate::protocol::Response {
    let params = match crate::rpc_helpers::typed_params::<FileWriteRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kilns = km
        .list()
        .await
        .into_iter()
        .map(|(path, _, _)| path)
        .collect::<Vec<_>>();
    let projects = pm
        .list()
        .into_iter()
        .map(|p| {
            let policy = read_project_config(&p.path)
                .map(|c| c.security.project_files)
                .unwrap_or_default();
            (p.path, policy)
        })
        .collect::<Vec<_>>();
    crate::protocol::Response::success(req.id, write_for_roots(params, &kilns, &projects).await)
}
