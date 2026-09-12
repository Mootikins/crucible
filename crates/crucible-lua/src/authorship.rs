//! Which author a Lua config write belongs to.
//!
//! Two kinds of Lua file write the config store, and they hold different
//! authority. The user's `init.lua` is the human's own line: it re-applies at
//! every boot, so a machine write that replaced it would act nowhere. A
//! plugin's `setup()` supplies a default: plugins run during the evaluation
//! of `init.lua`, and if every plugin write counted as the human's, nearly
//! every key would lock and the settings UI would refuse almost every save.
//!
//! **The layer follows the caller, not the phase.** The running plugin
//! context cannot answer this. `require("alpha").setup{}` written by the user
//! runs alpha's file with no plugin context installed, and a plugin that
//! calls its own `setup` from a handler runs with one. The FILE that holds
//! the call is the honest signal, and the chunk name carries it.
//!
//! **One caller has no file, and the owner answers for it.** A `lua.eval`
//! arrives over a socket and its chunk name is `=lua.eval`, which matches no
//! root here, so [`AuthorRoots::classify`] would fall back to the human layer
//! and pin a leaf that no file holds. That case is decided BEFORE this module
//! is reached, by `crate::plugin_context::Owner::config_layer`. Nothing in
//! this module changes for it: the rule above still holds wherever a file
//! exists.
//!
//! The chunk name must come from `DebugSource::source`, never `short_src`.
//! `short_src` is the printable form: Luau truncates it to fit an error
//! message, so a real path under a long directory stops matching its own
//! root and every write from it is misfiled.

use crucible_core::config::SourceTag;
use std::path::{Path, PathBuf};

/// The directories that decide who wrote a config line.
///
/// Empty by default, which classifies every write as the human's. That is the
/// safe absence: a wrong pin is visible and the user is told the file and the
/// line, while a wrong demotion silently lets a saved value shadow a line the
/// human wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorRoots {
    /// Directories whose Lua files carry the operator's own authority: the
    /// config directory that holds `init.lua`, and what it includes.
    config: Vec<PathBuf>,
    /// Directories that hold one directory per plugin. The child directory's
    /// name is the plugin's name.
    plugins: Vec<PathBuf>,
}

impl AuthorRoots {
    /// The roots as the boot resolves them.
    pub fn new(config: Vec<PathBuf>, plugins: Vec<PathBuf>) -> Self {
        Self { config, plugins }
    }

    /// Learn one more plugin root, after the boot resolved the list.
    ///
    /// The boot enumerates the roots that EXIST while it runs, so a plugin
    /// directory created later — by a runtime install — is not in the list
    /// and every write from it would be misfiled as the human's. Answers
    /// whether the root was new, so the caller logs the change once.
    pub fn add_plugin_root(&mut self, root: PathBuf) -> bool {
        if self.plugins.contains(&root) {
            return false;
        }
        self.plugins.push(root);
        true
    }

    /// The layer a write from `chunk` lands in.
    ///
    /// `chunk` is the raw chunk name. `modules.rs` names every module
    /// `@<full path>`; the config loader names `init.lua` by its bare path.
    /// Both forms are accepted, so the leading `@` comes off first.
    pub fn classify(&self, chunk: &str, line: Option<u32>) -> SourceTag {
        let file = chunk.strip_prefix('@').unwrap_or(chunk);
        let path = Path::new(file);

        // The longest matching root wins, and a plugin root wins a tie: a
        // plugin directory can sit under the config directory, and the more
        // specific root names the real author. Reading the config root first
        // would pin every plugin that ships under `~/.config/crucible`.
        let plugin_root = longest_root(&self.plugins, path);
        let config_root = longest_root(&self.config, path);
        let plugin_wins = match (plugin_root, config_root) {
            (Some(plugin), Some(config)) => depth(plugin) >= depth(config),
            (Some(_), None) => true,
            (None, _) => false,
        };

        match plugin_root
            .filter(|_| plugin_wins)
            .and_then(|root| plugin_name(root, path))
        {
            Some(plugin) => SourceTag::PluginDefault {
                plugin,
                file: file.to_string(),
                line,
            },
            None => SourceTag::Lua {
                file: file.to_string(),
                line,
            },
        }
    }
}

/// The most specific root in `roots` that contains `path`.
///
/// `Path::starts_with` compares whole components, so `/home/user/config-old`
/// is not under `/home/user/config`.
fn longest_root<'a>(roots: &'a [PathBuf], path: &Path) -> Option<&'a Path> {
    roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| depth(root))
        .map(PathBuf::as_path)
}

fn depth(path: &Path) -> usize {
    path.components().count()
}

/// The plugin that owns `path`: the directory directly under `root`.
///
/// A file lying loose in the root belongs to no plugin, and no name can be
/// invented for it, so it falls back to the human layer with the rest of the
/// unidentified.
fn plugin_name(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    // The last component is the file itself, so a name needs one more after it.
    components.next()?;
    Some(first.as_os_str().to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> AuthorRoots {
        AuthorRoots::new(
            vec![PathBuf::from("/home/user/.config/crucible")],
            vec![PathBuf::from("/home/user/.local/share/crucible/plugins")],
        )
    }

    #[test]
    fn a_write_from_the_config_directory_is_the_humans_own_line() {
        let tag = roots().classify("@/home/user/.config/crucible/init.lua", Some(7));
        assert_eq!(
            tag,
            SourceTag::Lua {
                file: "/home/user/.config/crucible/init.lua".to_string(),
                line: Some(7),
            }
        );
    }

    #[test]
    fn a_write_from_a_plugin_directory_is_that_plugins_default() {
        let tag = roots().classify(
            "@/home/user/.local/share/crucible/plugins/alpha/init.lua",
            Some(3),
        );
        assert_eq!(
            tag,
            SourceTag::PluginDefault {
                plugin: "alpha".to_string(),
                file: "/home/user/.local/share/crucible/plugins/alpha/init.lua".to_string(),
                line: Some(3),
            }
        );
    }

    /// The config loader sets the chunk name without the `@`.
    #[test]
    fn a_chunk_name_without_the_at_sign_classifies_the_same() {
        let tag = roots().classify("/home/user/.config/crucible/init.lua", None);
        assert_eq!(tag.short(), "lua");
    }

    /// A plugin directory under the config directory belongs to the plugin.
    #[test]
    fn the_more_specific_root_names_the_author() {
        let nested = AuthorRoots::new(
            vec![PathBuf::from("/cfg")],
            vec![PathBuf::from("/cfg/plugins")],
        );
        let tag = nested.classify("@/cfg/plugins/beta/lua/opts.lua", Some(1));
        assert_eq!(tag.short(), "plugin");
        assert!(tag.rank() < SourceTag::Settings.rank());
    }

    /// A sibling directory whose name merely starts with the root's text is
    /// not under it.
    #[test]
    fn a_neighbour_that_shares_the_root_text_is_not_under_the_root() {
        let tag = roots().classify("@/home/user/.config/crucible-old/init.lua", Some(1));
        assert_eq!(
            tag.short(),
            "lua",
            "an unknown file falls back to the human layer"
        );
    }

    /// The runtime-install case: the plugins directory did not exist while
    /// the boot resolved the roots, so the classifier has to learn it before
    /// the installed plugin's `setup()` runs.
    #[test]
    fn a_root_learned_after_the_boot_classifies_its_plugins() {
        let mut roots = AuthorRoots::new(vec![PathBuf::from("/cfg")], Vec::new());
        assert_eq!(
            roots
                .classify("@/cfg/plugins/gamma/init.lua", Some(4))
                .short(),
            "lua",
            "an unknown directory is the human's, which is the defect this fixes"
        );

        assert!(roots.add_plugin_root(PathBuf::from("/cfg/plugins")));
        assert_eq!(
            roots.classify("@/cfg/plugins/gamma/init.lua", Some(4)),
            SourceTag::PluginDefault {
                plugin: "gamma".to_string(),
                file: "/cfg/plugins/gamma/init.lua".to_string(),
                line: Some(4),
            }
        );

        assert!(
            !roots.add_plugin_root(PathBuf::from("/cfg/plugins")),
            "a second install of the same root is not a new root"
        );
    }

    #[test]
    fn an_unidentified_chunk_stays_with_the_human_layer() {
        let tag = roots().classify("=[C]", None);
        assert_eq!(tag.short(), "lua");
    }

    /// A truncated chunk name is what `short_src` hands back for a long path.
    /// It must not be mistaken for a file under the config root.
    #[test]
    fn a_truncated_chunk_name_matches_no_root() {
        let tag = roots().classify("...crucible/plugins/alpha/init.lua", Some(2));
        assert_eq!(tag.short(), "lua");
    }
}
