//! Tests for the CLI's `--kiln` door.
//!
//! Everything here is hermetic by construction: the registry context is built
//! from a `TempDir` rather than read from the environment, so the floor is
//! judged against a data root and a home the test owns. A fixture that let
//! `KilnRegistryContext::for_daemon` read the real `~/.crucible` would pass on
//! CI and fail on a developer's machine — or worse, the other way round.
//!
//! None of these touch a config file any more. The door used to WRITE a
//! `[kilns]` entry, and half these tests read the file back; it now DECIDES,
//! and the registration is the daemon's. So each one asserts the decision —
//! which of the two `KilnTarget` cases, and with what — and the file the
//! registration lands in is asserted where the registration happens.

use super::*;
use tempfile::TempDir;

/// A registry context rooted entirely inside `tmp`, matching the daemon's own
/// fixture: `cwd` anchors relative paths, `home` is what `~` expands to, and
/// the data root is `home/.crucible`.
fn context(tmp: &TempDir) -> KilnRegistryContext {
    KilnRegistryContext::new(
        tmp.path().join("cwd"),
        Some(tmp.path().join("home")),
        tmp.path().join("home").join(".crucible"),
    )
}

fn registry(tmp: &TempDir, toml: &str) -> CliKilnRegistry {
    let config: CliAppConfig = toml::from_str(toml).unwrap();
    CliKilnRegistry::new(&config, context(tmp)).unwrap()
}

/// The name a resolution produced, or a panic naming what came back instead.
fn registered(target: KilnTarget) -> AttachedKiln {
    match target {
        KilnTarget::Registered(attached) => attached,
        KilnTarget::Directory(path) => {
            panic!("expected a registered kiln, got a directory needing one: {path:?}")
        }
    }
}

/// The directory a resolution produced, or a panic naming what came back.
fn directory(target: KilnTarget) -> PathBuf {
    match target {
        KilnTarget::Directory(path) => path,
        KilnTarget::Registered(attached) => {
            panic!(
                "expected a directory needing a name, got {:?}",
                attached.name
            )
        }
    }
}

/// The first half of the rule: a value naming an entry the user already has is
/// that kiln, and no registration is needed.
#[test]
fn a_registered_name_resolves_to_itself() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("vault");
    std::fs::create_dir_all(&notes).unwrap();
    let cli = registry(&tmp, "[kilns]\nnotes = \"~/vault\"\n");

    let attached = registered(cli.resolve("notes").unwrap());

    assert_eq!(attached.name, KilnName::parse("notes").unwrap());
    assert_eq!(attached.path, notes);
    assert!(
        !attached.registered,
        "a name the registry already answers to is not a new registration"
    );
}

/// The second half: a directory with no name comes back as one needing a
/// registration, and the caller makes it.
///
/// The NAME is deliberately not decided here. It depends on what is already
/// registered — `notes`, then `notes-2` — so the registry that will answer to
/// it is the one that picks it, and that registry lives in the daemon.
#[test]
fn an_unregistered_directory_comes_back_needing_a_name() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("My Notes");
    std::fs::create_dir_all(&notes).unwrap();
    let cli = registry(&tmp, "");

    let path = directory(cli.resolve(notes.to_str().unwrap()).unwrap());

    assert_eq!(path, notes, "the resolved path is what gets registered");
}

/// A directory the registry already has a name for is that kiln, named — not a
/// second registration for the same directory.
#[test]
fn a_directory_the_registry_already_names_resolves_to_that_name() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let cli = registry(&tmp, "[kilns]\nnotes = \"~/notes\"\n");

    let attached = registered(cli.resolve(notes.to_str().unwrap()).unwrap());

    assert_eq!(attached.name, KilnName::parse("notes").unwrap());
    assert!(!attached.registered);
}

/// `~` is expanded by the registry, not by the shell, when the flag was
/// quoted. Un-expanded, the path handed to the daemon would be the literal
/// string `~/vault`, anchored at whatever directory the daemon runs in.
#[test]
fn a_tilde_path_is_expanded_before_it_leaves_the_cli() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("home").join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let cli = registry(&tmp, "");

    assert_eq!(directory(cli.resolve("~/vault").unwrap()), vault);
}

/// **The deny.** A bare word that is neither a registered name nor a directory
/// must not become a kiln. Otherwise `--kiln ntoes` mints an entry pointing at
/// nothing, and a name that resolves to nothing is the shape every consumer
/// that reads absence as "unconstrained" is waiting for.
#[test]
fn a_misspelled_name_is_refused_rather_than_registered() {
    let tmp = TempDir::new().unwrap();
    let real = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&real).unwrap();
    let cli = registry(&tmp, "[kilns]\nnotes = \"~/notes\"\n");

    let err = cli
        .resolve("ntoes")
        .expect_err("a name nothing claims, naming no directory, must be refused");

    let message = err.to_string();
    assert!(
        message.contains("ntoes"),
        "the refusal must echo what the caller typed: {message}"
    );
    assert!(
        message.contains("cru kiln register"),
        "the refusal must name the remedy: {message}"
    );
}

/// The floor is the registry's and the CLI adds none of its own, so this is
/// asserted as a *denial* at the CLI door: the paths five review rounds put
/// behind `refuse_forbidden_scope` are still refused when they arrive through
/// a flag, and none of them reaches the daemon.
#[test]
fn the_floor_still_refuses_a_catastrophic_root_through_the_flag() {
    let tmp = TempDir::new().unwrap();
    let sessions = tmp
        .path()
        .join("home")
        .join(".crucible")
        .join("sessions")
        .join("chat-victim");
    std::fs::create_dir_all(&sessions).unwrap();
    let cli = registry(&tmp, "");

    let mut forbidden = vec![
        PathBuf::from("/"),
        tmp.path().join("home"),
        tmp.path().join("home").join(".crucible"),
        sessions,
    ];
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home);
    }

    for path in &forbidden {
        assert!(
            cli.resolve(path.to_str().unwrap()).is_err(),
            "{} was accepted through --kiln",
            path.display()
        );
    }

    // Without this the assertions above could all be passing because the
    // fixture cannot resolve anything at all.
    let ok = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&ok).unwrap();
    assert!(cli.resolve(ok.to_str().unwrap()).is_ok());
}

/// A name beats a same-named directory in the working directory, and `./name`
/// is how the directory is named unambiguously — it can never be read as a
/// name, because a kiln name holds no separator and starts with no dot.
#[test]
fn a_name_wins_over_a_same_named_directory_and_dot_slash_forces_the_path() {
    let tmp = TempDir::new().unwrap();
    let configured = tmp.path().join("home").join("configured-notes");
    let local = tmp.path().join("cwd").join("notes");
    std::fs::create_dir_all(&configured).unwrap();
    std::fs::create_dir_all(&local).unwrap();
    let cli = registry(&tmp, "[kilns]\nnotes = \"~/configured-notes\"\n");

    assert_eq!(
        registered(cli.resolve("notes").unwrap()).path,
        configured,
        "a registered name must not be shadowed by a directory of the same name"
    );
    assert_eq!(
        directory(cli.resolve("./notes").unwrap()),
        local,
        "`./notes` must always be read as the directory"
    );
}

/// The in-memory half. The registration goes to the daemon, but the *running*
/// process also has to resolve the name — `session_kiln_name` is what decides
/// which kiln a new session gets, and it reads `session_kiln` against the
/// entries. Updating one without the other silently attaches the default kiln
/// instead of the one the flag named.
#[test]
fn an_attached_kiln_is_the_one_a_new_session_in_this_process_gets() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let mut config: CliAppConfig = toml::from_str("[kilns]\nother = \"~/other\"\n").unwrap();

    // What the caller builds from the daemon's reply.
    let attached = AttachedKiln {
        name: KilnName::parse("notes").unwrap(),
        path: notes.clone(),
        registered: true,
    };
    attached.apply_to(&mut config);

    assert_eq!(
        config.session_kiln_name(),
        Some(KilnName::parse("notes").unwrap()),
        "the flag must decide the session's kiln, not the alphabetically-first entry"
    );
}
