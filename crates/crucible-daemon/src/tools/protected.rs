//! Directories an agent may never write, because the daemon later reads them
//! as instructions or executes them.
//!
//! Containment (see [`super::containment`]) answers "may this session reach
//! this path". It is the wrong question for a whole class of escape, and the
//! class has CVEs against four separate products: the agent writes a file that
//! a **trusted host process later executes unsandboxed**. Containment holds at
//! the write layer and the artifact escapes anyway, because the thing that
//! consumes it was never inside the boundary.
//!
//! Claude Code **CVE-2026-25725** is the exact shape and the reason for the
//! most important rule here: bubblewrap failed to protect
//! `.claude/settings.json` when it *did not yet exist*, so sandboxed code
//! created it and injected `SessionStart` hooks that ran with host privileges
//! on the next start. Sibling instances: Claude Code CVE-2026-55607 (a
//! worktree named `.git` plus git's fsmonitor), Cursor CVE-2026-26268
//! (agent-written git hooks).
//!
//! **We have this.** An agent with write access to a workspace or kiln can
//! write `.crucible/project.toml`, `.crucible/kiln.toml`, a Lua plugin under
//! `.crucible/plugins/`, an agent card, a skill, or a hook — every one of
//! which the daemon loads on its next start, most of which it executes.
//!
//! ## The rules
//!
//! 1. **Write-deny, never read-deny.** Anthropic's documented position on
//!    transcripts generalises: reading a config is ordinary work, writing one
//!    is code execution. A read-deny here would break "explain my plugin" for
//!    no security gain.
//! 2. **Non-existence is not an exemption.** Every check runs on the *name*,
//!    not on what is there — that is precisely what CVE-2026-25725 exploited,
//!    and it is why [`super::path_resolution::absolutize`] is defined on paths
//!    that do not exist.
//! 3. **No configuration can re-open it.** [`write_protection`] takes no
//!    policy argument and is applied by [`super::fs_scope::FsScope`] on every
//!    write including [`super::containment::RootSet::Ambient`] ones, which is
//!    containment switched off. The permission chain above can at most produce
//!    a blanket allow for a *tool call*; it never gets to say what a path is.
//! 4. **The symlink corollary.** Claude Code's: if a symlink appears at a
//!    protected path, the file it POINTS TO is protected too. Without it the
//!    agent creates `.crucible -> ./cfg` once, and every later write to `cfg/`
//!    has no protected name anywhere in it.
//!
//! ## What this does not cover, stated rather than implied
//!
//! - `bash`. A shell command reaches the filesystem through a process this
//!   one does not mediate, so nothing in this module applies to it. That gap
//!   closes with the design's §4 Landlock backstop, not here.
//! - Plugin-provided tools, which reach the filesystem through `crucible-lua`'s
//!   `fs` module rather than through [`super::fs_scope::FsScope`]. A plugin is
//!   already trusted code the daemon executes — it is the thing this module
//!   protects, not a thing it protects against — but a plugin that forwards an
//!   agent-supplied path to `fs.write` re-opens the class, and no type here
//!   stops it.
//! - A symlink *inside* an already-protected directory, planted by someone
//!   other than the agent — `.claude/settings.json -> ../notes/s.json` with
//!   `.claude` itself a real directory. The agent cannot create it (writes
//!   into `.claude/` are refused), so this is a user's own configuration
//!   rather than an escape, and treating it as one would mean canonicalizing
//!   every file under every protected directory on every write.
//! - A symlink at a protected path pointing somewhere the written path does
//!   not pass under. [`link_at_a_protected_path`] asks the candidate's own
//!   ancestors, so `<kiln-a>/.crucible -> <kiln-b>/cfg` followed by a write to
//!   `<kiln-b>/cfg/kiln.toml` is missed: the link is not on the written path.
//!   Closing it would need a registry of every symlink under every allowed
//!   root, refreshed on every write, since the agent can create the link with
//!   one call and use it with the next. It is left open because reaching it
//!   requires creating a symlink, which no file tool can do — only `bash`, and
//!   an agent with `bash` writes the file directly instead. The residual case
//!   is a link a *user* planted, which is their own configuration.
//! - Shell startup files anywhere but the home directory — see
//!   [`SHELL_STARTUP_FILES`]. Deliberate: location is the rule.
//! - Files that are instructions but not executables — `AGENTS.md`,
//!   `CLAUDE.md`, `.rules`. They are read into the prompt, so they are an
//!   injection surface, but editing them is also the single most ordinary
//!   thing an agent is asked to do. They stay writable deliberately.

use super::path_resolution::ResolvedPath;
use std::path::{Path, PathBuf};

/// Directory names that are protected wherever they appear, at any depth.
///
/// Every one of these is a directory whose contents this daemon (or another
/// harness sharing the workspace) loads as configuration, instructions, or
/// code:
///
/// - `.crucible` — `project.toml`, `kiln.toml`, and the `plugins/`, `agents/`,
///   `skills/`, `hooks/`, `tools/`, `themes/` trees. `crucible-core`'s
///   `discovery.rs` states the invariant this relies on: "Every directory
///   Crucible auto-detects is now a `.crucible/` one." One name covers the
///   whole extension surface, and any future extension point lands inside it.
/// - `.git` — hooks, `config` (`core.fsmonitor`, `core.pager`), and the object
///   store. Both Claude Code CVE-2026-55607 and Cursor CVE-2026-26268 live
///   here.
/// - `.claude`, `.codex`, `.opencode`, `.pi` — the other harnesses'. Not
///   paranoia about someone else's product: `skills/discovery.rs` reads
///   `<workspace>/.{claude,codex,opencode}/skills` into *our* prompt, so a
///   write there is an injection into this daemon, and `~/.pi/agent/skills`
///   the same. CVE-2026-25725 was `.claude/settings.json` specifically.
///
/// Matched case-insensitively: on a case-insensitive filesystem `.GIT` and
/// `.git` are the same directory, and a check that only knows one spelling is
/// a check that is bypassed by holding shift.
const PROTECTED_DIRS: &[&str] = &[".crucible", ".git", ".claude", ".codex", ".opencode", ".pi"];

/// Files in the user's home directory that a trusted host process later reads
/// as commands to run, protected **only in the home directory itself**.
///
/// Design §6 names shell rc files, and they are the same class: `bash -c`
/// does not read them, but the user's next interactive terminal does, with
/// the user's privileges. Location is part of the rule rather than an
/// afterthought — a `.bashrc` inside a dotfiles repository is a file no shell
/// reads, and editing one is a perfectly ordinary request. Protecting the
/// name everywhere would refuse that for no gain; protecting the *place* the
/// shell actually reads costs nothing.
const SHELL_STARTUP_FILES: &[&str] = &[
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".bash_logout",
    ".profile",
    ".zshrc",
    ".zshenv",
    ".zprofile",
    ".zlogin",
    // Not a shell rc, same class and the same place: `.git/config` is in
    // `PROTECTED_DIRS` precisely for `core.fsmonitor` and `core.pager`
    // (CVE-2026-55607), and `~/.gitconfig` sets those same keys for EVERY
    // repository on the machine. Protecting the per-repo instance while leaving
    // the global one writable protects nothing.
    ".gitconfig",
];

/// Why a write was refused, kept as a value so the refusal can say something
/// true and a test can assert which rule fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Protection {
    /// The path lies inside a protected directory, under one of its two
    /// resolved forms.
    Inside(&'static str),
    /// The path is a shell startup file in the user's home directory.
    ShellStartup(&'static str),
    /// The path is what a symlink at a protected location points to. Carries
    /// the link, because "your write to `cfg/` is refused" is unintelligible
    /// without it.
    LinkTarget { link: PathBuf },
}

impl Protection {
    /// The message shown to the model, naming the rule and the reason.
    pub(crate) fn message(&self, user_path: &str) -> String {
        match self {
            Self::Inside(dir) => format!(
                "Refusing to write '{user_path}': it is inside a protected `{dir}/` directory. \
                 Crucible and other agent harnesses load configuration, plugins, skills, agent \
                 cards and hooks from there and execute them, so writes are never permitted — no \
                 permission rule or configuration can re-enable them. Reading these paths is \
                 still allowed."
            ),
            Self::ShellStartup(name) => format!(
                "Refusing to write '{user_path}': `~/{name}` is read by a trusted host process — \
                 the user's own shell, or git — which runs commands out of it with the user's \
                 privileges. Writes to these files in the home directory are never permitted. A \
                 copy of the same file inside a dotfiles repository is writable — nothing reads \
                 that one."
            ),
            Self::LinkTarget { link } => format!(
                "Refusing to write '{user_path}': '{}' is a symlink pointing at it, so writing \
                 here writes a protected path indirectly.",
                link.display()
            ),
        }
    }
}

/// Whether a write to `resolved` is refused, and why.
///
/// Judged on both resolved forms, for the reason [`ResolvedPath`] carries two:
/// the lexical form is blind to symlinks, and the canonical form of a path
/// whose parents do not exist is a reconstruction rather than an observation.
/// A name that never mentions `.crucible` can land in it, and a name that does
/// mention it must be refused even when it resolves somewhere innocent —
/// otherwise creating the innocent-looking symlink is the first move.
pub(crate) fn write_protection(resolved: &ResolvedPath) -> Option<Protection> {
    if let Some(dir) = protected_component(resolved.lexical()) {
        return Some(Protection::Inside(dir));
    }
    if let Some(dir) = protected_component(resolved.canonical()) {
        return Some(Protection::Inside(dir));
    }
    let home = dirs::home_dir();
    for form in [resolved.lexical(), resolved.canonical()] {
        if let Some(name) = shell_startup_file(form, home.as_deref()) {
            return Some(Protection::ShellStartup(name));
        }
    }
    link_at_a_protected_path(resolved.canonical()).map(|link| Protection::LinkTarget { link })
}

/// The shell startup file `path` names, if it sits directly in `home`.
///
/// `home` is a parameter rather than a call to `dirs::home_dir()` so a test
/// can point it at a tempdir; the caller supplies the real one. `None` (no
/// home directory on this machine) protects nothing, which is correct — the
/// rule is about the place a shell reads, and there is no such place.
fn shell_startup_file(path: &Path, home: Option<&Path>) -> Option<&'static str> {
    let home = home?;
    if path.parent() != Some(home) {
        return None;
    }
    let name = path.file_name()?;
    SHELL_STARTUP_FILES
        .iter()
        .copied()
        .find(|candidate| name.eq_ignore_ascii_case(candidate))
}

/// The first protected directory name appearing anywhere in `path`.
fn protected_component(path: &Path) -> Option<&'static str> {
    path.components().find_map(|component| {
        PROTECTED_DIRS
            .iter()
            .copied()
            .find(|dir| component.as_os_str().eq_ignore_ascii_case(dir))
    })
}

/// The symlink corollary, computed from the candidate rather than from a
/// registry of links.
///
/// For every ancestor directory of the candidate and every protected name, ask
/// whether that name is a symlink there and whether it lands on (or above) the
/// candidate. Bounded by path depth times the six names, on the write path
/// only.
///
/// The direction matters: a registry built by scanning for symlinks would have
/// to be refreshed on every write to be correct, since the agent can create
/// the link with one call and use it with the next. Asking the filesystem at
/// judgement time cannot go stale.
fn link_at_a_protected_path(candidate: &Path) -> Option<PathBuf> {
    // `skip(1)` because `ancestors()` starts with the candidate itself, and
    // `candidate/.git` is not on the path to `candidate`.
    for ancestor in candidate.ancestors().skip(1) {
        for dir in PROTECTED_DIRS {
            let link = ancestor.join(dir);
            if !link
                .symlink_metadata()
                .is_ok_and(|meta| meta.file_type().is_symlink())
            {
                continue;
            }
            if link
                .canonicalize()
                .is_ok_and(|target| candidate.starts_with(&target))
            {
                return Some(link);
            }
        }
    }
    None
}

/// Roots that are write-denied for every session.
///
/// These are the parts of the design's §6 set that are not expressible as a
/// directory name — the trees the daemon loads plugins, defaults, bundled
/// skills, agent cards and configuration from, at paths that depend on how
/// Crucible was installed and on what the environment and config say:
///
/// - the runtime roots (`~/.config/crucible/runtime`,
///   `<prefix>/share/crucible/runtime`, `<repo>/runtime` in a checkout).
///   `runtime/plugins/*/init.lua` is Lua the daemon executes, and a developer
///   whose workspace IS the Crucible checkout has it inside an allowed root.
/// - `~/.config/crucible` (and `$CRUCIBLE_CONFIG_DIR`), which holds `agents/`,
///   `skills/`, `plugins/` and `config.toml`. `~/.crucible` needs no entry —
///   the name covers it.
/// - whatever `$CRUCIBLE_RUNTIME`, `$CRUCIBLE_PLUGIN_PATH` and the config
///   `runtimepath` add, which is the half this function used to miss entirely:
///   it was built from `runtime_roots::for_current_exe()`, a function that
///   deliberately consults no environment variable, while three loaders layered
///   env vars on top of it.
///
/// Delegated wholesale to [`crate::execution_roots`] rather than rebuilt here,
/// so this is not a second list to keep in step — it is the loaders' own answer.
/// Not memoised for that reason: the config `runtimepath` is not known until
/// the config is read, and a session built before that read must not be
/// protected by less than one built after it.
pub(crate) fn daemon_roots() -> Vec<PathBuf> {
    crate::execution_roots::all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refuses(path: &Path) -> bool {
        write_protection(&ResolvedPath::resolve(path)).is_some()
    }

    /// The name check is on the path, not on the filesystem — a directory that
    /// does not exist yet is the case CVE-2026-25725 turned on.
    #[test]
    fn every_protected_directory_is_refused_before_it_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        for name in PROTECTED_DIRS {
            let path = dir.path().join(name).join("settings.json");
            assert!(!path.exists());
            assert!(refuses(&path), "{} must be refused", path.display());
        }
    }

    /// At any depth, not only at a root. Claude Code CVE-2026-55607 was a
    /// nested `.git`.
    #[test]
    fn a_protected_directory_nested_anywhere_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(refuses(&dir.path().join("a/b/c/.git/hooks/pre-commit")));
        assert!(refuses(
            &dir.path().join("vendor/dep/.crucible/plugins/x.lua")
        ));
        assert!(!refuses(&dir.path().join("a/b/c/notes/note.md")));
    }

    /// A case-insensitive filesystem resolves `.GIT` to the same directory, so
    /// a case-sensitive check is bypassed by holding shift.
    #[test]
    fn a_protected_directory_is_matched_regardless_of_case() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(refuses(&dir.path().join(".GIT").join("config")));
        assert!(refuses(&dir.path().join(".Crucible").join("kiln.toml")));
    }

    /// The corollary, and the reason it is stated as a rule rather than left
    /// to the canonical form: neither form of `cfg/project.toml` mentions a
    /// protected name.
    #[cfg(unix)]
    #[test]
    fn the_target_of_a_symlink_at_a_protected_path_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        let decoy = dir.path().join("cfg");
        std::fs::create_dir_all(&decoy).unwrap();
        std::os::unix::fs::symlink(&decoy, dir.path().join(".crucible")).unwrap();

        assert!(refuses(&decoy.join("project.toml")));
        assert!(refuses(&decoy.join("plugins").join("evil.lua")));
        assert!(
            refuses(&decoy),
            "the link's target directory itself, not only what is under it"
        );
        assert!(!refuses(&dir.path().join("notes").join("note.md")));
    }

    /// The direction only the path AS WRITTEN can catch, and the reason the
    /// lexical form is checked at all.
    ///
    /// With `.crucible` a symlink OUT of the workspace, the canonical form of
    /// `.crucible/kiln.toml` is `/elsewhere/kiln.toml` — no protected
    /// component anywhere in it — and the ancestor scan never looks at the
    /// workspace root, because the resolved path no longer passes through it.
    /// Every check but the lexical one misses this, and it is the shape a
    /// caller reaches by *naming* the protected path directly.
    #[cfg(unix)]
    #[test]
    fn a_path_named_inside_a_protected_directory_is_refused_however_it_resolves() {
        let workspace = tempfile::TempDir::new().unwrap();
        let elsewhere = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), workspace.path().join(".crucible")).unwrap();

        let named = workspace.path().join(".crucible").join("kiln.toml");
        assert_eq!(
            ResolvedPath::resolve(&named).canonical(),
            elsewhere.path().join("kiln.toml"),
            "precondition: the resolved form carries no protected name"
        );
        assert!(refuses(&named));
    }

    /// Shell startup files, and the place-not-name rule that scopes them.
    ///
    /// `bash -c` does not read `~/.bashrc`, so this is not about the `bash`
    /// tool — it is about the user's next terminal, which runs it with the
    /// user's privileges. The same file inside a dotfiles repository is read
    /// by nothing and stays writable, because "edit my dotfiles" is an
    /// ordinary request and refusing it would buy no security.
    #[test]
    fn a_shell_startup_file_is_protected_in_the_home_directory_and_nowhere_else() {
        let home = tempfile::TempDir::new().unwrap();
        let dotfiles = home.path().join("src").join("dotfiles");

        for name in SHELL_STARTUP_FILES {
            assert_eq!(
                shell_startup_file(&home.path().join(name), Some(home.path())),
                Some(*name),
                "~/{name} is executed by the user's shell"
            );
            assert_eq!(
                shell_startup_file(&dotfiles.join(name), Some(home.path())),
                None,
                "a dotfiles-repo copy of {name} is read by no shell"
            );
        }
        assert_eq!(
            shell_startup_file(&home.path().join("notes.md"), Some(home.path())),
            None
        );
        assert_eq!(
            shell_startup_file(&home.path().join(".bashrc"), None),
            None,
            "no home directory means no place a shell reads"
        );
    }

    /// The rule has to be WIRED into [`write_protection`], not merely to
    /// exist — the unit test above passes either way, which is what makes this
    /// one necessary rather than redundant.
    ///
    /// Uses the real home directory because that is the only one
    /// [`write_protection`] consults, and it is safe to: this computes a path
    /// and judges it, touching no filesystem and creating nothing.
    #[test]
    fn the_shell_startup_rule_reaches_write_protection() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        assert!(
            matches!(
                write_protection(&ResolvedPath::resolve(&home.join(".bashrc"))),
                Some(Protection::ShellStartup(".bashrc"))
            ),
            "~/.bashrc must be refused through the same door every write uses"
        );
        assert!(
            !refuses(&home.join("notes.md")),
            "and the home directory itself is not thereby read-only"
        );
    }

    /// `.git/config` is in `PROTECTED_DIRS` precisely for `core.fsmonitor` and
    /// `core.pager` (CVE-2026-55607). `~/.gitconfig` sets those same keys for
    /// every repository on the machine, so protecting the per-repo instance
    /// while leaving the global one writable protects nothing.
    #[test]
    fn the_global_git_config_is_protected_like_a_repositorys_own() {
        let home = tempfile::TempDir::new().unwrap();
        assert_eq!(
            shell_startup_file(&home.path().join(".gitconfig"), Some(home.path())),
            Some(".gitconfig"),
            "~/.gitconfig sets core.fsmonitor and core.pager for every repository"
        );
        assert!(
            refuses(&home.path().join("repo").join(".git").join("config")),
            "precondition: the per-repository instance was already protected"
        );
        assert_eq!(
            shell_startup_file(
                &home.path().join("dotfiles").join(".gitconfig"),
                Some(home.path())
            ),
            None,
            "a dotfiles-repo copy is read by nothing until the user links it"
        );
    }

    /// A real directory at a protected path is not a link, and must not make
    /// its siblings unwritable.
    #[test]
    fn a_real_protected_directory_does_not_protect_its_siblings() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        assert!(!refuses(&dir.path().join("src").join("main.rs")));
    }

    /// Instruction files stay writable. Asserted rather than left implicit,
    /// because "protect everything the model can influence" is the version of
    /// this that makes the agent useless, and the line has to be visible.
    #[test]
    fn instruction_files_outside_a_protected_directory_stay_writable() {
        let dir = tempfile::TempDir::new().unwrap();
        for name in ["AGENTS.md", "CLAUDE.md", ".rules", "README.md"] {
            assert!(!refuses(&dir.path().join(name)));
        }
    }

    /// The install-dependent half of the set. Asserted for shape rather than
    /// exact contents: what `for_current_exe` yields depends on how this test
    /// binary was built, but the config directory is always there and the
    /// runtime roots are always named.
    #[test]
    fn the_daemon_wide_protected_roots_cover_the_runtime_and_config_trees() {
        let roots = daemon_roots();
        assert!(
            roots.iter().any(|root| root.ends_with("runtime")),
            "the runtime tree the daemon loads plugins from must be protected: {roots:?}"
        );
        if let Some(config) = dirs::config_dir() {
            assert!(
                roots.contains(&config.join("crucible")),
                "the personal agent/skill/plugin directory must be protected: {roots:?}"
            );
        }
    }
}
