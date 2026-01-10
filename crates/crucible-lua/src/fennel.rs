//! Fennel compiler integration
//!
//! Fennel is a Lisp that compiles to Lua. This module embeds the
//! Fennel compiler for runtime compilation of .fnl files.
//!
//! ## Why Fennel?
//!
//! - S-expressions are unambiguous (great for LLM code generation)
//! - Macros allow users to define DSLs
//! - Pattern matching built-in
//! - Compiles to clean Lua with zero runtime overhead
//!
//! ## Example
//!
//! ```fennel
//! (fn handler [args]
//!   {:result (+ args.x args.y)})
//! ```
//!
//! Compiles to Lua:
//! ```lua
//! local function handler(args)
//!   return {result = (args.x + args.y)}
//! end
//! ```
//!
//! ## Tool Schema Extraction
//!
//! Use LDoc-style annotations for tool schemas (works with plain Lua too):
//!
//! ```fennel
//! ;;; Search the knowledge base
//! ;; @tool
//! ;; @param query string The search term
//! ;; @param limit number? Maximum results
//! (fn search [query limit]
//!   (kb.search query (or limit 10)))
//! ```

use crate::error::LuaError;
use mlua::{Function, Lua};

// These are only used in the compile_fennel standalone function
#[allow(unused_imports)]
use mlua::{LuaOptions, StdLib};

/// The bundled Fennel compiler (~255KB)
///
/// Standard Fennel compiler for Lua 5.4. Compiles Fennel (a Lisp) to Lua.
const FENNEL_SOURCE: &str = include_str!("../vendor/fennel.lua");

/// Fennel compiler wrapper
pub struct FennelCompiler {
    /// Cached compile function
    compile_fn_key: mlua::RegistryKey,
}

impl FennelCompiler {
    /// Initialize Fennel in the given Lua state
    pub fn new(lua: &Lua) -> Result<Self, LuaError> {
        // Load Fennel into the Lua state and capture return value
        // fennel.lua returns a module table, it doesn't set a global
        let fennel: mlua::Table = lua
            .load(FENNEL_SOURCE)
            .set_name("fennel")
            .eval()
            .map_err(|e| LuaError::FennelCompile(format!("Failed to load Fennel: {}", e)))?;

        // Store as global for compatibility with code that expects `fennel` global
        lua.globals()
            .set("fennel", fennel.clone())
            .map_err(|e| LuaError::FennelCompile(format!("Failed to set fennel global: {}", e)))?;

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
    ///
    /// Requires the Lua state reference since the compiler function is stored
    /// in the Lua registry.
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
    // Fennel requires the debug library for proper compilation
    // SAFETY: We're only loading Fennel and compiling, no user code execution
    let lua = unsafe {
        Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
    };

    // Load Fennel - it returns a module, doesn't set global
    let fennel: mlua::Table = lua
        .load(FENNEL_SOURCE)
        .set_name("fennel")
        .eval()
        .map_err(|e| LuaError::FennelCompile(format!("Failed to load Fennel: {}", e)))?;

    // Compile
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

        assert!(result.is_ok(), "Should compile simple Fennel: {:?}", result);
    }

    #[test]
    fn test_fennel_macro() {
        // Simple macro that doubles a value
        let result = compile_fennel(
            r#"
            (macro double [x]
              `(* 2 ,x))

            (double 21)
            "#,
        );

        assert!(result.is_ok(), "Should compile macro: {:?}", result);

        let compiled = result.unwrap();
        assert!(
            compiled.contains("42") || compiled.contains("(2 * 21)"),
            "Macro should expand: {}",
            compiled
        );
    }
}
