//! Registry tests.
//!
//! The floor tests below travelled here with [`refuse_forbidden_scope`] from
//! `server/session/scope.rs`. They are the accumulated output of five review
//! rounds against the attach sites, and the function they pin did not change —
//! only where it is called from. Moving them keeps the evidence next to the
//! code it is evidence about.

use super::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

fn name(s: &str) -> KilnName {
    KilnName::parse(s).expect("test name must be valid")
}

/// A registry context rooted entirely inside `tmp`: a stated base for relative
/// paths, an injected home for `~`, and a data root that is not the
/// developer's. Nothing here reads the environment except
/// `forbidden_root_reason`'s own look at the real home, which no fixture path
/// is inside.
fn context(tmp: &TempDir) -> KilnRegistryContext {
    KilnRegistryContext::new(
        tmp.path().join("cwd"),
        Some(tmp.path().join("home")),
        tmp.path().join("home").join(".crucible"),
    )
}

fn registry(tmp: &TempDir, config: serde_json::Value) -> KilnRegistry {
    KilnRegistry::from_app_config(context(tmp), Some(&config)).expect("registry must build")
}

/// The path an entry resolved to, or a failure naming what it resolved to
/// instead — `Unknown` and "resolved somewhere else" are different bugs.
fn ready_path(registry: &KilnRegistry, kiln: &str) -> PathBuf {
    match registry.resolve(&name(kiln)) {
        KilnResolution::Ready(entry) => entry.path().to_path_buf(),
        other => panic!("expected '{kiln}' to resolve to a ready kiln, got {other:?}"),
    }
}

// ── Building the registry from the config the daemon was handed ──────────

/// The two spellings a user actually types are one entry: `~/vault` in the
/// config, and the expanded absolute path a persisted `meta.json` carries.
/// Un-normalized, the first would be handed to a root builder as the literal
/// string `~/vault` — anchored at the daemon's working directory, reaching
/// nothing — and the second would miss the reverse lookup and register a
/// duplicate.
#[test]
fn a_tilde_path_and_its_expanded_form_are_one_entry() {
    let tmp = TempDir::new().unwrap();
    let registry = registry(&tmp, json!({ "kilns": { "vault": "~/vault" } }));

    let expanded = tmp.path().join("home").join("vault");
    assert_eq!(
        ready_path(&registry, "vault"),
        expanded,
        "the configured `~` must be expanded once, at construction"
    );
    assert_eq!(registry.name_for(&expanded), Some(&name("vault")));
    assert_eq!(
        registry.name_for(Path::new("~/vault")),
        Some(&name("vault"))
    );
}

/// The shipped `create_example` config has no `[kilns]` at all. A registry
/// built from the raw map is empty for it — every name unresolvable, which is
/// the empty-set shape — so it is built from the resolved map instead.
#[test]
fn a_kiln_path_only_config_still_yields_a_named_kiln() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("notes");
    let registry = registry(&tmp, json!({ "kiln_path": notes.to_str().unwrap() }));

    assert_eq!(ready_path(&registry, "default"), notes);
}

/// `lazy` has to survive the trip through the registry: dropping it opens and
/// indexes the entry, and the bundled `crucible-docs` corpus is lazy precisely
/// so Crucible's own documentation cannot turn up in results about your notes.
#[test]
fn a_lazy_entry_resolves_lazily_and_is_never_eager() {
    let tmp = TempDir::new().unwrap();
    let registry = registry(
        &tmp,
        json!({
            "kilns": {
                "vault": tmp.path().join("vault").to_str().unwrap(),
                "archive": { "path": tmp.path().join("archive").to_str().unwrap(), "lazy": true },
            }
        }),
    );

    assert!(matches!(
        registry.resolve(&name("archive")),
        KilnResolution::Lazy(_)
    ));
    let eager: Vec<&KilnName> = registry.eager().map(|kiln| kiln.name()).collect();
    assert!(
        !eager.contains(&&name("archive")),
        "a lazy kiln must not be in the set a startup open touches: {eager:?}"
    );

    // The bundled corpus is injected by `resolve_kiln_entries`, never by the
    // user's `[kilns]` map, so it is only assertable where it exists at all.
    if crucible_core::bundled_docs::bundled_docs_dir().is_some() {
        assert!(
            matches!(
                registry.resolve(&name("crucible-docs")),
                KilnResolution::Lazy(_)
            ),
            "the bundled docs kiln must arrive lazy"
        );
    }
}

/// A relative configured path is anchored at the base the registry was told
/// about, not at whatever directory the daemon happened to be auto-spawned in.
#[test]
fn a_relative_configured_path_is_anchored_at_the_stated_base() {
    let tmp = TempDir::new().unwrap();
    let registry = registry(&tmp, json!({ "kilns": { "notes": "notes" } }));

    assert_eq!(ready_path(&registry, "notes"), tmp.path().join("cwd/notes"));
}

/// `cru init` wrote the directory basename verbatim, so `My Vault` is a key in
/// configs in the wild. It folds and keeps working; it does not take the daemon
/// down.
#[test]
fn an_out_of_charset_config_key_folds_rather_than_aborting() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("vault");
    let registry = registry(
        &tmp,
        json!({ "kilns": { "My Vault": vault.to_str().unwrap() } }),
    );

    assert_eq!(ready_path(&registry, "my-vault"), vault);
}

/// Two keys folding onto one name for two *different* directories is a config
/// only the user can resolve. Picking a winner silently re-points every
/// already-persisted session that named it at a different corpus, which is
/// strictly worse than refusing to start.
#[test]
fn a_folded_name_claimed_by_two_directories_aborts_the_build() {
    let tmp = TempDir::new().unwrap();
    let err = KilnRegistry::from_app_config(
        context(&tmp),
        Some(&json!({
            "kilns": {
                "My Vault": tmp.path().join("alpha").to_str().unwrap(),
                "my-vault": tmp.path().join("bravo").to_str().unwrap(),
            }
        })),
    )
    .expect_err("a name claimed by two directories must not build");

    let RegistryError::Collision { name: n, .. } = &err;
    assert_eq!(n, &name("my-vault"));
    let message = err.to_string();
    assert!(
        message.contains("alpha") && message.contains("bravo"),
        "the abort must name both paths so the user can fix it: {message}"
    );
}

/// The same directory under two spellings of one name is not a collision —
/// there is nothing to choose between.
#[test]
fn two_spellings_of_one_name_for_one_directory_are_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("vault");
    let registry = registry(
        &tmp,
        json!({
            "kilns": {
                "My Vault": vault.to_str().unwrap(),
                "my-vault": vault.to_str().unwrap(),
            }
        }),
    );

    assert_eq!(ready_path(&registry, "my-vault"), vault);
}

/// A registry entry for a directory that does not exist is kept and compared
/// lexically. Dropping it would make a kiln disappear from `cru kiln list` the
/// moment its drive was unmounted; panicking would take the daemon down.
#[test]
fn an_entry_on_a_missing_directory_is_kept() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("not-created-yet");
    let registry = registry(
        &tmp,
        json!({ "kilns": { "vault": missing.to_str().unwrap() } }),
    );

    assert_eq!(ready_path(&registry, "vault"), missing);
    assert_eq!(registry.name_for(&missing), Some(&name("vault")));
}

// ── The floor at the registration door ───────────────────────────────────

/// `config.set {"kilns": {"evil": "/"}}` followed by
/// `session.create {"kilns": ["evil"]}` must resolve **nothing**. Asserted on
/// the denial rather than on the absence of a panic: a name that exists but
/// resolves nowhere is the shape that gets read as "unconstrained" downstream.
#[test]
fn a_forbidden_configured_path_yields_no_entry_and_no_name() {
    let tmp = TempDir::new().unwrap();
    let registry = registry(&tmp, json!({ "kilns": { "evil": "/" } }));

    assert_eq!(registry.resolve(&name("evil")), KilnResolution::Unknown);
    assert_eq!(registry.name_for(Path::new("/")), None);
}

/// An empty registry denies every name. The inverse — an empty set that
/// permits — is the bug this codebase has paid for twice.
#[test]
fn an_empty_registry_denies_every_name() {
    let tmp = TempDir::new().unwrap();
    let registry = KilnRegistry::empty(context(&tmp));

    assert!(registry.is_empty());
    for candidate in ["vault", "default", "crucible-docs"] {
        assert_eq!(
            registry.resolve(&name(candidate)),
            KilnResolution::Unknown,
            "an empty registry must deny '{candidate}'"
        );
    }
    assert_eq!(registry.name_for(tmp.path()), None);
}

/// `app_config: None` is a registry that knows of no kilns — not a registry
/// that falls back to reading the config file the daemon was deliberately not
/// given.
#[test]
fn no_app_config_yields_an_empty_registry() {
    let tmp = TempDir::new().unwrap();
    let registry = KilnRegistry::from_app_config(context(&tmp), None).unwrap();

    assert!(registry.is_empty());
    assert_eq!(registry.resolve(&name("default")), KilnResolution::Unknown);
}

/// The choke point. Every runtime door — `cru chat --kiln <path>`, an RPC that
/// still accepts a path, `meta.json` reverse-resolution — arrives at
/// `register_path`, and a refusal must leave **no entry and no name** rather
/// than a name that resolves to nothing.
#[test]
fn the_floor_refuses_a_runtime_registration_and_mints_no_name() {
    let tmp = TempDir::new().unwrap();
    let mut registry = KilnRegistry::empty(context(&tmp));
    let sessions_root = tmp.path().join("home").join(".crucible").join("sessions");
    let victim = sessions_root.join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();

    let mut forbidden = vec![
        PathBuf::from("/"),
        PathBuf::new(),
        sessions_root.clone(),
        victim.clone(),
        // A `..` through a directory that does not exist: the lexical form is
        // the only one that catches it.
        tmp.path()
            .join("home/.crucible/not-yet/../sessions/chat-victim"),
        // The data root itself, and an ancestor of it.
        tmp.path().join("home").join(".crucible"),
        tmp.path().join("home"),
    ];
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home);
    }

    for path in &forbidden {
        let refused = registry.register_path(path);
        assert!(
            refused.is_err(),
            "{} was accepted as a kiln",
            path.display()
        );
        assert!(
            registry.is_empty(),
            "{} left an entry behind after being refused",
            path.display()
        );
        assert_eq!(
            registry.name_for(path),
            None,
            "{} was refused but still has a name",
            path.display()
        );
    }

    // The precondition the refusals above are meaningless without: an
    // ordinary directory in the same fixture registers.
    let notes = tmp.path().join("home").join("notes");
    assert_eq!(registry.register_path(&notes).unwrap(), name("notes"));
}

/// A basename that folds to nothing must be refused, not turned into an empty
/// key — an empty name is the map-shaped spelling of the empty root that
/// out-ranked every denial.
#[test]
fn a_path_whose_basename_folds_to_nothing_is_refused() {
    let tmp = TempDir::new().unwrap();
    let mut registry = KilnRegistry::empty(context(&tmp));

    for basename in ["...", "\u{2026}", "-"] {
        let path = tmp.path().join(basename);
        assert!(
            registry.register_path(&path).is_err(),
            "{} produced a name",
            path.display()
        );
        assert!(registry.is_empty());
    }
}

/// `docs`, `notes` and `src` repeat constantly and no human chose them. The
/// second one gets a disambiguated name; what it must never get is the
/// incumbent's name, which would point the session at a different corpus with
/// the same basename.
#[test]
fn a_derived_name_collision_disambiguates_and_leaves_the_incumbent_alone() {
    let tmp = TempDir::new().unwrap();
    let mut registry = KilnRegistry::empty(context(&tmp));
    let first = tmp.path().join("a").join("notes");
    let second = tmp.path().join("b").join("notes");

    assert_eq!(registry.register_path(&first).unwrap(), name("notes"));
    assert_eq!(registry.register_path(&second).unwrap(), name("notes-2"));
    assert_eq!(ready_path(&registry, "notes"), first);
    assert_eq!(ready_path(&registry, "notes-2"), second);
}

/// Registering a path that is already registered is a no-op returning the name
/// it already has — including when the caller spells it through a symlink.
#[test]
fn registering_a_known_path_returns_its_existing_name() {
    let tmp = TempDir::new().unwrap();
    let mut registry = KilnRegistry::empty(context(&tmp));
    let notes = tmp.path().join("notes");
    std::fs::create_dir_all(&notes).unwrap();

    let first = registry.register_path(&notes).unwrap();
    assert_eq!(registry.register_path(&notes).unwrap(), first);
    assert_eq!(registry.len(), 1);

    #[cfg(unix)]
    {
        let link = tmp.path().join("notes-link");
        std::os::unix::fs::symlink(&notes, &link).unwrap();
        assert_eq!(
            registry.register_path(&link).unwrap(),
            first,
            "a symlinked spelling of a registered kiln must not become a second entry"
        );
        assert_eq!(registry.len(), 1);
    }
}

/// `cru kiln register <name> <path>` names the entry itself, and the floor is
/// the same one the derived door runs. Stated as the *denial*: a user-supplied
/// name buys no exemption from it, and a refusal leaves no entry and no name.
#[test]
fn a_user_named_registration_runs_the_same_floor() {
    let tmp = TempDir::new().unwrap();
    let mut registry = KilnRegistry::empty(context(&tmp));
    let sessions_root = tmp.path().join("home").join(".crucible").join("sessions");
    let victim = sessions_root.join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();

    let mut forbidden = vec![
        PathBuf::from("/"),
        PathBuf::new(),
        sessions_root,
        victim,
        tmp.path()
            .join("home/.crucible/not-yet/../sessions/chat-victim"),
        tmp.path().join("home").join(".crucible"),
        tmp.path().join("home"),
    ];
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home);
    }

    for path in &forbidden {
        assert!(
            registry.register_named(name("mine"), path).is_err(),
            "{} was accepted as a kiln under a name the user chose",
            path.display()
        );
        assert!(
            registry.is_empty(),
            "{} left an entry behind after being refused",
            path.display()
        );
        assert_eq!(
            registry.resolve(&name("mine")),
            KilnResolution::Unknown,
            "{} was refused but 'mine' still resolves",
            path.display()
        );
    }

    // The precondition without which every refusal above is vacuous.
    let notes = tmp.path().join("home").join("notes");
    registry.register_named(name("mine"), &notes).unwrap();
    assert_eq!(ready_path(&registry, "mine"), notes);
}

/// A name already claimed by a different directory is refused, never
/// re-pointed. Silently repointing is how a session that named `notes`
/// yesterday opens a different corpus today.
#[test]
fn a_user_named_registration_never_repoints_an_existing_name() {
    let tmp = TempDir::new().unwrap();
    let mut registry = registry(&tmp, json!({ "kilns": { "notes": "~/first" } }));
    let first = tmp.path().join("home").join("first");
    let second = tmp.path().join("home").join("second");

    let refused = registry
        .register_named(name("notes"), &second)
        .expect_err("a claimed name must not be repointed");
    assert!(
        refused.to_string().contains("notes"),
        "the refusal must name the name: {refused}"
    );
    assert_eq!(
        ready_path(&registry, "notes"),
        first,
        "the incumbent entry must be untouched"
    );

    // Re-registering the same pair is a no-op, so re-running the command is
    // not an error.
    registry.register_named(name("notes"), &first).unwrap();
    assert_eq!(ready_path(&registry, "notes"), first);
    assert_eq!(
        registry.resolve(&name("second")),
        KilnResolution::Unknown,
        "the refused registration must not have leaked a derived name either"
    );
}

// ── The relocated floor ──────────────────────────────────────────────────

/// `session.connect_kiln {"kiln_path": "/"}` used to reach `km.open("/")`
/// with only a trust check in the way. An open kiln is half of the file
/// API's `resolve_enclosing_root`, so that granted a read scope over the
/// whole filesystem — every credential included — without
/// `project.register` ever being called.
#[test]
fn a_session_kiln_may_not_be_the_filesystem_root_or_home() {
    let sessions_root = Path::new("/nonexistent-sessions-root");
    assert!(refuse_forbidden_scope("kiln", Path::new("/"), sessions_root).is_err());
    assert!(refuse_forbidden_scope("kiln", Path::new("/etc"), sessions_root).is_err());
    if let Some(home) = dirs::home_dir() {
        assert!(refuse_forbidden_scope("kiln", &home, sessions_root).is_err());
    }
}

/// Same door, other handler: `session.set_workspace` (and `session.create`)
/// hand the workspace straight to `register_if_missing`, whose failure was
/// only ever a warning — the session kept the scope regardless.
#[test]
fn a_session_workspace_may_not_be_the_filesystem_root_or_home() {
    let sessions_root = Path::new("/nonexistent-sessions-root");
    assert!(refuse_forbidden_scope("workspace", Path::new("/"), sessions_root).is_err());
    if let Some(home) = dirs::home_dir() {
        assert!(refuse_forbidden_scope("workspace", &home, sessions_root).is_err());
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_forbidden_root_is_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    let link = tmp.path().join("innocent-looking");
    std::os::unix::fs::symlink("/", &link).unwrap();

    assert!(refuse_forbidden_scope("kiln", &link, &tmp.path().join("sessions")).is_err());
}

#[test]
fn an_ordinary_directory_is_allowed_as_scope() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("notes");
    std::fs::create_dir(&kiln).unwrap();
    let sessions_root = tmp.path().join("sessions");

    assert_eq!(
        refuse_forbidden_scope("kiln", &kiln, &sessions_root),
        Ok(())
    );
    assert_eq!(
        refuse_forbidden_scope("workspace", tmp.path(), &sessions_root),
        Ok(())
    );
}

/// Containment is deepest-match-wins, so an allowed root *inside* the
/// denied sessions root beats the denial. Attaching another session's
/// storage directory as a kiln is therefore a way to re-open the subtree
/// the denial exists to close — and the catastrophic-roots floor lets it
/// through, since `~/.crucible/sessions/chat-victim` is neither `/`, home,
/// nor a system tree. Both doors — `session.create` and
/// `session.connect_kiln` — go through this one gate.
#[test]
fn session_storage_may_not_be_attached_as_scope() {
    let tmp = TempDir::new().unwrap();
    let sessions_root = tmp.path().join("sessions");
    let victim = sessions_root.join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();

    assert!(
        refuse_forbidden_scope("kiln", &victim, &sessions_root).is_err(),
        "another session's storage dir must not be attachable as a kiln"
    );
    assert!(
        refuse_forbidden_scope("kiln", &sessions_root, &sessions_root).is_err(),
        "the sessions root itself must not be attachable as a kiln"
    );
    assert!(
        refuse_forbidden_scope("workspace", &victim, &sessions_root).is_err(),
        "the same door via set_workspace must be shut too"
    );
}

/// The rule must survive a symlink, like the rest of the floor: it is
/// decided on the resolved path.
#[cfg(unix)]
#[test]
fn a_symlink_into_the_sessions_root_is_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    let sessions_root = tmp.path().join("sessions");
    let victim = sessions_root.join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();
    let link = tmp.path().join("innocent-notes");
    std::os::unix::fs::symlink(&victim, &link).unwrap();

    assert!(refuse_forbidden_scope("kiln", &link, &sessions_root).is_err());
}

/// An empty path is not a narrow scope, it is no scope: `Path::starts_with("")`
/// is true of every path and `"".components()` counts zero, so an empty
/// root out-ranks every denial at the shallowest possible depth. The
/// builders drop it; the gate must not be the place it gets blessed.
#[test]
fn an_empty_path_is_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    assert!(refuse_forbidden_scope("kiln", Path::new(""), &tmp.path().join("sessions")).is_err());
    assert!(
        refuse_forbidden_scope("workspace", Path::new(""), &tmp.path().join("sessions")).is_err()
    );
}

/// A data home behind a symlink (`/tmp` on macOS, a relocated
/// `~/.crucible` anywhere) gives the sessions root two spellings, and the
/// caller picks which one to name. Judging the caller's spelling against
/// only the resolved root misses the symlinked one entirely — which is how
/// the rule ends up silently never matching.
#[cfg(unix)]
#[test]
fn a_kiln_named_through_a_symlinked_data_home_is_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    let real_home = tmp.path().join("real-home");
    let innocent = tmp.path().join("notes");
    std::fs::create_dir_all(real_home.join("sessions")).unwrap();
    std::fs::create_dir_all(&innocent).unwrap();
    std::os::unix::fs::symlink(&real_home, tmp.path().join("home")).unwrap();
    // The decoy resolves out of the sessions root, so only its NAME —
    // spelled through the symlinked data home — places it there.
    let decoy = tmp.path().join("home").join("sessions").join("chat-decoy");
    std::os::unix::fs::symlink(&innocent, &decoy).unwrap();

    let sessions_root = tmp.path().join("home").join("sessions");
    assert!(
        refuse_forbidden_scope("kiln", &decoy, &sessions_root).is_err(),
        "the sessions-root rule must hold under the spelling the caller used"
    );
}

/// The other direction of the same rule, and the one only the lexical form
/// can catch: a path *named* inside the sessions root that resolves
/// somewhere innocent. Attaching it would put a sessions-root path into
/// the session's allowed roots — where, being deeper than the deny root,
/// it out-ranks the denial — and answering at all tells the caller which
/// session ids exist.
#[cfg(unix)]
#[test]
fn a_symlink_named_inside_the_sessions_root_is_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    let sessions_root = tmp.path().join("sessions");
    let innocent = tmp.path().join("notes");
    std::fs::create_dir_all(&sessions_root).unwrap();
    std::fs::create_dir_all(&innocent).unwrap();
    let decoy = sessions_root.join("chat-decoy");
    std::os::unix::fs::symlink(&innocent, &decoy).unwrap();

    assert!(
        refuse_forbidden_scope("kiln", &decoy, &sessions_root).is_err(),
        "a kiln named inside the sessions root must be refused however it resolves"
    );
}

/// `canonicalize_lenient` re-appends the un-resolved remainder, so a `..`
/// that traverses through a directory which does not exist yet survives
/// into the comparison — and `starts_with` then misses the sessions root
/// the path actually lands in.
#[test]
fn a_traversal_through_a_missing_directory_is_still_refused_as_scope() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path();
    let sessions_root = data_home.join("sessions");
    let victim = sessions_root.join("chat-victim");
    std::fs::create_dir_all(&victim).unwrap();

    let dodge = data_home
        .join("not-yet")
        .join("..")
        .join("sessions")
        .join("chat-victim");
    assert!(
        refuse_forbidden_scope("kiln", &dodge, &sessions_root).is_err(),
        "a `..` through a missing directory dodged the sessions-root refusal: {}",
        dodge.display()
    );
}
