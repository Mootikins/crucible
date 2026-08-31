//! The host-owned module resolver: one `require` for every Crucible VM.
//!
//! PUC Lua resolved modules through `package.path`, a mutable global string,
//! and `package.searchers`, a mutable global list. Luau has neither. That is
//! an improvement rather than a loss: lookup is authority — a plugin that can
//! append to `package.path` can widen what it may import — so the host owns
//! it here, and Lua code can only ask.
//!
//! ## The two layers, and why there are two
//!
//! **Public roots** are the plugin directories and the user's `lua/`
//! directory. A module found under one of them is cached by NAME, in the
//! `package.loaded` compatibility table, exactly as Lua caches it. Names
//! there are directory names, so they are already unique, and a plugin that
//! writes `package.loaded["auto-title"] = plugin` (five shipped plugins do)
//! makes the copy the daemon executed the copy a later `require` answers with.
//!
//! **Private roots** are one plugin's own `lua/` directory, pushed for the
//! duration of that plugin's load and popped after. A module found under one
//! is cached by PATH and never by name. Several plugins ship a module called
//! `config`; under one shared name table the second plugin to load silently
//! got the first plugin's module.
//!
//! ## What a resolver owes its callers
//!
//! - **A cycle must raise, not recurse.** A file that is mid-load is recorded
//!   as loading; a second `require` for it raises rather than reading the
//!   file again.
//! - **A `nil` return must be recorded.** Lua stores `true` for a module that
//!   returns nothing, so the file runs once. A resolver that skips this runs
//!   the file on every call.
//! - **A reload must forget.** [`ModuleRegistry::invalidate_under`] drops
//!   every cached module whose file lies under a directory, so a plugin's
//!   private module is re-read after its file changes.
//! - **A name may not traverse.** `..`, `/`, `\` and absolute paths are
//!   refused, and a resolved file that escapes its root is refused again
//!   after canonicalization.

use mlua::{Lua, RegistryKey, Table, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Which layer a root belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// The user's own `lua/` directory. Searched before plugin roots, so a
    /// user module shadows a same-named plugin module.
    User,
    /// A directory that holds plugin directories.
    Plugin,
}

/// One resolution, handed to the load hook before the default load runs.
pub struct ModuleRequest {
    /// The name the caller passed to `require`.
    pub name: String,
    /// The canonical file that answers it.
    pub path: PathBuf,
    /// The layer the file came from.
    pub kind: RootKind,
    /// Whether this is a plugin ENTRY module: an undotted name answered by
    /// `<plugin root>/<name>.lua` or `<plugin root>/<name>/init.lua`. A
    /// plugin's own submodule is not one.
    pub is_entry: bool,
    /// Whether the same name also resolves under a plugin root. Only ever
    /// true for a `User` resolution — it is how the boot reports shadowing.
    pub shadows_plugin: bool,
    /// Whether the file came from a plugin's own `lua/` directory rather than
    /// a public root. A private module is cached by PATH and never by name,
    /// so two plugins that each ship a `config` module keep their own.
    pub private: bool,
}

/// A hook that may load a module itself. `None` means "load it the ordinary
/// way". The boot phase uses this to run a plugin entry module under that
/// plugin's context.
pub type ModuleLoadHook =
    Arc<dyn Fn(&Lua, &ModuleRequest) -> Option<mlua::Result<Value>> + Send + Sync>;

/// A module cached by path, with the value it evaluated to.
struct CachedModule {
    key: RegistryKey,
}

#[derive(Default)]
struct ModuleState {
    /// Public roots, in search order.
    roots: Vec<(PathBuf, RootKind)>,
    /// Active private roots, innermost last. Searched before every public
    /// root, so a plugin's own module wins over a same-named public one.
    private: Vec<PathBuf>,
    /// Every plugin directory that has been entered. A `require` that runs
    /// after the load — from a handler, a timer, a service — resolves against
    /// the directory of the FILE that called it, so a plugin keeps its own
    /// modules for the process's lifetime without keeping a root active for
    /// every other plugin.
    plugin_dirs: Vec<PathBuf>,
    /// Private modules, keyed by canonical file.
    by_path: HashMap<PathBuf, CachedModule>,
    /// The file each publicly-cached name came from, for invalidation.
    by_name: HashMap<String, PathBuf>,
    /// Files whose load has started and not finished.
    loading: HashSet<PathBuf>,
    hook: Option<ModuleLoadHook>,
}

/// A handle on one VM's module state. Cloning shares the state.
#[derive(Clone, Default)]
pub struct ModuleRegistry {
    state: Arc<Mutex<ModuleState>>,
}

/// Pops a private root when it drops, so a plugin's own `lua/` directory
/// cannot outlive its load. Without this the root list grew for the
/// process's lifetime and resolution order followed load order.
pub struct PrivateRootGuard {
    registry: ModuleRegistry,
    root: PathBuf,
}

impl Drop for PrivateRootGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.registry.state.lock() {
            if let Some(index) = state.private.iter().rposition(|root| *root == self.root) {
                state.private.remove(index);
            }
        }
    }
}

/// The refusal a poisoned lock raises. The state is shared by handle across
/// the loader, the boot and the `require` callback; a panic while it is held
/// must not silently widen or narrow what a plugin may import.
fn poisoned() -> mlua::Error {
    mlua::Error::runtime("the module resolver lock is poisoned")
}

impl ModuleRegistry {
    /// Install `require`, `package.loaded` and `package.preload` on this VM
    /// and return the handle that owns their state.
    ///
    /// `package.path` is deliberately absent. Nothing in a Crucible VM reads
    /// it, and a plugin that could write it would be choosing its own import
    /// authority.
    pub fn install(lua: &Lua) -> mlua::Result<Self> {
        let registry = Self::default();

        let package = lua.create_table()?;
        package.set("loaded", lua.create_table()?)?;
        package.set("preload", lua.create_table()?)?;
        let searchpath_registry = registry.clone();
        package.set(
            "searchpath",
            lua.create_function(move |_, (name, _path): (String, Option<String>)| {
                match searchpath_registry.resolve(&name) {
                    Ok(Some(found)) => Ok((
                        Some(found.path.to_string_lossy().into_owned()),
                        None::<String>,
                    )),
                    Ok(None) | Err(_) => Ok((
                        None,
                        Some(format!("no module '{name}' under the host module roots")),
                    )),
                }
            })?,
        )?;
        lua.globals().set("package", package)?;

        let require_registry = registry.clone();
        let require =
            lua.create_function(move |lua, name: String| require_registry.require(lua, &name))?;
        lua.globals().set("require", require)?;

        Ok(registry)
    }

    /// Replace the public roots. The order is the search order.
    pub fn set_roots(&self, roots: Vec<(PathBuf, RootKind)>) -> mlua::Result<()> {
        self.state.lock().map_err(|_| poisoned())?.roots = roots;
        Ok(())
    }

    /// The public plugin roots, in search order.
    pub fn plugin_roots(&self) -> Vec<PathBuf> {
        self.state
            .lock()
            .map(|state| {
                state
                    .roots
                    .iter()
                    .filter(|(_, kind)| *kind == RootKind::Plugin)
                    .map(|(path, _)| path.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Make one plugin's own `lua/` directory resolvable for as long as the
    /// guard lives.
    pub fn enter_plugin_root(&self, plugin_dir: &Path) -> mlua::Result<PrivateRootGuard> {
        let root = plugin_dir.to_path_buf();
        {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            state.private.push(root.clone());
            if !state.plugin_dirs.contains(&root) {
                state.plugin_dirs.push(root.clone());
            }
        }
        Ok(PrivateRootGuard {
            registry: self.clone(),
            root,
        })
    }

    /// Install the load hook, returning the one it replaced.
    pub fn set_load_hook(&self, hook: Option<ModuleLoadHook>) -> Option<ModuleLoadHook> {
        match self.state.lock() {
            Ok(mut state) => std::mem::replace(&mut state.hook, hook),
            Err(_) => None,
        }
    }

    /// Every publicly cached module name, with the file it came from.
    pub fn loaded_modules(&self) -> Vec<(String, PathBuf)> {
        self.state
            .lock()
            .map(|state| {
                state
                    .by_name
                    .iter()
                    .map(|(name, path)| (name.clone(), path.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record a name as publicly loaded from `path` without re-running it.
    /// The daemon uses this when it executes a plugin's entry file itself.
    pub fn record_public(&self, name: &str, path: &Path) {
        if let Ok(mut state) = self.state.lock() {
            state.by_name.insert(name.to_string(), path.to_path_buf());
        }
    }

    /// Forget the PRIVATE modules cached from under `dir`, leaving
    /// `package.loaded` alone.
    ///
    /// This is what an activation needs: a reload must re-read the plugin's
    /// own `lua/` modules, while the entry instance in `package.loaded` — the
    /// one a user's boot `require` created, and the one activation reuses
    /// instead of executing the file twice — has to survive.
    pub fn invalidate_private_under(&self, lua: &Lua, dir: &Path) -> mlua::Result<()> {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        let paths: Vec<PathBuf> = state
            .by_path
            .keys()
            .filter(|path| path.starts_with(&dir))
            .cloned()
            .collect();
        for path in paths {
            if let Some(cached) = state.by_path.remove(&path) {
                let _ = lua.remove_registry_value(cached.key);
            }
        }
        Ok(())
    }

    /// Forget every cached module whose file lies under `dir`, the entry
    /// name in `package.loaded` included.
    ///
    /// This is what a full reload needs: the plugin's private `config` module
    /// is re-read after its file changes, and the next `require` of the
    /// plugin evaluates its entry file again.
    pub fn invalidate_under(&self, lua: &Lua, dir: &Path) -> mlua::Result<()> {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let (paths, names) = {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            let paths: Vec<PathBuf> = state
                .by_path
                .keys()
                .filter(|path| path.starts_with(&dir))
                .cloned()
                .collect();
            let names: Vec<String> = state
                .by_name
                .iter()
                .filter(|(_, path)| path.starts_with(&dir))
                .map(|(name, _)| name.clone())
                .collect();
            for path in &paths {
                if let Some(cached) = state.by_path.remove(path) {
                    let _ = lua.remove_registry_value(cached.key);
                }
            }
            for name in &names {
                state.by_name.remove(name);
            }
            (paths, names)
        };
        if names.is_empty() && paths.is_empty() {
            return Ok(());
        }
        let loaded: Table = lua.globals().get::<Table>("package")?.get("loaded")?;
        for name in names {
            loaded.set(name, Value::Nil)?;
        }
        Ok(())
    }

    /// Forget one publicly cached name.
    pub fn invalidate_name(&self, lua: &Lua, name: &str) -> mlua::Result<()> {
        self.state
            .lock()
            .map_err(|_| poisoned())?
            .by_name
            .remove(name);
        let loaded: Table = lua.globals().get::<Table>("package")?.get("loaded")?;
        loaded.set(name, Value::Nil)
    }

    /// Where a name resolves, without loading it.
    pub fn resolve(&self, name: &str) -> mlua::Result<Option<ModuleRequest>> {
        self.resolve_for(name, None)
    }

    /// [`Self::resolve`], with the file that asked. A `require` from inside a
    /// plugin resolves that plugin's private modules even after its load has
    /// finished — a handler, a timer or a service may require lazily.
    pub fn resolve_for(
        &self,
        name: &str,
        caller: Option<&Path>,
    ) -> mlua::Result<Option<ModuleRequest>> {
        let relative = validate_name(name)?;
        let (private, plugin_dirs, roots) = {
            let state = self.state.lock().map_err(|_| poisoned())?;
            (
                state.private.clone(),
                state.plugin_dirs.clone(),
                state.roots.clone(),
            )
        };

        let mut scopes: Vec<PathBuf> = private.iter().rev().cloned().collect();
        if let Some(caller) = caller {
            // The caller's own plugin directory, innermost first: a nested
            // plugin directory wins over the tree that contains it.
            let mut owning: Vec<&PathBuf> = plugin_dirs
                .iter()
                .filter(|dir| caller.starts_with(dir))
                .collect();
            owning.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
            scopes.extend(owning.into_iter().cloned());
        }

        for root in scopes {
            if let Some(path) = module_file(&root.join("lua"), &relative)? {
                return Ok(Some(ModuleRequest {
                    name: name.to_string(),
                    path,
                    kind: RootKind::Plugin,
                    is_entry: false,
                    shadows_plugin: false,
                    private: true,
                }));
            }
        }

        for (root, kind) in &roots {
            let Some(path) = module_file(root, &relative)? else {
                continue;
            };
            let is_entry = *kind == RootKind::Plugin && !name.contains('.');
            let shadows_plugin = *kind == RootKind::User
                && roots.iter().any(|(other, other_kind)| {
                    *other_kind == RootKind::Plugin && module_exists(other, &relative)
                });
            return Ok(Some(ModuleRequest {
                name: name.to_string(),
                path,
                kind: *kind,
                is_entry,
                shadows_plugin,
                private: false,
            }));
        }
        Ok(None)
    }

    /// The `require` body.
    fn require(&self, lua: &Lua, name: &str) -> mlua::Result<Value> {
        let package: Table = lua.globals().get("package")?;
        let loaded: Table = package.get("loaded")?;

        // The compatibility table wins. A plugin publishes its own instance
        // there, and a test clears an entry to force a reload.
        let cached: Value = loaded.get(name)?;
        if !matches!(cached, Value::Nil) {
            return Ok(cached);
        }

        let preload: Table = package.get("preload")?;
        if let Value::Function(loader) = preload.get::<Value>(name)? {
            let value: Value = loader.call(name.to_string())?;
            let value = if matches!(value, Value::Nil) {
                Value::Boolean(true)
            } else {
                value
            };
            loaded.set(name, value.clone())?;
            return Ok(value);
        }

        let Some(request) = self.resolve_for(name, calling_dir(lua).as_deref())? else {
            return Err(mlua::Error::runtime(format!(
                "module '{name}' was not found under the host module roots"
            )));
        };

        // A private module is cached by path, so two plugins that each ship a
        // `config` module get their own.
        if request.private {
            let cached = {
                let state = self.state.lock().map_err(|_| poisoned())?;
                state
                    .by_path
                    .get(&request.path)
                    .map(|cached| lua.registry_value::<Value>(&cached.key))
            };
            if let Some(value) = cached {
                return value;
            }
        }

        {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            if !state.loading.insert(request.path.clone()) {
                return Err(mlua::Error::runtime(format!(
                    "circular require of module '{name}' ({})",
                    request.path.display()
                )));
            }
        }

        let outcome = self.load(lua, &request);

        self.state
            .lock()
            .map_err(|_| poisoned())?
            .loading
            .remove(&request.path);

        // Lua records a module that returns nothing as loaded, so its file
        // runs once. Mirror that rather than re-reading the file forever.
        let value = match outcome? {
            Value::Nil => Value::Boolean(true),
            value => value,
        };

        // A module from a public root is cached by NAME, as Lua caches it:
        // its name already carries the plugin directory, so it cannot
        // collide, and the boot needs the name→file record to know which
        // files the user's `require` already executed.
        if !request.private {
            loaded.set(name, value.clone())?;
            self.state
                .lock()
                .map_err(|_| poisoned())?
                .by_name
                .insert(name.to_string(), request.path.clone());
        } else {
            let key = lua.create_registry_value(value.clone())?;
            self.state
                .lock()
                .map_err(|_| poisoned())?
                .by_path
                .insert(request.path.clone(), CachedModule { key });
        }
        Ok(value)
    }

    /// Run the hook, or read and evaluate the file.
    fn load(&self, lua: &Lua, request: &ModuleRequest) -> mlua::Result<Value> {
        let hook = self.state.lock().map_err(|_| poisoned())?.hook.clone();
        if let Some(hook) = hook {
            if let Some(result) = hook(lua, request) {
                return result;
            }
        }
        let source = std::fs::read_to_string(&request.path)
            .map_err(|e| mlua::Error::runtime(format!("read {}: {e}", request.path.display())))?;
        lua.load(&source)
            .set_name(format!("@{}", request.path.display()))
            .call(request.name.clone())
    }
}

/// The directory of the file the running chunk came from.
///
/// Chunk names are set as `@<path>` (a path) or left as literal source. Only
/// the first form names a file, and only a file can own private modules.
fn calling_dir(lua: &Lua) -> Option<PathBuf> {
    for level in 1..8 {
        let source = lua.inspect_stack(level, |debug| {
            debug.source().source.map(|source| source.to_string())
        })??;
        let path = source.strip_prefix('@').unwrap_or(&source);
        let path = Path::new(path);
        if path.is_absolute() && path.is_file() {
            return path.parent().map(Path::to_path_buf);
        }
    }
    None
}

/// A module name's path fragment, or a refusal.
///
/// The refusal is the containment: `require("../../etc/passwd")` and
/// `require("/etc/passwd")` never reach the filesystem.
fn validate_name(name: &str) -> mlua::Result<PathBuf> {
    let refuse = || mlua::Error::runtime(format!("invalid module name '{name}'"));
    if name.is_empty() || name.len() > 256 {
        return Err(refuse());
    }
    let mut relative = PathBuf::new();
    for part in name.split('.') {
        if part.is_empty()
            || !part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(refuse());
        }
        relative.push(part);
    }
    Ok(relative)
}

/// The first of `<root>/<relative>.luau`, `.lua`, `<relative>/init.luau` and
/// `init.lua` that exists, canonicalized and proved to still lie under the
/// root.
///
/// The order lives in [`crate::source_files::module_candidates`], which every
/// other site that resolves a module name uses too.
///
/// An ERROR when TWO extensions of one name are there, naming both files.
/// `init.luau` beside `init.lua` was already refused for a plugin's entry
/// point; `helper.luau` beside `helper.lua` used to resolve silently in favour
/// of `.luau`, so the same mistake by the same author had two different
/// answers depending on whether the file happened to be an entry point. See
/// [`crate::source_files::collides`].
///
/// The refusal used to be a `None`, which the caller reports as "module
/// 'helper' was not found under the host module roots" — about a file that is
/// plainly there, with the second copy that caused it never mentioned.
/// `cru plugin check` named the collision and `require` denied the file
/// existed, so the two halves of the same rule disagreed at the moment an
/// author needed them to agree.
fn module_file(root: &Path, relative: &Path) -> mlua::Result<Option<PathBuf>> {
    if let Some(ambiguous) = crate::source_files::collides(root, relative) {
        return Err(mlua::Error::runtime(ambiguous.to_string()));
    }
    let Ok(root_canonical) = std::fs::canonicalize(root) else {
        return Ok(None);
    };
    for candidate in crate::source_files::module_candidates(root, relative) {
        if !candidate.is_file() {
            continue;
        }
        // A symlink out of the tree is the traversal a name check cannot
        // catch, so containment is proved on the resolved path.
        let Ok(canonical) = std::fs::canonicalize(&candidate) else {
            return Ok(None);
        };
        if canonical.starts_with(&root_canonical) {
            return Ok(Some(canonical));
        }
    }
    Ok(None)
}

/// Whether a root PROVIDES this module name, a collision included.
///
/// Used where the question is "does something else answer to this name", not
/// "which file loads": a plugin root holding both `helper.lua` and
/// `helper.luau` still shadows, and reading the collision as an absence would
/// say the user's copy shadows nothing.
fn module_exists(root: &Path, relative: &Path) -> bool {
    crate::source_files::collides(root, relative).is_some()
        || matches!(module_file(root, relative), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn vm() -> (Lua, ModuleRegistry) {
        let lua = Lua::new();
        let registry = ModuleRegistry::install(&lua).expect("install");
        (lua, registry)
    }

    fn write(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dir");
        }
        fs::write(path, source).expect("write");
    }

    #[test]
    fn a_plugin_entry_module_resolves_by_name() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/demo/init.lua"),
            "return { name = 'demo' }",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let name: String = lua.load("return require('demo').name").eval().unwrap();
        assert_eq!(name, "demo");
    }

    /// Two plugins, two private modules, one name. Under a single global
    /// name table the second plugin silently got the first one's module.
    #[test]
    fn two_plugins_get_their_own_config_module() {
        let tmp = TempDir::new().unwrap();
        for (plugin, value) in [("alpha", "1"), ("beta", "2")] {
            write(
                &tmp.path().join(format!("plugins/{plugin}/init.lua")),
                "return { value = require('config').value }",
            );
            write(
                &tmp.path().join(format!("plugins/{plugin}/lua/config.lua")),
                &format!("return {{ value = {value} }}"),
            );
        }
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let mut seen = Vec::new();
        for plugin in ["alpha", "beta"] {
            let guard = registry
                .enter_plugin_root(&tmp.path().join("plugins").join(plugin))
                .unwrap();
            let value: i64 = lua
                .load(format!("return require('{plugin}').value"))
                .eval()
                .unwrap();
            seen.push(value);
            drop(guard);
        }
        assert_eq!(seen, vec![1, 2], "each plugin must get its own config");
    }

    /// The guard pops. Without it, `alpha`'s private modules stay resolvable
    /// while `beta` loads, and resolution order follows load order.
    #[test]
    fn a_private_root_is_unreachable_once_its_guard_drops() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/alpha/lua/private.lua"),
            "return { ok = true }",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let guard = registry
            .enter_plugin_root(&tmp.path().join("plugins/alpha"))
            .unwrap();
        lua.load("return require('private').ok")
            .eval::<bool>()
            .unwrap();
        drop(guard);

        let err = lua
            .load("return require('private').ok")
            .eval::<bool>()
            .expect_err("the private module must be out of reach");
        assert!(err.to_string().contains("was not found"), "got: {err}");
    }

    /// A handler requires lazily, long after the load that registered it.
    /// The private root is no longer on the stack, so the resolver has to
    /// answer from the calling file's own plugin directory.
    #[test]
    fn a_handler_requires_a_private_module_after_the_load() {
        let tmp = TempDir::new().unwrap();
        let plugin = tmp.path().join("plugins/late");
        write(
            &plugin.join("init.lua"),
            "return { run = function() return require('helper').value end }",
        );
        write(&plugin.join("lua/helper.lua"), "return { value = 'late' }");
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let guard = registry.enter_plugin_root(&plugin).unwrap();
        lua.load("_G.late = require('late')").exec().unwrap();
        drop(guard);

        let value: String = lua.load("return _G.late.run()").eval().unwrap();
        assert_eq!(
            value, "late",
            "a lazy require must reach the plugin's own module"
        );
    }

    /// One plugin's handler must not reach another plugin's private module,
    /// even though both directories are registered.
    #[test]
    fn a_handler_does_not_reach_another_plugins_private_module() {
        let tmp = TempDir::new().unwrap();
        for (plugin, value) in [("first", "1"), ("second", "2")] {
            let dir = tmp.path().join("plugins").join(plugin);
            write(
                &dir.join("init.lua"),
                "return { run = function() return require('config').value end }",
            );
            write(
                &dir.join("lua/config.lua"),
                &format!("return {{ value = {value} }}"),
            );
        }
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        for plugin in ["first", "second"] {
            let guard = registry
                .enter_plugin_root(&tmp.path().join("plugins").join(plugin))
                .unwrap();
            lua.load(format!("_G.{plugin} = require('{plugin}')"))
                .exec()
                .unwrap();
            drop(guard);
        }

        let first: i64 = lua.load("return _G.first.run()").eval().unwrap();
        let second: i64 = lua.load("return _G.second.run()").eval().unwrap();
        assert_eq!((first, second), (1, 2), "each handler keeps its own config");
    }

    #[test]
    fn a_circular_require_raises_instead_of_recursing() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/loop-a/init.lua"),
            "return require('loop-b')",
        );
        write(
            &tmp.path().join("plugins/loop-b/init.lua"),
            "return require('loop-a')",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let err = lua
            .load("return require('loop-a')")
            .eval::<Value>()
            .expect_err("a cycle must raise");
        assert!(err.to_string().contains("circular require"), "got: {err}");
    }

    /// A module that returns nothing is loaded once, as in Lua.
    #[test]
    fn a_module_that_returns_nil_is_recorded_as_loaded() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/counter/init.lua"),
            "_G.counter_runs = (_G.counter_runs or 0) + 1",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let value: bool = lua.load("return require('counter')").eval().unwrap();
        assert!(value, "a nil-returning module resolves to true");
        lua.load("require('counter')").exec().unwrap();
        let runs: i64 = lua.load("return _G.counter_runs").eval().unwrap();
        assert_eq!(runs, 1, "the file must not run twice");
    }

    #[test]
    fn a_traversing_name_is_refused() {
        let tmp = TempDir::new().unwrap();
        write(&tmp.path().join("plugins/demo/init.lua"), "return {}");
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        for name in ["../secret", "/etc/passwd", "a/b", "..", ""] {
            let err = lua
                .load(format!("return require({name:?})"))
                .eval::<Value>()
                .expect_err("a traversing name must be refused");
            assert!(
                err.to_string().contains("invalid module name"),
                "name {name:?} got: {err}"
            );
        }
    }

    /// A symlink out of the root is the traversal a name check cannot see.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_root_is_refused() {
        let tmp = TempDir::new().unwrap();
        let outside = tmp.path().join("outside/secret.lua");
        write(&outside, "return { secret = true }");
        fs::create_dir_all(tmp.path().join("plugins")).unwrap();
        std::os::unix::fs::symlink(&outside, tmp.path().join("plugins/secret.lua")).unwrap();

        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let err = lua
            .load("return require('secret')")
            .eval::<Value>()
            .expect_err("a symlink out of the root must be refused");
        assert!(err.to_string().contains("was not found"), "got: {err}");
    }

    #[test]
    fn a_reload_re_reads_a_private_module() {
        let tmp = TempDir::new().unwrap();
        let plugin = tmp.path().join("plugins/demo");
        write(&plugin.join("init.lua"), "return require('version')");
        write(&plugin.join("lua/version.lua"), "return { v = 1 }");
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let guard = registry.enter_plugin_root(&plugin).unwrap();
        let first: i64 = lua.load("return require('demo').v").eval().unwrap();
        assert_eq!(first, 1);

        write(&plugin.join("lua/version.lua"), "return { v = 2 }");
        registry.invalidate_under(&lua, &plugin).unwrap();
        let second: i64 = lua.load("return require('demo').v").eval().unwrap();
        assert_eq!(second, 2, "the reload must re-read the private module");
        drop(guard);
    }

    /// The compatibility table is authoritative, so a plugin's own
    /// `package.loaded[NAME] = plugin` line answers a later `require`.
    #[test]
    fn package_loaded_answers_require() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/demo/init.lua"),
            "local plugin = { marker = 'from the file' }\n\
             package.loaded['demo'] = plugin\n\
             return plugin",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let same: bool = lua
            .load("return require('demo') == require('demo')")
            .eval()
            .unwrap();
        assert!(same, "the same instance must come back");

        lua.load("package.loaded['demo'] = { marker = 'replaced' }")
            .exec()
            .unwrap();
        let marker: String = lua.load("return require('demo').marker").eval().unwrap();
        assert_eq!(marker, "replaced");
    }

    /// A user `lua/` module shadows a same-named plugin module, and the
    /// resolution says so, which is what the boot logs.
    #[test]
    fn a_user_module_shadows_a_plugin_module() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("user/lua/shared.lua"),
            "return { who = 'user' }",
        );
        write(
            &tmp.path().join("plugins/shared/init.lua"),
            "return { who = 'plugin' }",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![
                (tmp.path().join("user/lua"), RootKind::User),
                (tmp.path().join("plugins"), RootKind::Plugin),
            ])
            .unwrap();

        let who: String = lua.load("return require('shared').who").eval().unwrap();
        assert_eq!(who, "user");
        let request = registry.resolve("shared").unwrap().expect("resolves");
        assert!(request.shadows_plugin, "the shadow must be reported");
    }

    /// `require` names the collision. It used to report the module MISSING.
    ///
    /// `cru plugin check` reported "two files answer to the same module name"
    /// and `require` answered "module 'helper' was not found under the host
    /// module roots" for the same pair — about a file plainly there, never
    /// mentioning the second copy that caused it. The two halves of one rule
    /// disagreed at the moment an author needed them to agree.
    #[test]
    fn an_ambiguous_require_names_both_files() {
        let tmp = TempDir::new().unwrap();
        write(&tmp.path().join("plugins/helper.lua"), "return { n = 1 }");
        write(&tmp.path().join("plugins/helper.luau"), "return { n = 2 }");
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let raised = lua
            .load("return require('helper')")
            .eval::<Value>()
            .expect_err("an ambiguous name must refuse")
            .to_string();
        assert!(
            raised.contains("helper.lua") && raised.contains("helper.luau"),
            "the refusal must name both files: {raised}"
        );
        assert!(
            !raised.contains("was not found"),
            "a file that is there must not be reported missing: {raised}"
        );
    }

    /// The hook decides how a plugin entry module runs. The boot needs this
    /// to stamp the plugin context around the file.
    #[test]
    fn the_load_hook_can_answer_an_entry_module() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/demo/init.lua"),
            "return { real = true }",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();
        registry.set_load_hook(Some(Arc::new(|lua: &Lua, request: &ModuleRequest| {
            if !request.is_entry {
                return None;
            }
            let table = lua.create_table().ok()?;
            table.set("hooked", true).ok()?;
            Some(Ok(Value::Table(table)))
        })));

        let hooked: bool = lua.load("return require('demo').hooked").eval().unwrap();
        assert!(hooked);
    }

    #[test]
    fn searchpath_answers_from_the_host_roots() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("plugins/demo/tests/fixtures/init.lua"),
            "return {}",
        );
        let (lua, registry) = vm();
        registry
            .set_roots(vec![(tmp.path().join("plugins"), RootKind::Plugin)])
            .unwrap();

        let found: Option<String> = lua
            .load("return package.searchpath('demo.tests.fixtures', package.path)")
            .eval()
            .unwrap();
        assert!(
            found.is_some_and(|path| path.ends_with("tests/fixtures/init.lua")),
            "searchpath must answer over the host roots"
        );
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }

    /// `.luau` is what Luau's own tooling expects, and Crucible reads it.
    #[test]
    fn a_luau_module_resolves() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("plugins");
        write(
            &root.join("demo/init.luau"),
            "return { value = require('./lua/helper').value }",
        );
        write(
            &root.join("demo/lua/helper.luau"),
            "return { value = 'luau' }",
        );

        let resolved = module_file(&root, Path::new("demo")).expect("no collision");
        assert_eq!(
            resolved,
            Some(std::fs::canonicalize(root.join("demo/init.luau")).unwrap()),
            "an init.luau directory must resolve"
        );
    }

    /// Every plugin already on a user's disk is `.lua`, and stays working.
    #[test]
    fn a_lua_module_still_resolves() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("plugins");
        write(&root.join("demo/lua/helper.lua"), "return {}");

        let resolved = module_file(&root, Path::new("demo/lua/helper")).expect("no collision");
        assert_eq!(
            resolved,
            Some(std::fs::canonicalize(root.join("demo/lua/helper.lua")).unwrap()),
            "the legacy extension must keep resolving"
        );
    }

    /// Two extensions of one name RESOLVE TO NOTHING, at every depth.
    ///
    /// This used to prefer `.luau` silently. An entry point in the same state
    /// was refused and both files named, so one mistake had two answers
    /// depending on whether the file was an entry point — a line no author
    /// would predict, and the silent half is the one where an edit to the
    /// wrong file appears to do nothing.
    #[test]
    fn two_extensions_of_one_name_are_refused_by_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("plugins");
        write(&root.join("helper.lua"), "return { which = 'lua' }");
        write(&root.join("helper.luau"), "return { which = 'luau' }");
        let refused = module_file(&root, Path::new("helper"))
            .expect_err("a collision must refuse, not pick the preferred extension")
            .to_string();
        assert!(
            refused.contains("helper.lua") && refused.contains("helper.luau"),
            "the refusal must name BOTH files, not report the module missing: {refused}"
        );

        // The same rule one level down, where the name is a directory.
        write(&root.join("mod/init.lua"), "return {}");
        write(&root.join("mod/init.luau"), "return {}");
        assert!(
            module_file(&root, Path::new("mod")).is_err(),
            "a directory entry point collides the same way"
        );

        // And the ordinary case still resolves: one of the two, not both.
        assert!(
            matches!(module_file(&root, Path::new("alone")), Ok(None)),
            "nothing there resolves to nothing"
        );
        write(&root.join("alone.luau"), "return {}");
        assert!(
            matches!(module_file(&root, Path::new("alone")), Ok(Some(_))),
            "one file of the pair still resolves"
        );
    }
}
