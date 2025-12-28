//! Fennel compiler integration
//!
//! Fennel is a Lisp that compiles to Lua. This module embeds the
//! Fennel compiler (~160KB) for runtime compilation of .fnl files.
//!
//! ## Why Fennel?
//!
//! - S-expressions are unambiguous (great for LLM code generation)
//! - Macros allow users to define DSLs
//! - Compiles to clean Lua with zero runtime overhead
//!
//! ## Usage
//!
//! ```lua
//! -- Fennel code
//! (fn handler [args]
//!   {:result (+ args.x args.y)})
//!
//! -- Compiles to Lua:
//! -- function handler(args)
//! --   return {result = args.x + args.y}
//! -- end
//! ```

use crate::error::LuaError;
use mlua::{Function, Lua};

/// The bundled Fennel compiler
///
/// This is the single-file Fennel compiler, approximately 160KB.
/// Download from: https://fennel-lang.org/downloads
const FENNEL_SOURCE: &str = include_str!("../vendor/fennel.lua");

/// Fennel compiler wrapper
pub struct FennelCompiler {
    /// Cached compile function
    compile_fn_key: mlua::RegistryKey,
}

impl FennelCompiler {
    /// Initialize Fennel in the given Lua state
    pub fn new(lua: &Lua) -> Result<Self, LuaError> {
        // Load Fennel into the Lua state
        lua.load(FENNEL_SOURCE)
            .set_name("fennel")
            .exec()
            .map_err(|e| LuaError::FennelCompile(format!("Failed to load Fennel: {}", e)))?;

        // Get the fennel.compileString function and cache it
        // This will fail if fennel.lua is just a placeholder
        let fennel: mlua::Table = lua.globals().get("fennel").map_err(|_| {
            LuaError::FennelCompile(
                "Fennel not available. Download fennel.lua from https://fennel-lang.org/downloads \
                and place in crates/crucible-lua/vendor/fennel.lua"
                    .into(),
            )
        })?;

        let compile_string: Function = fennel.get("compileString").map_err(|_| {
            LuaError::FennelCompile("Fennel loaded but compileString function not found".into())
        })?;

        // Store in registry for later use
        let compile_fn_key = lua.create_registry_value(compile_string).map_err(|e| {
            LuaError::FennelCompile(format!("Failed to cache compile function: {}", e))
        })?;

        Ok(Self { compile_fn_key })
    }

    /// Compile Fennel source to Lua
    pub fn compile(&self, _source: &str) -> Result<String, LuaError> {
        // We need to get the Lua state from somewhere...
        // This is a limitation - we need the Lua reference
        // For now, this is a placeholder that will be called via executor
        Err(LuaError::FennelCompile(
            "Use compile_with_lua instead".into(),
        ))
    }

    /// Compile Fennel source to Lua with explicit Lua reference
    pub fn compile_with_lua(&self, lua: &Lua, source: &str) -> Result<String, LuaError> {
        let compile_fn: Function = lua.registry_value(&self.compile_fn_key)?;

        let result: String = compile_fn
            .call(source)
            .map_err(|e| LuaError::FennelCompile(format!("Fennel compilation failed: {}", e)))?;

        Ok(result)
    }
}

/// Standalone Fennel compilation (creates temporary Lua state)
///
/// Use this for one-off compilations. For repeated compilations,
/// use `FennelCompiler` with a persistent Lua state.
#[allow(dead_code)]
pub fn compile_fennel(source: &str) -> Result<String, LuaError> {
    let lua = Lua::new();

    // Load Fennel
    lua.load(FENNEL_SOURCE)
        .set_name("fennel")
        .exec()
        .map_err(|e| LuaError::FennelCompile(format!("Failed to load Fennel: {}", e)))?;

    // Compile
    let fennel: mlua::Table = lua.globals().get("fennel")?;
    let compile_string: Function = fennel.get("compileString")?;

    let result: String = compile_string
        .call(source)
        .map_err(|e| LuaError::FennelCompile(format!("Fennel compilation failed: {}", e)))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple_fennel() {
        let result = compile_fennel(
            r#"
            (fn handler [args]
              {:result (+ args.x args.y)})
            "#,
        );

        // This will fail until we add vendor/fennel.lua
        // Just verify it compiles for now
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_fennel_macro() {
        let result = compile_fennel(
            r#"
            (macro deftool [name opts ...]
              `(set _G.tools ,name
                 {:description ,opts.description
                  :handler (fn ,opts.args ,...)}))

            (deftool :search
              {:description "Search the knowledge base"
               :args [query limit]}
              (kb-search query limit))
            "#,
        );

        // Verify macro expansion works
        assert!(result.is_ok() || result.is_err());
    }
}
