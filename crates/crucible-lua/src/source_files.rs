//! Which files are Lua source, and what `require` tries.
//!
//! Luau's own tooling — `luau-lsp`, the VS Code extension, the Luau
//! playground — treats `.luau` as the native extension and `.lua` as the
//! legacy one. Crucible runs Luau and names every file `.lua`, so an editor
//! opening a plugin gets no Luau support until the user configures it by hand.
//!
//! Both extensions work, and `.luau` is preferred. A rename alone would break
//! every plugin already on a user's disk, and Crucible has no way to reach
//! those. So the resolution order below puts `.luau` first and keeps `.lua`
//! working for good.
//!
//! ## One decision, one place
//!
//! Eleven sites across four crates used to decide "is this a Lua file" or
//! "what does `require` try", each with its own literal `"lua"`. They now all
//! call in here. That is what makes the extension a decision rather than a
//! convention: adding `.luau` to ten of eleven sites produces a plugin that
//! discovers but does not load, or loads but does not typecheck.
//!
//! ## Ambiguity is refused, not resolved — everywhere
//!
//! A directory holding both `config.luau` and `config.lua` has no obvious
//! answer, and picking one silently means an edit to the wrong file appears to
//! do nothing.
//!
//! Both [`init_file`] and [`collides`] report that collision, and the two
//! callers that resolve a name — plugin discovery and `require` — refuse on
//! it. This used to be true of the ENTRY POINT only: `init.luau` beside
//! `init.lua` was refused and named, while `helper.luau` beside `helper.lua`
//! was silently resolved in favour of `.luau` and reported by nothing, not
//! even `cru plugin check`. That is the same mistake by the same author, and
//! the line between the two cases — entry point versus submodule — is not one
//! anyone would predict.
//!
//! [`module_candidates`] only ORDERS the names. The preference it encodes
//! decides nothing once a collision is refused; it still matters for the
//! ordinary case of a directory holding one of the two.

use std::path::{Path, PathBuf};

/// The extensions Crucible reads, in preference order.
pub const SOURCE_EXTENSIONS: [&str; 2] = ["luau", "lua"];

/// The extension a NEW file gets.
pub const PREFERRED_EXTENSION: &str = "luau";

/// Whether this path names Lua source.
pub fn is_lua_source(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext))
}

/// The entry-point file names for a plugin or module directory, preferred
/// first.
pub fn init_file_names() -> [String; 2] {
    SOURCE_EXTENSIONS.map(|ext| format!("init.{ext}"))
}

/// The `init.*` file in `dir`, if exactly one is there.
///
/// `Err` names both when both exist. A plugin directory holding `init.luau`
/// and `init.lua` is a plugin whose author edited one of them and watched
/// nothing happen.
pub fn init_file(dir: &Path) -> Result<Option<PathBuf>, Ambiguous> {
    let found: Vec<PathBuf> = init_file_names()
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found.into_iter().next().expect("one"))),
        _ => Err(Ambiguous(found)),
    }
}

/// Every file `require("<relative>")` may resolve to under `root`, preferred
/// first: `<relative>.luau`, `<relative>.lua`, then the same two as
/// `<relative>/init.*`.
///
/// ORDERING only. Whether a collision between two of these is acceptable is
/// [`collides`]'s question, and the answer is no.
pub fn module_candidates(root: &Path, relative: &Path) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(4);
    for ext in SOURCE_EXTENSIONS {
        out.push(root.join(relative).with_extension(ext));
    }
    for name in init_file_names() {
        out.push(root.join(relative).join(name));
    }
    out
}

/// The two files that answer to one name, if both are there.
///
/// A collision is two EXTENSIONS of one path — `helper.luau` beside
/// `helper.lua`, or `helper/init.luau` beside `helper/init.lua`. A bare file
/// beside a directory of the same name (`helper.lua` and `helper/init.lua`)
/// is the ordinary Lua shadowing rule and is left alone.
pub fn collides(root: &Path, relative: &Path) -> Option<Ambiguous> {
    for stem in [root.join(relative), root.join(relative).join("init")] {
        let both: Vec<PathBuf> = SOURCE_EXTENSIONS
            .iter()
            .map(|ext| stem.with_extension(ext))
            .filter(|path| path.is_file())
            .collect();
        if both.len() > 1 {
            return Some(Ambiguous(both));
        }
    }
    None
}

/// Two files that answer to one name.
#[derive(Debug, Clone)]
pub struct Ambiguous(pub Vec<PathBuf>);

impl std::fmt::Display for Ambiguous {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "two files answer to the same module name, so which one loads \
             would depend on the search order rather than on anything the \
             author wrote: {}. Delete one.",
            self.0
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" and ")
        )
    }
}

impl std::error::Error for Ambiguous {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luau_is_preferred_and_lua_still_resolves() {
        let candidates = module_candidates(Path::new("/root"), Path::new("config"));
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/root/config.luau"),
                PathBuf::from("/root/config.lua"),
                PathBuf::from("/root/config/init.luau"),
                PathBuf::from("/root/config/init.lua"),
            ],
            "a bare file beats a directory, and .luau beats .lua within each"
        );
    }

    #[test]
    fn both_extensions_are_source_and_nothing_else_is() {
        assert!(is_lua_source(Path::new("a/b.lua")));
        assert!(is_lua_source(Path::new("a/b.luau")));
        assert!(!is_lua_source(Path::new("a/b.luac")));
        assert!(!is_lua_source(Path::new("a/b.md")));
        assert!(!is_lua_source(Path::new("a/lua")));
    }

    #[test]
    fn one_init_file_resolves_and_two_are_refused() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(init_file(tmp.path()).unwrap().is_none(), "neither present");

        std::fs::write(tmp.path().join("init.lua"), "return {}").unwrap();
        assert_eq!(
            init_file(tmp.path()).unwrap(),
            Some(tmp.path().join("init.lua")),
            "the legacy extension still resolves on its own"
        );

        std::fs::write(tmp.path().join("init.luau"), "return {}").unwrap();
        let err = init_file(tmp.path()).expect_err("two files, one name");
        let message = err.to_string();
        assert!(
            message.contains("init.luau") && message.contains("init.lua"),
            "the refusal must name both files: {message}"
        );
    }
}
