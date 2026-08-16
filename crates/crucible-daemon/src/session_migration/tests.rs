//! Tests for [`super`] — split out to keep the parent under the
//! 1000-line module budget (`no_new_oversized_modules`).

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
    session.id = SessionId::parse(id).expect("a valid test session id");
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
    let loser = storage
        .load(&SessionId::parse(&suffixed).unwrap())
        .await
        .unwrap();
    assert_eq!(loser.id, suffixed, "persisted id must follow the directory");

    // Exactly what `resume_session_from_storage` does on the way back in.
    storage.save(&loser).await.unwrap();

    assert_eq!(
        storage
            .load(&SessionId::parse("chat-1").unwrap())
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("a"),
        "resuming the disambiguated session overwrote the winner"
    );
    assert_eq!(
        storage
            .load(&SessionId::parse(&suffixed).unwrap())
            .await
            .unwrap()
            .title
            .as_deref(),
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
            .load(&SessionId::parse("chat-1").unwrap())
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

    assert!(relocate(&source, &dest, "chat-1", tmp.path())
        .await
        .is_err());

    assert!(source.join("meta.json").exists());
    assert_eq!(
        tokio::fs::read_to_string(&dest).await.unwrap(),
        "not a directory"
    );
    assert!(!staging_root(tmp.path()).join("dest").exists());
}

/// Migration is how a foreign `meta.json` gets into the sessions root.
/// The kiln it comes from may be shared, synced, or restored from someone
/// else's backup, so its `id` field is not the daemon's own writing — and
/// on the non-collision path it used to be published verbatim, after which
/// `session.cleanup` resolves `{sessions_root}/{that string}` and calls
/// `remove_dir_all` on it.
#[tokio::test]
async fn a_session_whose_persisted_id_traverses_is_left_in_the_kiln() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("shared-vault");
    let root = tmp.path().join("home").join("sessions");
    let mut meta = serde_json::from_str::<serde_json::Value>(&session_meta("chat-1", "a"))
        .expect("a valid session body");
    meta["id"] = serde_json::Value::String("../keys".into());
    seed_session(
        &kiln,
        "chat-1",
        &serde_json::to_string_pretty(&meta).unwrap(),
    )
    .await;

    let report = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

    assert_eq!(
        report,
        MigrationReport {
            skipped: 1,
            ..Default::default()
        },
        "a session with an unusable persisted id must not be published"
    );
    assert!(
        !root.join("chat-1").exists(),
        "the poisoned session was published into the sessions root"
    );
    assert!(
        kiln.join(".crucible")
            .join("sessions")
            .join("chat-1")
            .exists(),
        "the session must stay where it is, not be destroyed"
    );
}

/// A session is named by the directory it lives in. `meta.json` carries an
/// `id` too, and every WRITE resolves against that field rather than
/// against the directory the session was loaded from — so a `meta.json`
/// naming a *different* session turns a load of one directory into a write
/// to another. Migration is the door: a shared or synced kiln's
/// `meta.json` is not the daemon's own writing, and both strings arrive
/// from outside.
#[tokio::test]
async fn a_migrated_session_cannot_name_another_sessions_directory() {
    use crate::session_manager::SessionManager;
    use std::sync::Arc;

    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("shared-vault");
    let root = tmp.path().join("home").join("sessions");

    // A real session already filed in the root.
    let storage = FileSessionStorage::new(root.clone());
    let victim: Session = serde_json::from_str(&session_meta("chat-victim", "victim")).unwrap();
    storage.save(&victim).await.unwrap();

    // The crafted kiln: directory `chat-evil`, persisted id `chat-victim`,
    // and a kiln set naming the filesystem root — `kilns` and `workspace`
    // ARE the containment allowlist a session is rebuilt with.
    let mut crafted: serde_json::Value =
        serde_json::from_str(&session_meta("chat-victim", "pwned")).unwrap();
    crafted["kilns"] = serde_json::json!(["/"]);
    crafted["workspace"] = serde_json::json!("/");
    seed_session(
        &kiln,
        "chat-evil",
        &serde_json::to_string_pretty(&crafted).unwrap(),
    )
    .await;

    migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

    // Whatever migration decided, resuming what it published must not
    // reach a directory other than its own.
    let sm = SessionManager::with_storage(Arc::new(storage.clone()));
    let resumed = sm
        .resume_session_from_storage(&SessionId::parse("chat-evil").unwrap())
        .await;

    // Containment is derived from the loaded session: its `storage_path`
    // (the read carve-out) and its `kilns`/`workspace` (the allowlist).
    if let Ok(resumed) = &resumed {
        let roots = crate::agent_manager::scope::session_containment(resumed, &root);
        let scope = crate::tools::fs_scope::FsScope::workspace(PathBuf::new(), roots);
        let victim_log = root.join("chat-victim").join("session.jsonl");
        assert!(
            scope.resolve(&victim_log.to_string_lossy()).is_err(),
            "resuming one session carved out another session's transcript for reading"
        );
    }

    let after = storage
        .load(&SessionId::parse("chat-victim").unwrap())
        .await
        .unwrap();
    assert_eq!(
        after.title.as_deref(),
        Some("victim"),
        "loading one session's directory overwrote another session's meta.json"
    );
    assert_ne!(
        after.kilns,
        vec![PathBuf::from("/")],
        "the victim's containment allowlist was rewritten from a foreign kiln"
    );
}

/// `kilns` and `workspace` in a `meta.json` are not identifiers, but they
/// are the DEFAULT-DENY ALLOWLIST a resumed session's tools run under —
/// `session_containment` reads them verbatim. Migration imports them from
/// a kiln the daemon did not write, so a session that looks ordinary in a
/// listing can carry a scope root of `/`.
#[tokio::test]
async fn a_migrated_sessions_scope_is_not_taken_from_the_file_it_arrived_in() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("shared-vault");
    let root = tmp.path().join("home").join("sessions");

    let mut crafted: serde_json::Value =
        serde_json::from_str(&session_meta("chat-import", "looks ordinary")).unwrap();
    crafted["kilns"] = serde_json::json!(["/"]);
    crafted["workspace"] = serde_json::json!("/");
    seed_session(
        &kiln,
        "chat-import",
        &serde_json::to_string_pretty(&crafted).unwrap(),
    )
    .await;

    migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

    let storage = FileSessionStorage::new(root.clone());
    let Ok(imported) = storage
        .load(&SessionId::parse("chat-import").unwrap())
        .await
    else {
        return; // migration refused it — nothing to escape with
    };
    let roots = crate::agent_manager::scope::session_containment(&imported, &root);
    let scope = crate::tools::fs_scope::FsScope::workspace(PathBuf::new(), roots);

    assert!(
        scope.resolve("/etc/shadow").is_err(),
        "an imported meta.json chose the containment allowlist"
    );
}

/// The sibling of the test above, through the door the stamp does not
/// close. `stamp_published_session` overwrites `kilns` and `workspace`, but
/// it edits raw JSON deliberately so unknown fields survive — and the
/// PRE-flatten `kiln` / `connected_kilns` keys are exactly such fields.
/// `absorb_legacy_kilns` then merges them back in *ahead* of the stamped
/// value, so a crafted legacy key wins over the daemon's own decision.
#[tokio::test]
async fn a_migrated_sessions_scope_is_not_taken_from_its_pre_flatten_keys() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("shared-vault");
    let root = tmp.path().join("home").join("sessions");

    let mut crafted: serde_json::Value =
        serde_json::from_str(&session_meta("chat-legacy", "looks ordinary")).unwrap();
    // The keys a file written before the flatten would carry — and which
    // the stamp leaves untouched.
    crafted.as_object_mut().unwrap().remove("kilns");
    crafted["kiln"] = serde_json::json!("/");
    crafted["connected_kilns"] = serde_json::json!(["/etc"]);
    seed_session(
        &kiln,
        "chat-legacy",
        &serde_json::to_string_pretty(&crafted).unwrap(),
    )
    .await;

    migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

    let storage = FileSessionStorage::new(root.clone());
    let Ok(imported) = storage
        .load(&SessionId::parse("chat-legacy").unwrap())
        .await
    else {
        return; // migration refused it — nothing to escape with
    };
    let roots = crate::agent_manager::scope::session_containment(&imported, &root);
    let scope = crate::tools::fs_scope::FsScope::workspace(PathBuf::new(), roots);

    assert!(
        scope.resolve("/etc/shadow").is_err(),
        "a pre-flatten `kiln` key survived the stamp and chose the allowlist"
    );
}

/// The directory name is joined onto the sessions root too. `read_dir`
/// never yields `.` or `..`, but it does yield whatever a shared kiln
/// happens to contain.
#[tokio::test]
async fn a_legacy_directory_whose_name_is_not_a_path_component_is_left_alone() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("shared-vault");
    let root = tmp.path().join("home").join("sessions");
    seed_session(&kiln, ".migrating", &session_meta("chat-1", "a")).await;

    let report = migrate_sessions(&root, std::slice::from_ref(&kiln)).await;

    assert_eq!(
        report,
        MigrationReport {
            skipped: 1,
            ..Default::default()
        }
    );
    assert!(!root.join(".migrating").exists());
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
