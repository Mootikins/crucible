//! Modes, defined in Lua.
//!
//! ```lua
//! cru.modes.auto   = { tools = "*",       permissions = "allow" }
//! cru.modes.plan   = { tools = READ_ONLY, permissions = "deny"  }
//! cru.modes.review = { tools = { "read_*", "*_search" }, permissions = "ask" }
//! cru.modes.plan   = nil   -- and it's gone
//! ```
//!
//! ```fennel
//! (set cru.modes.review {:tools ["read_*" "*_search"] :permissions :ask})
//! ```
//!
//! A mode is two things: which tools it exposes, and what it does when one of
//! them needs permission. Both were previously Rust constants
//! (`default_internal_modes`, `PLAN_TOOL_NAMES`) plus a pair of shipped Lua
//! hooks, so a user could add a *rule* for a mode but not a mode.
//!
//! ## What a mode is not
//!
//! Not a security boundary. A mode declares intent — "this one is for
//! reading" — and the things that must not be widenable live elsewhere and
//! stay unconditional: `[permissions]` deny rules, workspace containment, the
//! shell policy. That separation is what makes it safe for a mode to be
//! ordinary Lua data that any file can rewrite.
//!
//! ## Layering
//!
//! `permissions` is the mode's *default stance*. `cru.permissions.on_request`
//! hooks still run and still win, because a stance can only be static and
//! real policy often isn't. Same split as `cru.defaults` (a value) versus
//! `cru.on_session_start` (a decision).

use mlua::{Lua, MetaMethod, Result as LuaResult, Table, UserData, UserDataMethods, Value};
use std::sync::{Arc, RwLock};

/// What a mode does when a tool needs permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeStance {
    /// Prompt the user. The interactive default.
    Ask,
    /// Approve without asking.
    Allow,
    /// Refuse without asking.
    Deny,
}

impl ModeStance {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "ask" => Some(Self::Ask),
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

/// Which tools a mode exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSelector {
    /// Everything the agent has.
    All,
    /// Only names matching one of these glob patterns.
    Patterns(Vec<String>),
}

impl ToolSelector {
    pub fn matches(&self, tool_name: &str) -> bool {
        match self {
            Self::All => true,
            Self::Patterns(patterns) => patterns.iter().any(|p| glob_matches(p, tool_name)),
        }
    }
}

/// One mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeDefinition {
    pub name: String,
    pub description: Option<String>,
    pub tools: ToolSelector,
    pub permissions: ModeStance,
}

/// `*` matches any run of characters, including none. Everything else is
/// literal — deliberately not a full glob: tool names are flat identifiers,
/// and `?`/`[...]` would invite patterns nobody can read at a glance.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let mut segments = pattern.split('*');
    let Some(first) = segments.next() else {
        return pattern == name;
    };
    if !name.starts_with(first) {
        return false;
    }
    // No `*` at all: the whole pattern had to match exactly.
    let rest_patterns: Vec<&str> = segments.collect();
    if rest_patterns.is_empty() {
        return pattern == name;
    }

    let mut cursor = &name[first.len()..];
    for (i, segment) in rest_patterns.iter().enumerate() {
        let is_last = i == rest_patterns.len() - 1;
        if segment.is_empty() {
            // Trailing `*` swallows the remainder.
            if is_last {
                return true;
            }
            continue;
        }
        match cursor.find(segment) {
            Some(pos) => {
                if is_last && pos + segment.len() != cursor.len() {
                    // The last literal must land at the END unless the pattern
                    // ended with `*`.
                    let tail = &cursor[pos + segment.len()..];
                    if !tail.is_empty() {
                        // Try to find a later occurrence that does reach the end.
                        return cursor.ends_with(segment);
                    }
                }
                cursor = &cursor[pos + segment.len()..];
            }
            None => return false,
        }
    }
    true
}

/// Shared, ordered registry of modes.
///
/// Order is insertion order, because it is what a mode-cycling UI walks and a
/// map's iteration order would make that arbitrary between runs.
#[derive(Debug, Clone, Default)]
pub struct ModeRegistry {
    inner: Arc<RwLock<Vec<ModeDefinition>>>,
}

impl ModeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> Vec<ModeDefinition> {
        self.inner.read().expect("mode registry: poisoned").clone()
    }

    pub fn get(&self, name: &str) -> Option<ModeDefinition> {
        self.inner
            .read()
            .expect("mode registry: poisoned")
            .iter()
            .find(|m| m.name == name)
            .cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .read()
            .expect("mode registry: poisoned")
            .is_empty()
    }

    /// Insert or replace. Replacing keeps the original position, so redefining
    /// a shipped mode doesn't move it to the end of the user's mode cycle.
    pub fn set(&self, definition: ModeDefinition) {
        let mut guard = self.inner.write().expect("mode registry: poisoned");
        match guard.iter().position(|m| m.name == definition.name) {
            Some(i) => guard[i] = definition,
            None => guard.push(definition),
        }
    }

    pub fn remove(&self, name: &str) {
        self.inner
            .write()
            .expect("mode registry: poisoned")
            .retain(|m| m.name != name);
    }
}

fn definition_from_lua(name: &str, table: &Table) -> LuaResult<ModeDefinition> {
    let tools = match table.get::<Value>("tools") {
        Ok(Value::Nil) => ToolSelector::All,
        Ok(Value::String(s)) => {
            let s = s.to_str()?.to_string();
            if s == "*" {
                ToolSelector::All
            } else {
                ToolSelector::Patterns(vec![s])
            }
        }
        Ok(Value::Table(t)) => {
            let mut patterns = Vec::new();
            for pair in t.sequence_values::<String>() {
                patterns.push(pair?);
            }
            ToolSelector::Patterns(patterns)
        }
        _ => {
            return Err(mlua::Error::runtime(format!(
                "cru.modes.{name}.tools must be \"*\" or a list of patterns"
            )))
        }
    };

    let permissions = match table.get::<Option<String>>("permissions")? {
        None => ModeStance::Ask,
        Some(s) => ModeStance::parse(&s).ok_or_else(|| {
            mlua::Error::runtime(format!(
                "cru.modes.{name}.permissions must be \"ask\", \"allow\" or \"deny\", got {s:?}"
            ))
        })?,
    };

    Ok(ModeDefinition {
        name: name.to_string(),
        description: table.get::<Option<String>>("description")?,
        tools,
        permissions,
    })
}

impl UserData for ModeRegistry {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let Some(mode) = this.get(&key) else {
                return Ok(Value::Nil);
            };
            let t = lua.create_table()?;
            t.set("name", mode.name)?;
            if let Some(description) = mode.description {
                t.set("description", description)?;
            }
            match mode.tools {
                ToolSelector::All => t.set("tools", "*")?,
                ToolSelector::Patterns(patterns) => {
                    t.set("tools", lua.create_sequence_from(patterns)?)?
                }
            }
            t.set("permissions", mode.permissions.as_str())?;
            Ok(Value::Table(t))
        });

        methods.add_meta_method(
            MetaMethod::NewIndex,
            |_, this, (key, val): (String, Value)| match val {
                // `cru.modes.plan = nil` removes it. Shadowing a shipped mode
                // with an empty one would leave a broken entry in the cycle;
                // deleting says what you mean.
                Value::Nil => {
                    this.remove(&key);
                    Ok(())
                }
                Value::Table(t) => {
                    this.set(definition_from_lua(&key, &t)?);
                    Ok(())
                }
                _ => Err(mlua::Error::runtime(format!(
                    "cru.modes.{key} must be a table or nil"
                ))),
            },
        );
    }
}

/// Register `cru.modes` (and the `crucible.modes` alias) on `lua`.
pub fn register_modes(lua: &Lua, registry: ModeRegistry) -> LuaResult<()> {
    for namespace in ["cru", "crucible"] {
        crate::lua_util::get_or_create_namespace(lua, namespace)?.set("modes", registry.clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with_modes() -> (Lua, ModeRegistry) {
        let lua = Lua::new();
        let registry = ModeRegistry::new();
        register_modes(&lua, registry.clone()).unwrap();
        (lua, registry)
    }

    #[test]
    fn a_mode_can_be_declared_from_lua() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.auto = { tools = "*", permissions = "allow" }"#)
            .exec()
            .unwrap();

        let mode = registry.get("auto").expect("auto must be registered");
        assert_eq!(mode.tools, ToolSelector::All);
        assert_eq!(mode.permissions, ModeStance::Allow);
    }

    #[test]
    fn tools_default_to_everything_and_permissions_to_ask() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.normal = {}"#).exec().unwrap();

        let mode = registry.get("normal").unwrap();
        assert_eq!(mode.tools, ToolSelector::All);
        assert_eq!(
            mode.permissions,
            ModeStance::Ask,
            "an unspecified stance must prompt, never silently allow"
        );
    }

    #[test]
    fn a_tool_list_becomes_patterns() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.review = { tools = { "read_*", "grep" } }"#)
            .exec()
            .unwrap();

        let mode = registry.get("review").unwrap();
        assert!(mode.tools.matches("read_file"));
        assert!(mode.tools.matches("grep"));
        assert!(!mode.tools.matches("write_file"));
    }

    #[test]
    fn redefining_a_mode_keeps_its_position_in_the_cycle() {
        let (lua, registry) = lua_with_modes();
        lua.load(
            r#"cru.modes.a = {}
               cru.modes.b = {}
               cru.modes.a = { permissions = "allow" }"#,
        )
        .exec()
        .unwrap();

        let names: Vec<String> = registry.all().into_iter().map(|m| m.name).collect();
        assert_eq!(names, vec!["a", "b"], "a redefinition must not reorder");
        assert_eq!(registry.get("a").unwrap().permissions, ModeStance::Allow);
    }

    #[test]
    fn a_mode_can_be_removed() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.plan = {}"#).exec().unwrap();
        assert!(registry.get("plan").is_some());
        lua.load(r#"cru.modes.plan = nil"#).exec().unwrap();
        assert!(registry.get("plan").is_none());
    }

    #[test]
    fn a_mode_reads_back_as_a_table() {
        let (lua, _) = lua_with_modes();
        let stance: String = lua
            .load(
                r#"cru.modes.plan = { tools = { "read_*" }, permissions = "deny" }
                   return cru.modes.plan.permissions"#,
            )
            .eval()
            .unwrap();
        assert_eq!(stance, "deny");
    }

    #[test]
    fn an_unknown_mode_reads_as_nil() {
        let (lua, _) = lua_with_modes();
        let v: Value = lua.load(r#"return cru.modes.nope"#).eval().unwrap();
        assert!(matches!(v, Value::Nil));
    }

    /// A typo in a stance must not silently become "ask" — that would turn a
    /// mode a user believed was locked down into a prompting one.
    #[test]
    fn an_invalid_stance_is_an_error() {
        let (lua, _) = lua_with_modes();
        let err = lua
            .load(r#"cru.modes.x = { permissions = "allways" }"#)
            .exec()
            .unwrap_err();
        assert!(err.to_string().contains("permissions"), "got: {err}");
    }

    #[test]
    fn a_non_table_definition_is_an_error() {
        let (lua, _) = lua_with_modes();
        let err = lua.load(r#"cru.modes.x = 5"#).exec().unwrap_err();
        assert!(err.to_string().contains("table or nil"), "got: {err}");
    }

    #[cfg(feature = "fennel")]
    #[test]
    fn the_surface_reads_and_works_in_fennel() {
        let (lua, registry) = lua_with_modes();
        let src = crate::fennel::compile_fennel(
            r#"(set cru.modes.review {:tools ["read_*"] :permissions :ask})"#,
        )
        .expect("fennel must compile");
        lua.load(&src).exec().expect("compiled fennel must run");

        let mode = registry.get("review").expect("registered from fennel");
        assert!(mode.tools.matches("read_note"));
        assert_eq!(mode.permissions, ModeStance::Ask);
    }

    #[test]
    fn glob_star_matches_prefix_suffix_and_middle() {
        assert!(glob_matches("read_*", "read_file"));
        assert!(glob_matches("*_search", "semantic_search"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("a*c", "abc"));
        assert!(glob_matches("a*c", "ac"));
        assert!(!glob_matches("read_*", "write_file"));
        assert!(!glob_matches("*_search", "search_notes"));
        assert!(!glob_matches("exact", "exactly"));
        assert!(glob_matches("exact", "exact"));
    }

    /// `*_search` must not match a name that merely CONTAINS the suffix.
    #[test]
    fn a_trailing_literal_must_reach_the_end() {
        assert!(!glob_matches("*_search", "text_search_v2"));
        assert!(glob_matches("*_search*", "text_search_v2"));
    }
}
