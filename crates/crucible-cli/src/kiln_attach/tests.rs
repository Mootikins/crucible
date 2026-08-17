//! Tests for the CLI's `--kiln` door.
//!
//! Everything here is hermetic by construction: the registry context is built
//! from a `TempDir` rather than read from the environment, so the floor is
//! judged against a data root and a home the test owns. A fixture that let
//! `KilnRegistryContext::for_daemon` read the real `~/.crucible` would pass on
//! CI and fail on a developer's machine — or worse, the other way round.

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

/// A config file plus a loaded config that agree with each other, which is the
/// state every real invocation is in.
fn fixture(tmp: &TempDir, toml: &str) -> (CliAppConfig, PathBuf) {
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, toml).unwrap();
    let config: CliAppConfig = toml::from_str(toml).unwrap();
    (config, config_path)
}

fn registry(tmp: &TempDir, toml: &str) -> (CliKilnRegistry, PathBuf) {
    let (config, config_path) = fixture(tmp, toml);
    let cli = CliKilnRegistry::new(&config, config_path.clone(), context(tmp)).unwrap();
    (cli, config_path)
}

fn written(config_path: &Path) -> CliAppConfig {
    toml::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap()
}

/// The first half of the rule: a value naming an entry the user already has is
/// that kiln, and nothing is written.
#[test]
fn a_registered_name_resolves_without_touching_the_config() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("vault");
    std::fs::create_dir_all(&notes).unwrap();
    let (mut cli, config_path) = registry(&tmp, "[kilns]\nnotes = \"~/vault\"\n");
    let before = std::fs::read_to_string(&config_path).unwrap();

    let attached = cli.attach("notes").unwrap();

    assert_eq!(attached.name, KilnName::parse("notes").unwrap());
    assert_eq!(attached.path, notes);
    assert!(!attached.registered);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        before,
        "resolving an existing name must not rewrite the user's config"
    );
}

/// The second half: a directory registers, under a name derived from its
/// basename, and the entry lands in the one registry file.
#[test]
fn a_directory_auto_registers_under_a_derived_name() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("My Notes");
    std::fs::create_dir_all(&notes).unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    let attached = cli.attach(notes.to_str().unwrap()).unwrap();

    assert_eq!(attached.name, KilnName::parse("my-notes").unwrap());
    assert!(attached.registered);
    assert_eq!(written(&config_path).kilns["my-notes"].path(), notes);
}

/// One registry file, not two: the entry a `--kiln <path>` writes is the same
/// entry `cru kiln register` and a hand edit write, so deleting it actually
/// detaches the kiln and a later run resolves the name rather than adding a
/// second entry for the same directory.
#[test]
fn attaching_the_same_directory_twice_writes_one_entry() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    let first = cli.attach(notes.to_str().unwrap()).unwrap();
    let second = cli.attach(notes.to_str().unwrap()).unwrap();

    assert_eq!(first.name, second.name);
    assert!(first.registered);
    assert!(
        !second.registered,
        "the second attach must recognise the directory, not register it again"
    );
    assert_eq!(written(&config_path).kilns.len(), 1);
}

/// `~` is expanded by the registry, not by the shell, when the flag was
/// quoted. Un-expanded, the entry would be the literal string `~/vault`
/// anchored at whatever directory the daemon happens to be running in.
#[test]
fn a_tilde_path_is_expanded_before_it_is_written() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("home").join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    let attached = cli.attach("~/vault").unwrap();

    assert_eq!(attached.path, vault);
    assert_eq!(written(&config_path).kilns["vault"].path(), vault);
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
    let (mut cli, config_path) = registry(&tmp, "[kilns]\nnotes = \"~/notes\"\n");
    let before = std::fs::read_to_string(&config_path).unwrap();

    let err = cli
        .attach("ntoes")
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
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        before,
        "a refused attach must leave no entry behind"
    );
}

/// The floor is the registry's and the CLI adds none of its own, so this is
/// asserted as a *denial* at the CLI door: the paths five review rounds put
/// behind `refuse_forbidden_scope` are still refused when they arrive through
/// a flag, and none of them leaves an entry.
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
    let (mut cli, config_path) = registry(&tmp, "");

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
            cli.attach(path.to_str().unwrap()).is_err(),
            "{} was accepted through --kiln",
            path.display()
        );
    }
    assert!(
        !config_path.exists() || written(&config_path).kilns.is_empty(),
        "a refused path must write no entry"
    );

    // Without this the assertions above could all be passing because the
    // fixture cannot register anything at all.
    let ok = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&ok).unwrap();
    assert!(cli.attach(ok.to_str().unwrap()).is_ok());
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
    let (mut cli, _) = registry(&tmp, "[kilns]\nnotes = \"~/configured-notes\"\n");

    assert_eq!(
        cli.attach("notes").unwrap().path,
        configured,
        "a registered name must not be shadowed by a directory of the same name"
    );
    assert_eq!(
        cli.attach("./notes").unwrap().path,
        local,
        "`./notes` must always be read as the directory"
    );
}

/// `cru kiln register` names the entry itself and writes the shorthand every
/// hand-written config uses.
#[test]
fn registering_a_name_writes_the_entry_the_user_asked_for() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("home").join("some-directory");
    std::fs::create_dir_all(&dir).unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    let attached = cli.register("work", &dir).unwrap();

    assert_eq!(attached.name, KilnName::parse("work").unwrap());
    assert_eq!(
        written(&config_path).kilns["work"],
        KilnEntry::Path(dir),
        "a name the user chose is written as the plain shorthand"
    );
}

/// Names are case-folded, so `Notes` is refused rather than becoming a second
/// kiln beside `notes` — and the refusal says what a name may be, because
/// "invalid kiln name" alone leaves the user guessing.
#[test]
fn registering_an_out_of_charset_name_is_refused_with_the_rule() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&dir).unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    for bad in ["Notes", "my notes", "../escape", ".hidden", ""] {
        let err = cli
            .register(bad, &dir)
            .expect_err(&format!("{bad:?} must be refused as a kiln name"));
        assert!(
            err.to_string().contains("[a-z0-9._-]"),
            "{bad:?}: the refusal must state the rule, got: {err}"
        );
    }
    assert!(!config_path.exists() || written(&config_path).kilns.is_empty());
}

/// Re-pointing a name at a different directory is refused at the CLI door too,
/// because a session that stored `notes` yesterday would open a different
/// corpus today.
#[test]
fn registering_over_an_existing_name_is_refused() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("home").join("first");
    let second = tmp.path().join("home").join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let (mut cli, config_path) = registry(&tmp, "[kilns]\nnotes = \"~/first\"\n");
    let before = std::fs::read_to_string(&config_path).unwrap();

    let err = cli
        .register("notes", &second)
        .expect_err("a claimed name must not be repointed");
    assert!(err.to_string().contains("notes"), "{err}");
    // Byte-for-byte: the incumbent entry must survive as the user wrote it,
    // `~` and all. Asserting on the *resolved* path would also pass if the
    // refusal had rewritten `~/first` to its expansion.
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
    // And the name still reaches the same directory.
    let (reloaded, _) = registry(&tmp, &before);
    assert_eq!(
        reloaded
            .registry
            .resolve(&KilnName::parse("notes").unwrap())
            .path(),
        Some(first.as_path())
    );
}

/// Registering a directory that does not exist is refused: the entry would be
/// a name resolving to nothing, which is what the whole rule exists to
/// prevent, and a typo'd path is the ordinary way to produce one.
#[test]
fn registering_a_missing_directory_is_refused() {
    let tmp = TempDir::new().unwrap();
    let (mut cli, config_path) = registry(&tmp, "");

    let err = cli
        .register("work", &tmp.path().join("home").join("not-there"))
        .expect_err("a missing directory must not be registered");
    assert!(err.to_string().contains("not a directory"), "{err}");
    assert!(!config_path.exists() || written(&config_path).kilns.is_empty());
}

/// The in-memory half. Attaching writes the file, but the *running* process
/// also has to resolve the name — `session_kiln_name` is what decides which
/// kiln a new session gets, and it reads `session_kiln` against the entries.
/// Updating one without the other silently attaches the default kiln instead
/// of the one the flag named.
#[test]
fn an_attached_kiln_is_the_one_a_new_session_in_this_process_gets() {
    let tmp = TempDir::new().unwrap();
    let notes = tmp.path().join("home").join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let (mut config, config_path) = fixture(&tmp, "[kilns]\nother = \"~/other\"\n");
    let mut cli = CliKilnRegistry::new(&config, config_path, context(&tmp)).unwrap();

    let attached = cli.attach(notes.to_str().unwrap()).unwrap();
    attached.apply_to(&mut config);

    assert_eq!(
        config.session_kiln_name(),
        Some(KilnName::parse("notes").unwrap()),
        "the flag must decide the session's kiln, not the alphabetically-first entry"
    );
}
