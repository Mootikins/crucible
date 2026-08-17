//! The kiln registry: the one place a filesystem path becomes a kiln.
//!
//! A kiln is addressed by [`KilnName`] everywhere else in the daemon. This
//! module owns the only mapping from a name to a directory, and — more
//! importantly — the only door in the other direction: a *path* becomes a kiln
//! here, or it does not become one at all.
//!
//! # The floor lives here
//!
//! [`refuse_forbidden_scope`] is not new. It was the gate at each attach site
//! (`session.create`, `session.connect_kiln`, `session.set_workspace`, a
//! revived `meta.json`), and five review rounds' worth of cases are pinned by
//! its tests. Names do not delete it — they **move it**, from every attach site
//! to this one registration site, because once `Session.kilns` holds names the
//! attach sites no longer have a `&Path` to check.
//!
//! Every path entering the registry runs that floor plus the data-root check
//! *before* an entry is written, and a refused path yields **no entry and no
//! name** — never a name that resolves to nothing. That distinction is the
//! whole point: a name that exists but resolves nowhere is a permit waiting for
//! a consumer that reads absence as "unconstrained", and this codebase has paid
//! for that shape more than once (an empty root out-ranks every denial, because
//! `Path::starts_with("")` is true of everything).
//!
//! # Normalization happens once, here, in both directions
//!
//! `KilnEntry::path()` returns the configured string verbatim — `~/vault` stays
//! `~/vault`, which its own test pins. Un-normalized, a resolution hands a root
//! spelled `~/vault` to a containment builder that anchors it at the daemon's
//! working directory, and the session silently reaches nothing; symmetrically a
//! reverse lookup for the expanded path misses and registers a duplicate. So
//! `~` expansion and relative-path anchoring happen at construction, entries
//! are stored absolute, and every comparison is normalized-against-normalized.
//!
//! Resolution is the same lenient two-form resolution containment uses
//! ([`ResolvedPath`]), not `std::fs::canonicalize`: a registry entry for a
//! directory that does not exist yet is kept and compared lexically rather than
//! disappearing.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crucible_core::config::{resolve_kiln_entries, KilnEntry, KilnName};
use tracing::{debug, warn};

use crate::project_manager::{forbidden_root_reason, resolve_registration_root};
use crate::session_storage::FileSessionStorage;
use crate::tools::path_resolution::ResolvedPath;

/// How many `-2`, `-3`, … suffixes a derived name may try before the
/// registration is refused outright.
///
/// It is refused rather than resolved to the incumbent entry: `docs` and
/// `notes` repeat constantly, and quietly handing back the entry that already
/// owns the name points the session at a *different corpus of the same
/// basename*.
const MAX_DERIVED_SUFFIX: u32 = 99;

/// Daemon-side floor for a caller-supplied directory that becomes session
/// scope — a kiln (indexed, then served by the file API through `kiln.list`)
/// or a workspace (the containment boundary for the agent's file tools).
///
/// The RPC socket has no authentication, so this is enforced here and not left
/// to whichever client happened to make the call: `session.connect_kiln
/// {"kiln_path": "/"}` otherwise granted the whole filesystem as a read scope
/// without `project.register` ever being involved.
///
/// This is only the floor ([`forbidden_root_reason`] — catastrophic for every
/// caller). Narrower policy for untrusted callers belongs at that caller's
/// boundary; the web routes layer their own on top before reaching this.
///
/// `sessions_root` is refused on top of that floor, and it is not a "root that
/// encloses everything" rule — it is the opposite. Tool containment is
/// deepest-match-wins, so an allowed root *inside* the denied sessions root
/// beats the denial: attaching `~/.crucible/sessions/chat-victim` as a kiln (or
/// setting it as a workspace) hands the agent exactly the transcript the deny
/// root exists to close, and the catastrophic-roots floor waves it through
/// because it is neither `/`, home, nor a system tree.
pub(crate) fn refuse_forbidden_scope(
    kind: &str,
    path: &Path,
    sessions_root: &Path,
) -> Result<(), String> {
    // Before resolution, because resolution anchors a relative path at the
    // working directory and `""` would become the daemon's cwd — an ordinary
    // directory that clears the floor. The pre-resolution form refused `""`
    // for an accidental reason (`Path::new("").parent()` is `None`, which
    // `forbidden_root_reason` reads as the filesystem root); it is refused
    // here on purpose, because an empty path names nothing and every builder
    // downstream treats it as a universal root.
    if path.as_os_str().is_empty() {
        return Err(format!(
            "Refusing an empty path as a session {kind}: it names no directory"
        ));
    }

    // Decided on BOTH resolved forms of the path — the `..`-clamped lexical
    // one and the one walked back through its deepest existing ancestor.
    //
    // The lexical form is what closes `{data}/not-yet/../sessions/{victim}`:
    // nothing on that path needs to exist for it to name a transcript, and a
    // resolution that only understands existing directories hands it back with
    // the `..` still in it. The resolved form is what closes a symlink
    // presenting an innocent name for a forbidden target. Neither subsumes the
    // other, and this is a *denial*, so any form landing somewhere forbidden
    // refuses — the broad match is the fail-closed one here.
    //
    // The sessions root gets the same treatment, or a data home behind a
    // symlink (`/tmp` on macOS) would make that rule silently never match.
    let resolved = ResolvedPath::resolve(path);
    for candidate in [resolved.lexical(), resolved.canonical()] {
        if let Some(why) = forbidden_root_reason(candidate, dirs::home_dir().as_deref()) {
            return Err(format!(
                "Refusing '{}' as a session {kind}: {why}",
                candidate.display()
            ));
        }
    }

    // An unset sessions root is skipped rather than resolved: resolution
    // anchors a relative path at the working directory, which would turn `""`
    // into "refuse everything under the daemon's cwd".
    if sessions_root.as_os_str().is_empty() {
        return Ok(());
    }
    let sessions_root = ResolvedPath::resolve(sessions_root);
    for candidate in [resolved.lexical(), resolved.canonical()] {
        for root in [sessions_root.lexical(), sessions_root.canonical()] {
            if candidate.starts_with(root) {
                return Err(format!(
                    "Refusing '{}' as a session {kind}: it is inside the session storage root, \
                     which holds every recorded transcript",
                    candidate.display()
                ));
            }
        }
    }
    Ok(())
}

/// The anchors and denials a registry is built against.
///
/// All four are values rather than process globals for the reason the rest of
/// the daemon threads `data_home`: a registry that read the environment would
/// resolve the developer's real `~/.crucible` in every test, and the floor
/// would be judged against a root no test controls.
#[derive(Debug, Clone)]
pub struct KilnRegistryContext {
    /// Anchor for a configured path that is *relative*. Stated rather than
    /// implied: `ResolvedPath::resolve` would anchor at the process working
    /// directory, which for a daemon is wherever the client happened to be
    /// standing when it auto-spawned. Naming it makes that choice reviewable
    /// and lets a test pin it.
    base: PathBuf,
    /// Home directory for `~` expansion. `None` leaves a `~` prefix
    /// unexpanded, which then fails the floor's own resolution rather than
    /// landing somewhere surprising — the same contract as
    /// [`resolve_registration_root`].
    home: Option<PathBuf>,
    /// Daemon data root. Refused as a kiln, along with every ancestor of it.
    data_home: PathBuf,
    /// Derived from `data_home` rather than passed in, so the registry's idea
    /// of the sessions root cannot drift from the storage layer's.
    sessions_root: PathBuf,
}

impl KilnRegistryContext {
    /// The production shape: relative configured paths anchored at the
    /// daemon's working directory, `~` against the real home.
    ///
    /// Reading either is the composition root's job and nobody else's — a
    /// registry built anywhere downstream takes them as values, so a test can
    /// hand it a home and a data root it owns.
    pub fn for_daemon(data_home: PathBuf) -> Self {
        Self::new(
            std::env::current_dir().unwrap_or_else(|_| data_home.clone()),
            dirs::home_dir(),
            data_home,
        )
    }

    pub fn new(base: PathBuf, home: Option<PathBuf>, data_home: PathBuf) -> Self {
        let sessions_root = FileSessionStorage::root_for(&data_home);
        Self {
            base,
            home,
            data_home,
            sessions_root,
        }
    }
}

/// A kiln the registry knows about: a name, an absolute path, and whether it
/// may be opened without being asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredKiln {
    name: KilnName,
    /// `~`-expanded, anchored, `..`-clamped. This is the spelling pinned
    /// alongside the name in a session's `meta.json`.
    path: PathBuf,
    /// The same path walked back through its deepest existing ancestor, so a
    /// kiln opened under a symlinked spelling still matches.
    resolved: PathBuf,
    lazy: bool,
}

impl RegisteredKiln {
    pub fn name(&self) -> &KilnName {
        &self.name
    }

    /// The absolute, un-symlink-resolved path. What a session pins.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The path after following symlinks through the deepest existing
    /// ancestor. What an opener should actually open.
    pub fn resolved_path(&self) -> &Path {
        &self.resolved
    }

    pub fn lazy(&self) -> bool {
        self.lazy
    }
}

/// What a name resolves to.
///
/// A variant rather than `Option<&Path>` so a caller cannot lose the `lazy`
/// bit by accident: dropping it opens and indexes the bundled `crucible-docs`
/// corpus, which is exactly what marking it lazy exists to prevent — Crucible's
/// own documentation turning up in every search about your notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KilnResolution<'a> {
    /// Registered and eager: safe to open, index and search.
    Ready(&'a RegisteredKiln),
    /// Registered, but must not be opened until explicitly asked for.
    Lazy(&'a RegisteredKiln),
    /// No such entry. **Not a kiln**: no root, no classification, no search
    /// source, no prompt line.
    Unknown,
}

impl<'a> KilnResolution<'a> {
    /// The entry behind a name, eager or lazy — for the consumers that need to
    /// know a name *is* a kiln (containment, classification, the reverse path
    /// a save writes) rather than whether it may be opened unasked.
    ///
    /// `None` is [`Self::Unknown`], and it is a denial. Callers that would
    /// *open* or *index* the kiln must branch on the variant instead, or a
    /// lazy entry's whole point is lost.
    pub fn registered(self) -> Option<&'a RegisteredKiln> {
        match self {
            Self::Ready(kiln) | Self::Lazy(kiln) => Some(kiln),
            Self::Unknown => None,
        }
    }

    /// The path a name resolves to, or `None` when it resolves to nothing.
    pub fn path(self) -> Option<&'a Path> {
        self.registered().map(RegisteredKiln::path)
    }
}

/// A registration the floor refused. Carries the reason, which names the path
/// the *caller supplied* and is therefore safe to echo back to that caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RegistrationRefused(String);

/// A registry that cannot be built, because acting on it would need Crucible to
/// guess which of two corpora the user meant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    // Names both paths and the fix. A collision the user cannot locate is a
    // daemon that will not start for reasons they cannot act on.
    #[error(
        "kiln name '{name}' is claimed by two different directories — '{first}' and '{second}'. \
         Rename one of the `[kilns]` entries in your config and start the daemon again."
    )]
    Collision {
        name: KilnName,
        first: String,
        second: String,
    },
}

/// Name → kiln, and the floor every path crosses to get in here.
#[derive(Debug)]
pub struct KilnRegistry {
    /// Ordered so iteration — and therefore every diagnostic built from it —
    /// is stable.
    entries: BTreeMap<KilnName, RegisteredKiln>,
    /// Reverse index, holding **both** spellings of each entry's path. A kiln
    /// opened through a symlink must still match the name it was registered
    /// under.
    by_path: HashMap<PathBuf, KilnName>,
    ctx: KilnRegistryContext,
}

impl KilnRegistry {
    /// A registry with no entries — which denies every name.
    ///
    /// `app_config: None` lands here, and that is the intended answer, not a
    /// degraded one: a daemon handed no config knows of no kilns, so every
    /// name is unresolvable and no session gets scope it cannot justify.
    pub fn empty(ctx: KilnRegistryContext) -> Self {
        Self {
            entries: BTreeMap::new(),
            by_path: HashMap::new(),
            ctx,
        }
    }

    /// Build from the config JSON the daemon was **handed**
    /// (`BindWithPluginConfigParams::app_config`).
    ///
    /// Not `crucible_home()`, not `CliAppConfig::default_config_path()`, and
    /// not the live Lua store: the daemon never loads a `CliAppConfig` of its
    /// own, and a registry that re-read the config file would disagree with
    /// the client that spawned it (and with every test that injects one).
    /// `execution_roots.rs` reads the file directly and is the in-repo
    /// precedent for the wrong choice.
    ///
    /// The entries come from [`resolve_kiln_entries`], not the raw `kilns`
    /// map, so a `kiln_path`-only config — the shape `create_example` ships —
    /// still yields a named kiln, and the bundled `crucible-docs` entry keeps
    /// its `lazy` flag.
    pub fn from_app_config(
        ctx: KilnRegistryContext,
        app_config: Option<&serde_json::Value>,
    ) -> Result<Self, RegistryError> {
        let mut registry = Self::empty(ctx);
        let Some(app_config) = app_config else {
            debug!("No app config: the kiln registry is empty and every name unresolvable");
            return Ok(registry);
        };

        let view = match serde_json::from_value::<KilnConfigView>(app_config.clone()) {
            Ok(view) => view,
            Err(e) => {
                warn!(error = %e, "Could not read the kiln section of the app config; no kilns registered");
                return Ok(registry);
            }
        };

        // Sorted, so which entry wins the reverse index for an aliased path —
        // and which pair a collision names — does not depend on hash order.
        let entries: BTreeMap<String, KilnEntry> =
            resolve_kiln_entries(&view.kiln_path, &view.kilns)
                .into_iter()
                .collect();
        for (key, entry) in entries {
            registry.insert_configured(&key, &entry.path(), entry.lazy())?;
        }
        Ok(registry)
    }

    /// Resolve a name. [`KilnResolution::Unknown`] is a **denial**, and every
    /// consumer must treat it as one.
    pub fn resolve(&self, name: &KilnName) -> KilnResolution<'_> {
        match self.entries.get(name) {
            None => KilnResolution::Unknown,
            Some(kiln) if kiln.lazy => KilnResolution::Lazy(kiln),
            Some(kiln) => KilnResolution::Ready(kiln),
        }
    }

    /// The directories a set of kiln names reaches.
    ///
    /// **An unresolvable name is dropped, loudly.** This is the structural half
    /// of the rule, spelled once so that every consumer inherits it instead of
    /// each deciding for itself what an absent entry means: an unresolvable
    /// name is not a kiln, so it contributes no containment root, no data
    /// classification, no search source and no prompt line. Consumers that read
    /// absence as "unconstrained" are the shape this codebase has paid for more
    /// than once, and the fix is that they never see the absence at all.
    ///
    /// Order is the caller's, which is what [`Session::default_kiln`] reads
    /// position from; duplicates collapse.
    ///
    /// [`Session::default_kiln`]: crucible_core::Session::default_kiln
    pub fn paths_for<'a>(&self, names: impl IntoIterator<Item = &'a KilnName>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for name in names {
            match self.resolve(name).registered() {
                Some(kiln) if paths.contains(&kiln.path().to_path_buf()) => {}
                Some(kiln) => paths.push(kiln.path().to_path_buf()),
                None => warn!(
                    kiln = %name,
                    "No registry entry for this kiln name; it grants nothing to the session naming it"
                ),
            }
        }
        paths
    }

    /// The name a path is registered under, if any.
    ///
    /// Both spellings of both sides are compared, so `~/vault` in the config
    /// and the expanded absolute path in a persisted `meta.json` are one
    /// entry, and so is a kiln opened through a symlink.
    pub fn name_for(&self, path: &Path) -> Option<&KilnName> {
        let resolved = ResolvedPath::resolve(&self.absolutize(path));
        self.by_path
            .get(resolved.lexical())
            .or_else(|| self.by_path.get(resolved.canonical()))
    }

    /// Registered, eager entries, in name order — what a startup open should
    /// touch. Lazy entries are absent by construction rather than by a filter
    /// the caller has to remember.
    pub fn eager(&self) -> impl Iterator<Item = &RegisteredKiln> {
        self.entries.values().filter(|kiln| !kiln.lazy)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RegisteredKiln> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Register a path that arrived at runtime — `cru chat --kiln ./notes`, an
    /// RPC that still accepts a path, a `meta.json` being reverse-resolved on
    /// a first-party request.
    ///
    /// This is the choke point item: the floor runs *before* an entry exists,
    /// so a refusal leaves no entry and mints no name. The refusal is an
    /// `Err` the call site cannot ignore, and it must not be turned into "use
    /// the entry that already has this name" — that is how a session ends up
    /// pointed at a different corpus with the same basename.
    ///
    /// Re-registering a path that is already known is a no-op returning its
    /// existing name. Persisting the new entry back to the user's config is
    /// the caller's job (Phase 3); this registry is the in-memory authority.
    pub fn register_path(&mut self, path: &Path) -> Result<KilnName, RegistrationRefused> {
        let absolute = self.absolutize(path);
        self.refuse(&absolute).map_err(RegistrationRefused)?;

        let resolved = ResolvedPath::resolve(&absolute);
        if let Some(existing) = self
            .by_path
            .get(resolved.lexical())
            .or_else(|| self.by_path.get(resolved.canonical()))
        {
            return Ok(existing.clone());
        }

        let derived = absolute
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(KilnName::normalize)
            .ok_or_else(|| {
                RegistrationRefused(format!(
                    "Refusing '{}' as a kiln: no usable name can be derived from it",
                    absolute.display()
                ))
            })?;
        let name = self.free_name(&derived).ok_or_else(|| {
            RegistrationRefused(format!(
                "Refusing '{}' as a kiln: '{derived}' and every disambiguation of it are taken. \
                 Give it a name of its own under `[kilns]` in your config.",
                absolute.display()
            ))
        })?;

        self.index(name.clone(), resolved, false);
        Ok(name)
    }

    /// A configured entry: same floor, but a name the user wrote rather than
    /// one we derived — so a collision is a startup abort instead of a
    /// disambiguation. Nobody chose `docs` twice on purpose; two `[kilns]`
    /// keys folding onto one name is a config the user must resolve, and
    /// picking a winner silently re-points already-persisted sessions at a
    /// different corpus.
    fn insert_configured(
        &mut self,
        key: &str,
        raw: &Path,
        lazy: bool,
    ) -> Result<(), RegistryError> {
        let Some(name) = KilnName::normalize(key) else {
            warn!(
                kiln = key,
                "Ignoring a `[kilns]` entry whose name folds to nothing usable"
            );
            return Ok(());
        };
        if name != key {
            // Both spellings, once: `cru init` wrote the directory basename
            // verbatim, so `My Vault` is in configs in the wild and the user
            // needs to know which name the daemon answers to.
            warn!(
                configured = key,
                using = %name,
                "Kiln name folded to the registry charset"
            );
        }

        let absolute = self.absolutize(raw);
        if let Err(reason) = self.refuse(&absolute) {
            // No entry AND no name. A name that resolves to nothing is worse
            // than no name: it is an absence, and absences get read as
            // "unconstrained" by whichever consumer forgets.
            warn!(kiln = %name, "{reason}");
            return Ok(());
        }

        let resolved = ResolvedPath::resolve(&absolute);
        match self.entries.get(&name) {
            // Same name, same directory: two spellings of one entry, so
            // there is nothing to choose between.
            Some(existing) if existing.path == *resolved.lexical() => return Ok(()),
            Some(existing) => {
                return Err(RegistryError::Collision {
                    name,
                    first: existing.path.display().to_string(),
                    second: resolved.lexical().display().to_string(),
                })
            }
            None => {}
        }

        self.index(name, resolved, lazy);
        Ok(())
    }

    /// `~` expansion and relative anchoring — the *only* place either happens,
    /// so both directions of the map agree on what a path is called.
    fn absolutize(&self, raw: &Path) -> PathBuf {
        // Only a `~` path reaches the expander, and only a valid-UTF-8 path can
        // hold one; anything else passes through byte-for-byte rather than
        // being lossily rewritten into a different path.
        let expanded = match raw.to_str() {
            Some(s) if s.starts_with('~') => resolve_registration_root(s, self.ctx.home.as_deref()),
            _ => raw.to_path_buf(),
        };
        if expanded.is_absolute() || expanded.as_os_str().is_empty() {
            // An empty path stays empty rather than becoming `base`: the floor
            // must see the nothing the caller actually supplied.
            expanded
        } else {
            self.ctx.base.join(expanded)
        }
    }

    /// The floor: the shared scope refusal, plus the data root.
    ///
    /// The data root is refused as itself *and* as anything enclosing it —
    /// same shape as the home-directory rule — because a kiln that contains
    /// `~/.crucible` contains every recorded transcript. Everything *under*
    /// the data root that matters is already covered: the sessions root is
    /// refused by [`refuse_forbidden_scope`] above.
    fn refuse(&self, absolute: &Path) -> Result<(), String> {
        refuse_forbidden_scope("kiln", absolute, &self.ctx.sessions_root)?;

        if self.ctx.data_home.as_os_str().is_empty() {
            return Ok(());
        }
        let candidate = ResolvedPath::resolve(absolute);
        let data_home = ResolvedPath::resolve(&self.ctx.data_home);
        for form in [candidate.lexical(), candidate.canonical()] {
            for root in [data_home.lexical(), data_home.canonical()] {
                if root.starts_with(form) {
                    return Err(format!(
                        "Refusing '{}' as a kiln: it is the daemon data root or an ancestor of \
                         it, which holds every recorded transcript",
                        form.display()
                    ));
                }
            }
        }
        Ok(())
    }

    fn index(&mut self, name: KilnName, resolved: ResolvedPath, lazy: bool) {
        let kiln = RegisteredKiln {
            name: name.clone(),
            path: resolved.lexical().to_path_buf(),
            resolved: resolved.canonical().to_path_buf(),
            lazy,
        };
        // `or_insert`, not `insert`: two names may legitimately alias one
        // directory, and the reverse lookup then answers with the first in
        // name order rather than whichever was indexed last.
        self.by_path
            .entry(kiln.path.clone())
            .or_insert_with(|| name.clone());
        self.by_path
            .entry(kiln.resolved.clone())
            .or_insert_with(|| name.clone());
        self.entries.insert(name, kiln);
    }

    /// `derived`, or the first free `derived-2`, `derived-3`, … `None` when
    /// every candidate is taken — which is a refusal, not a licence to reuse
    /// the incumbent.
    fn free_name(&self, derived: &KilnName) -> Option<KilnName> {
        if !self.entries.contains_key(derived) {
            return Some(derived.clone());
        }
        for n in 2..=MAX_DERIVED_SUFFIX {
            let suffix = format!("-{n}");
            let room = KilnName::MAX_LEN.saturating_sub(suffix.len());
            let stem = &derived.as_str()[..derived.as_str().len().min(room)];
            let candidate = KilnName::normalize(&format!("{stem}{suffix}"))?;
            if !self.entries.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

/// The two fields of the app config the registry reads.
///
/// A narrow view rather than `CliAppConfig`: the daemon is handed JSON, and a
/// registry that failed to build because an unrelated section of the config
/// had drifted would take the daemon down with it.
#[derive(serde::Deserialize)]
struct KilnConfigView {
    #[serde(default)]
    kiln_path: PathBuf,
    #[serde(default)]
    kilns: HashMap<String, KilnEntry>,
}

#[cfg(test)]
mod tests;
