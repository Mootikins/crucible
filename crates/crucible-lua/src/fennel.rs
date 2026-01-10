//! Fennel compiler integration
//!
//! Fennel is a Lisp that compiles to Lua. This module embeds the
//! Fennel compiler for runtime compilation of .fnl files.
//!
//! ## Why Fennel?
//!
//! - S-expressions are unambiguous (great for LLM code generation)
//! - Macros allow users to define DSLs
//! - Compiles to clean Lua with zero runtime overhead
//!
//! ## Luau Type Support
//!
//! When the `luau` feature is enabled, uses a modified Fennel compiler
//! that supports type annotations:
//!
//! ```fennel
//! (fn add [a :number b :number] :-> number
//!   (+ a b))
//! ```
//!
//! Compiles to Luau with types:
//! ```lua
//! local function add(a: number, b: number): number
//!   return (a + b)
//! end
//! ```
//!
//! ## Standard Usage
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

// These are only used in the compile_fennel standalone function
#[allow(unused_imports)]
use mlua::{LuaOptions, StdLib};

/// The bundled Fennel compiler (fennel-luau fork with type annotation support)
///
/// This is the fennel-luau fork (~313KB) which supports `--target luau` for
/// emitting Luau type annotations. Standard Fennel code compiles normally.
///
/// Note: The Fennel compiler requires Lua 5.4 (uses package.preload which Luau lacks).
/// When you need Luau runtime, pre-compile your .fnl files first.
const FENNEL_SOURCE: &str = include_str!("../vendor/fennel-luau.lua");

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

    /// Compile Fennel source to Luau with type annotations
    ///
    /// Uses `--target luau` to emit Luau-style type annotations.
    /// The compiled output can be parsed by full_moon for schema extraction.
    ///
    /// Note: This requires the fennel-luau fork. When using the standard
    /// Fennel compiler, this will compile but type annotations will be ignored.
    ///
    /// # Example
    ///
    /// ```fennel
    /// (fn search [query :string limit :number] :-> (array Result)
    ///   [...])
    /// ```
    ///
    /// Compiles to:
    /// ```lua
    /// local function search(query: string, limit: number): {Result}
    ///   return {...}
    /// end
    /// ```
    pub fn compile_to_luau(&self, lua: &Lua, source: &str) -> Result<String, LuaError> {
        let compile_fn: Function = lua.registry_value(&self.compile_fn_key)?;

        // Create options table with target = "luau"
        let opts = lua
            .create_table()
            .map_err(|e| LuaError::FennelCompile(format!("Failed to create options: {}", e)))?;
        opts.set("target", "luau")
            .map_err(|e| LuaError::FennelCompile(format!("Failed to set target: {}", e)))?;

        let result: String = compile_fn
            .call((source, opts))
            .map_err(|e| LuaError::FennelCompile(format!("Fennel compilation failed: {}", e)))?;

        Ok(result)
    }

    /// Compile Fennel source to plain Lua (no type annotations)
    ///
    /// Use this when you need standard Lua output regardless of features.
    pub fn compile_to_lua(&self, lua: &Lua, source: &str) -> Result<String, LuaError> {
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

    // These tests verify the fennel-luau fork can compile typed Fennel to Luau.
    // The compiler runs in Lua 5.4 but outputs Luau-compatible code with types.

    #[test]
    fn test_compile_typed_fennel_to_luau() {
        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
        };
        let compiler = FennelCompiler::new(&lua).expect("Should create compiler");

        let source = "(fn add [a :number b :number] :-> number (+ a b))";
        let result = compiler.compile_to_luau(&lua, source);

        assert!(result.is_ok(), "Should compile typed Fennel: {:?}", result);

        let compiled = result.unwrap();
        assert!(
            compiled.contains("a: number"),
            "Should have typed param a: {}",
            compiled
        );
        assert!(
            compiled.contains("b: number"),
            "Should have typed param b: {}",
            compiled
        );
        assert!(
            compiled.contains("): number"),
            "Should have return type: {}",
            compiled
        );
    }

    #[test]
    fn test_compile_deftype_to_luau() {
        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
        };
        let compiler = FennelCompiler::new(&lua).expect("Should create compiler");

        let source = "(deftype UserId number)";
        let result = compiler.compile_to_luau(&lua, source);

        assert!(result.is_ok(), "Should compile deftype: {:?}", result);

        let compiled = result.unwrap();
        assert!(
            compiled.contains("type UserId = number"),
            "Should have type alias: {}",
            compiled
        );
    }

    #[test]
    fn test_compile_generic_fn_to_luau() {
        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
        };
        let compiler = FennelCompiler::new(&lua).expect("Should create compiler");

        let source = "(fn identity [T] [x :T] :-> T x)";
        let result = compiler.compile_to_luau(&lua, source);

        assert!(result.is_ok(), "Should compile generic fn: {:?}", result);

        let compiled = result.unwrap();
        assert!(
            compiled.contains("identity<T>"),
            "Should have generic param: {}",
            compiled
        );
        assert!(
            compiled.contains("x: T"),
            "Should have typed param using generic: {}",
            compiled
        );
    }
}
