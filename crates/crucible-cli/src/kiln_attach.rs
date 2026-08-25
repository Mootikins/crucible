//! Turning a `--kiln` value, or a `cru kiln register` pair, into a registry
//! name.
//!
//! Kilns are addressed by name everywhere past this point — that is the whole
//! change these doors serve. But a *person* at a shell types a directory at
//! least as often as a name, and the plan is explicit that a path is still a
//! door: it moves the floor from every attach site to one registration site, it
//! does not close the door. So this module is where a path becomes a name, and
//! the only interesting thing about it is that it never becomes one cheaply.
//!
//! # The disambiguation rule, stated once
//!
//! > A value that names a **registered** kiln is that kiln. Anything else is a
//! > directory, and it must be an existing one.
//!
//! Two consequences worth stating because each is a bug the other reading
//! would have:
//!
//! - A bare word that no entry claims is **not** silently a new kiln. It is
//!   read as a relative directory, and if no such directory exists the command
//!   fails naming both readings. Otherwise every typo'd name — `cru acp --kiln
//!   ntoes` — mints an entry pointing at nothing, and a kiln that quietly is
//!   not there reads exactly like a kiln with nothing in it. An unresolvable
//!   name must deny, and the cheapest way to guarantee that is never to create
//!   one.
//! - A name always wins over a same-named directory in the working directory.
//!   `./notes` is how you say you meant the directory, and it can never be
//!   read as a name, because [`KilnName`] refuses a leading dot and a path
//!   separator.
//!
//! # The floor is the registry's, not ours
//!
//! Every path here goes through [`KilnRegistry::register_path`] or
//! [`KilnRegistry::register_named`], which run `refuse_forbidden_scope` and the
//! catastrophic-root check *before* an entry exists. This module adds no
//! policy of its own and must not: a second gate is a second thing to forget
//! to update, and the reason the registration site exists is that there used to
//! be one gate per attach site.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crucible_core::config::{CliAppConfig, KilnEntry, KilnName};
use crucible_daemon::kiln_registry::{KilnRegistry, KilnRegistryContext};

/// What a `--kiln` value, or a `cru kiln register` pair, turned out to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedKiln {
    /// The registry name. This, and not the path, is what crosses the wire.
    pub name: KilnName,
    /// Where it lives — absolute, `~`-expanded, `..`-clamped. The CLI needs
    /// this for its own local work (opening the directory, the ACP kiln
    /// discovery check); it is not what gets sent to the daemon.
    pub path: PathBuf,
    /// True when this call wrote a new `[kilns]` entry into the user's config.
    /// False when the name or the directory was already registered, which is
    /// what keeps a repeated `--kiln` from churning the file.
    pub registered: bool,
}

/// The kiln registry as a CLI process sees it: the same in-memory registry the
/// daemon builds, plus the config file that outlives the process.
///
/// Both halves are needed and neither is sufficient. The registry is where the
/// floor lives and where names are derived and disambiguated; the file is the
/// only thing that makes a name mean anything on the *next* run, and the plan
/// is explicit that there is one registry file, not two — a second writer
/// produces two entries with one derived name and a daemon that will not start
/// under our own collision rule.
pub struct CliKilnRegistry {
    registry: KilnRegistry,
}

impl CliKilnRegistry {
    /// Build from the config this process loaded.
    ///
    /// `ctx` is a value rather than something read in here for the reason the
    /// daemon threads its data root: a registry that read the environment would
    /// resolve the developer's real `~/.crucible` in every test, and the floor
    /// would then be judged against a root no test controls.
    pub fn new(config: &CliAppConfig, ctx: KilnRegistryContext) -> Result<Self> {
        // Through `from_app_config`, not by reaching into `config.kilns`: that
        // is the builder the daemon uses, so a name means the same thing on
        // both sides of the socket. Going around it loses the `kiln_path`-only
        // config shape and the bundled `crucible-docs` entry's `lazy` flag.
        let app_config = serde_json::to_value(config)
            .context("serializing the loaded config for the kiln registry")?;
        let registry = KilnRegistry::from_app_config(ctx, Some(&app_config))
            .context("building the kiln registry from the loaded config")?;
        Ok(Self { registry })
    }

    /// The production shape: relative paths anchored at the user's working
    /// directory, `~` against the real home, the real data root.
    ///
    /// Deliberately the same anchors the daemon uses. The CLI is the process
    /// that *spawns* the daemon and hands it this very config, so a CLI that
    /// anchored a relative `--kiln ./notes` anywhere else would write an entry
    /// the daemon then resolves to a different directory.
    pub fn for_cli(config: &CliAppConfig) -> Result<Self> {
        Self::new(
            config,
            KilnRegistryContext::for_daemon(crucible_core::config::crucible_home()),
        )
    }

    /// Decide what a `--kiln` value names. See the module docs for the rule.
    ///
    /// **Decides; does not register.** This used to write a `[kilns]` entry
    /// into the user's config, which is the whole reason it needed `&mut self`
    /// and a config path. A registration is state the daemon owns, and the
    /// caller is the layer holding the daemon connection — so this answers the
    /// question and the caller acts on the answer.
    ///
    /// The floor still runs here, on the resolved path, so the refusal that
    /// names BOTH readings of an unresolvable value is produced where both
    /// readings are still in view.
    pub fn resolve(&self, value: &str) -> Result<KilnTarget> {
        if let Ok(name) = KilnName::parse(value) {
            if let Some(entry) = self.registry.resolve(&name).registered() {
                return Ok(KilnTarget::Registered(AttachedKiln {
                    name,
                    path: entry.path().to_path_buf(),
                    registered: false,
                }));
            }
        }

        let raw = Path::new(value);
        // A directory the registry already has a name for is not a
        // registration request: it is that kiln, named.
        if let Some(name) = self.registry.name_for(raw) {
            let path = self.path_of(&name);
            return Ok(KilnTarget::Registered(AttachedKiln {
                name,
                path,
                registered: false,
            }));
        }

        let path = self
            .registry
            .canonical_for_registration(raw)
            .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;

        if !path.is_dir() {
            // Both readings, because the caller does not yet know which one we
            // took, and the fix differs: a misspelled name, or a missing
            // directory. Nothing has been written and nothing downstream runs.
            bail!(
                "Unknown kiln {value:?}: no kiln is registered under that name, and \
                 '{}' is not a directory. Register one with `cru kiln register <name> <path>`.",
                path.display()
            );
        }

        Ok(KilnTarget::Directory(path))
    }

    /// Where a name the registry resolves lives.
    ///
    /// Only ever called on a name the registry just answered for, so `Unknown`
    /// is unreachable — and it is a panic rather than a fallback because the
    /// fallback would be a path, which is exactly what must not be invented
    /// here.
    fn path_of(&self, name: &KilnName) -> PathBuf {
        self.registry
            .resolve(name)
            .path()
            .expect("a name the registry just accepted must resolve")
            .to_path_buf()
    }
}

/// What a `--kiln` value turned out to name.
///
/// Two cases and no third: either the registry already answers to it, or it is
/// a directory that passed the floor and needs a name. The caller decides what
/// to do about the second, because registering is the daemon's job and the
/// caller is the layer holding the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KilnTarget {
    /// A name the registry resolves, or a directory it already has a name for.
    Registered(AttachedKiln),
    /// An existing directory with no name yet, absolute and normalized.
    Directory(PathBuf),
}

impl AttachedKiln {
    /// Teach an already-loaded config about this kiln, so the rest of *this*
    /// process resolves the name without re-reading the file we just wrote.
    ///
    /// `session_kiln` too, and not only `kilns`: that field is what
    /// [`CliAppConfig::session_kiln_name`] reads to decide which kiln a new
    /// session attaches, and it compares the configured path verbatim. Setting
    /// one without the other yields a session that silently attaches the
    /// *default* kiln instead of the one the flag named.
    pub fn apply_to(&self, config: &mut CliAppConfig) {
        config
            .kilns
            .insert(self.name.to_string(), KilnEntry::Path(self.path.clone()));
        config.session_kiln = Some(self.path.clone());
        config.kiln_path = self.path.clone();
    }
}

#[cfg(test)]
mod tests;
