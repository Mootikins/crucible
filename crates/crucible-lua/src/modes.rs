//! Modes, defined in Lua.
//!
//! ```lua
//! cru.modes.auto   = { tools = "*", permissions = "allow" }
//! cru.modes.review = {
//!   tools = { "read_*", "*_search", "bash" },
//!   permissions = { default = "deny", allow = { "bash:rg *", "bash:grep *" } },
//! }
//! cru.modes.review = nil   -- and it's gone
//! ```
//!
//! ```fennel
//! (set cru.modes.review {:tools ["read_*" "bash"]
//!                        :permissions {:default :deny :allow ["bash:rg *"]}})
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeStance {
    /// Prompt the user. The interactive default.
    #[default]
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
///
/// Patterns use the shared glob syntax from `crucible_core::utils::glob_match`
/// — `*`, `?`, `[a-z]`, `{a,b}` — the same one `crucible.on`'s `pattern`
/// accepts.
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
            // The canonical matcher, shared with `crucible.on`'s `pattern`
            // (`handlers/registry.rs`) and the MCP gateway. A mode's `tools`
            // globs and a hook's `pattern` globs must be the same language;
            // this file previously hand-rolled a reduced `*`-only variant,
            // so `{read,write}_*` worked in one and silently matched nothing
            // in the other.
            Self::Patterns(patterns) => patterns
                .iter()
                .any(|p| crucible_core::utils::glob_match(p, tool_name)),
        }
    }
}

/// What a mode permits, once its tools are visible.
///
/// The rule lists use the SAME `tool:pattern` vocabulary as the `[permissions]`
/// config (`"bash:rg *"`), and the daemon evaluates them with the same engine
/// — including its chained-command handling, so `bash:rg *` does not quietly
/// admit `rg foo && rm -rf /`. Reusing that grammar is the point: a mode
/// should not invent a second, subtly-different way to say the same thing.
///
/// `permissions = "allow"` is shorthand for `{ default = "allow" }`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModePermissions {
    pub default: ModeStance,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub ask: Vec<String>,
}

impl ModePermissions {
    /// True when only the default stance is set, so callers can skip building
    /// an engine for the common case.
    pub fn has_rules(&self) -> bool {
        !self.allow.is_empty() || !self.deny.is_empty() || !self.ask.is_empty()
    }
}

/// One mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeDefinition {
    pub name: String,
    pub description: Option<String>,
    pub tools: ToolSelector,
    pub permissions: ModePermissions,
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

fn parse_stance(mode: &str, s: &str) -> LuaResult<ModeStance> {
    ModeStance::parse(s).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "cru.modes.{mode}.permissions must be \"ask\", \"allow\" or \"deny\", got {s:?}"
        ))
    })
}

fn string_list(table: &Table, key: &str) -> LuaResult<Vec<String>> {
    match table.get::<Value>(key) {
        Ok(Value::Nil) => Ok(Vec::new()),
        Ok(Value::Table(t)) => {
            let mut out = Vec::new();
            for v in t.sequence_values::<String>() {
                out.push(v?);
            }
            Ok(out)
        }
        _ => Err(mlua::Error::runtime(format!(
            "permissions.{key} must be a list of \"tool:pattern\" strings"
        ))),
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

    let permissions = match table.get::<Value>("permissions") {
        Ok(Value::Nil) => ModePermissions::default(),
        Ok(Value::String(s)) => ModePermissions {
            default: parse_stance(name, &s.to_str()?)?,
            ..Default::default()
        },
        Ok(Value::Table(t)) => {
            let default = match t.get::<Option<String>>("default")? {
                Some(s) => parse_stance(name, &s)?,
                None => ModeStance::Ask,
            };
            ModePermissions {
                default,
                allow: string_list(&t, "allow")?,
                deny: string_list(&t, "deny")?,
                ask: string_list(&t, "ask")?,
            }
        }
        _ => {
            return Err(mlua::Error::runtime(format!(
                "cru.modes.{name}.permissions must be a stance string or a rules table"
            )))
        }
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
            if mode.permissions.has_rules() {
                let p = lua.create_table()?;
                p.set("default", mode.permissions.default.as_str())?;
                p.set("allow", lua.create_sequence_from(mode.permissions.allow)?)?;
                p.set("deny", lua.create_sequence_from(mode.permissions.deny)?)?;
                p.set("ask", lua.create_sequence_from(mode.permissions.ask)?)?;
                t.set("permissions", p)?;
            } else {
                t.set("permissions", mode.permissions.default.as_str())?;
            }
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
        assert_eq!(mode.permissions.default, ModeStance::Allow);
    }

    #[test]
    fn tools_default_to_everything_and_permissions_to_ask() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.normal = {}"#).exec().unwrap();

        let mode = registry.get("normal").unwrap();
        assert_eq!(mode.tools, ToolSelector::All);
        assert_eq!(
            mode.permissions.default,
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
        assert_eq!(
            registry.get("a").unwrap().permissions.default,
            ModeStance::Allow
        );
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
        assert_eq!(mode.permissions.default, ModeStance::Ask);
    }

    /// The gap a name-glob cannot close: "review may use bash, but only for
    /// rg and grep". Tool selectors gate VISIBILITY; what may be done with a
    /// visible tool is a permission rule, in the same `tool:pattern` grammar
    /// the `[permissions]` config already uses.
    #[test]
    fn a_mode_can_carry_permission_rules() {
        let (lua, registry) = lua_with_modes();
        lua.load(
            r#"cru.modes.review = {
                 tools = { "read_*", "bash" },
                 permissions = {
                   default = "deny",
                   allow = { "bash:rg *", "bash:grep *" },
                 },
               }"#,
        )
        .exec()
        .unwrap();

        let mode = registry.get("review").unwrap();
        assert!(mode.tools.matches("bash"), "bash must be visible");
        assert_eq!(mode.permissions.default, ModeStance::Deny);
        assert_eq!(mode.permissions.allow, vec!["bash:rg *", "bash:grep *"]);
        assert!(mode.permissions.has_rules());
    }

    /// The string form stays, because most modes have nothing to say beyond
    /// their stance and a rules table would be ceremony.
    #[test]
    fn a_stance_string_is_shorthand_for_a_rules_table() {
        let (lua, registry) = lua_with_modes();
        lua.load(r#"cru.modes.auto = { permissions = "allow" }"#)
            .exec()
            .unwrap();

        let mode = registry.get("auto").unwrap();
        assert_eq!(mode.permissions.default, ModeStance::Allow);
        assert!(
            !mode.permissions.has_rules(),
            "no rules means callers can skip building an engine"
        );
    }

    #[test]
    fn rules_read_back_as_a_table_and_a_bare_stance_as_a_string() {
        let (lua, _) = lua_with_modes();
        let rule: String = lua
            .load(
                r#"cru.modes.a = { permissions = { default = "deny", allow = { "bash:rg *" } } }
                   return cru.modes.a.permissions.allow[1]"#,
            )
            .eval()
            .unwrap();
        assert_eq!(rule, "bash:rg *");

        let stance: String = lua
            .load(
                r#"cru.modes.b = { permissions = "allow" }
                   return cru.modes.b.permissions"#,
            )
            .eval()
            .unwrap();
        assert_eq!(stance, "allow");
    }

    #[test]
    fn a_malformed_rule_list_is_an_error() {
        let (lua, _) = lua_with_modes();
        let err = lua
            .load(r#"cru.modes.a = { permissions = { allow = "bash:rg *" } }"#)
            .exec()
            .unwrap_err();
        assert!(err.to_string().contains("list of"), "got: {err}");
    }

    /// The behaviour the deleted hand-rolled matcher had, re-asserted against
    /// the shared one so the swap is a behaviour check rather than a rename.
    #[test]
    fn tool_selectors_use_the_shared_glob_syntax() {
        let sel = |p: &str| ToolSelector::Patterns(vec![p.to_string()]);

        assert!(sel("read_*").matches("read_file"));
        assert!(sel("*_search").matches("semantic_search"));
        assert!(sel("*").matches("anything"));
        assert!(sel("a*c").matches("abc"));
        assert!(sel("a*c").matches("ac"));
        assert!(!sel("read_*").matches("write_file"));
        assert!(!sel("*_search").matches("search_notes"));
        assert!(!sel("exact").matches("exactly"));
        assert!(sel("exact").matches("exact"));
        // A trailing literal still has to reach the end.
        assert!(!sel("*_search").matches("text_search_v2"));
        assert!(sel("*_search*").matches("text_search_v2"));
    }

    /// The syntax the hand-rolled matcher silently could NOT do. These are
    /// the assertions that would have caught the divergence: each works in
    /// `crucible.on`'s `pattern` and matched nothing in a mode's `tools`.
    #[test]
    fn tool_selectors_accept_the_full_glob_syntax_hooks_accept() {
        let sel = |p: &str| ToolSelector::Patterns(vec![p.to_string()]);

        assert!(sel("{read,write}_file").matches("write_file"));
        assert!(sel("read_?").matches("read_a"));
        assert!(!sel("read_?").matches("read_ab"));
        assert!(sel("tool:[a-z]*").matches("tool:grep"));
    }
}
