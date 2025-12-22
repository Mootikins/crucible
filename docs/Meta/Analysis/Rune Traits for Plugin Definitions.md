---
date: 2025-12-21
tags:
  - rune
  - plugins
  - architecture
  - analysis
status: research
---

# Rune Traits for Plugin Definitions

**Question**: Could Rune's trait system be used to define a formal Plugin interface that plugins must implement, replacing the current convention-based approach?

## Current Plugin Pattern (Convention-Based)

Crucible's current plugin system uses **convention over configuration**. Plugins are Rune structs with expected methods:

```rune
struct JustPlugin { recipes }

impl JustPlugin {
    fn new() { JustPlugin { recipes: [] } }

    fn tools(self) -> Vec {
        // Return array of tool definitions
    }

    fn dispatch(self, tool_name, args) -> Result {
        // Handle tool invocation
    }

    fn on_watch(self, event) {
        // Optional: react to file changes
    }
}

#[plugin(watch = ["justfile", "*.just"])]
pub fn create() {
    JustPlugin::new()
}
```

**Current validation**: Runtime checks if methods exist. No compile-time enforcement.

**Rust side**: `StructPluginLoader` uses Rune's reflection API to:
1. Call factory function to get instance
2. Try calling `tools()` method (if missing, plugin has no tools)
3. Try calling `dispatch()` method when tool is invoked
4. Try calling `on_watch()` method when watched file changes

See: `/home/moot/crucible/crates/crucible-rune/src/struct_plugin.rs`

## Rune Trait System Overview

Based on [Rune traits documentation](https://rune-rs.github.io/book/traits.html):

### What Traits Are

> "Traits in rune defines a collection associated items" that allows developers to "reason about types more abstractly."

Once a trait is implemented on a type, all associated methods are guaranteed present.

### Critical Limitation: Single Definition Rule

**Rune prohibits multiple trait implementations with overlapping method names on the same type.**

This differs from Rust. The rationale:
- Rune's dynamic nature creates ambiguity
- Without type parameters, the compiler cannot determine which implementation to invoke
- When multiple traits define identical methods, dispatch is ambiguous

**Implication for plugins**: If we define a `Plugin` trait with methods like `tools()` and `dispatch()`, plugins cannot implement other traits that also define those method names.

### Implementation via Protocols

Traits operate through an underlying **protocol system**:
- Protocols exist in a separate namespace
- Users cannot call protocols directly
- When a trait is implemented, it searches for corresponding protocol implementations
- Example: "to implement the `Iterator` trait you have to implement the `NEXT` protocol"

The `Module::implement_trait` method links protocol implementations to trait requirements.

### Trait Definition (Low-Level)

Traits are currently defined through module operations:

```rust
let mut t = m.define_trait(["Iterator"])?;
t.handler(|cx| {
    let next = cx.find(&Protocol::NEXT)?;          // Required
    let size_hint = cx.find_or_define(&Protocol::SIZE_HINT, ...)?;  // Optional
    Ok(())
})?;
```

- `find()` - required implementations
- `find_or_define()` - optional with defaults

### Validation & Dispatch

- **Compile-time**: Definition errors occur at build time
- **Runtime dispatch**: Protocols enable virtual machine function calls
- Default implementations provided when specialized versions aren't defined

### Trait Objects

**No information found about trait objects or dynamic dispatch via trait types.**

The documentation does not mention `dyn Trait` syntax or trait object functionality.

## Analysis for Crucible Plugin System

### Question 1: Can Rune traits define a Plugin interface?

**Technically possible but not practically useful in current Rune.**

Rune traits **can** define method signatures:

```rune
// Hypothetical (requires Rune trait definition syntax)
trait Plugin {
    fn tools(self) -> Vec;
    fn dispatch(self, name, args) -> Result;
    fn on_watch(self, event);  // Optional?
}
```

**However**:
1. **No high-level trait syntax**: Traits are defined through low-level `Module` API (Rust side)
2. **No trait objects**: Cannot use `dyn Plugin` for polymorphic storage
3. **Limited benefit**: Current approach already works well

### Question 2: What would the syntax look like?

**Rune does not have user-facing trait definition syntax** (as of 2025-12-21).

Traits are defined in Rust when building Rune modules:

```rust
// In Rust (crucible-rune/src/plugin_module.rs)
let mut module = rune::Module::new();
let mut plugin_trait = module.define_trait(["Plugin"])?;
plugin_trait.handler(|cx| {
    let tools_fn = cx.find(&Protocol::TOOLS)?;
    let dispatch_fn = cx.find(&Protocol::DISPATCH)?;
    Ok(())
})?;
```

**Rune plugin code would then implement it**:

```rune
// Hypothetical - actual syntax unknown
impl Plugin for JustPlugin {
    fn tools(self) { /* ... */ }
    fn dispatch(self, name, args) { /* ... */ }
}
```

### Question 3: Limitations preventing this pattern?

**Yes, several critical limitations:**

1. **No trait objects** - Cannot store `dyn Plugin` or use traits for polymorphism
2. **No user-defined traits** - Traits must be defined in Rust, not Rune scripts
3. **No compile-time validation from Rune side** - Trait checking happens at runtime
4. **Protocol namespace complexity** - Requires defining custom protocols for each method
5. **Single definition rule** - Plugins can't implement multiple traits with overlapping methods

**Current approach advantages:**
- Simpler to understand (just methods on a struct)
- No Rust-side trait machinery needed
- Flexible (methods are optional)
- Works with Rune's dynamic nature

### Question 4: Would traits provide better compile-time validation?

**No, not meaningfully.**

**Why not**:
- Rune is dynamically typed - validation is always runtime
- Current `StructPluginLoader` already checks method existence at load time
- Trait implementation errors would still be runtime errors (missing protocol implementations)
- No static analysis of Rune scripts before loading

**Current validation approach** (see `struct_plugin.rs:459-513`):

```rust
// Try to call instance.tools()
let output = match vm.call(method_hash, (instance_clone,)) {
    Ok(output) => output,
    Err(e) => {
        // Method might not exist, return empty tools
        debug!("Plugin has no tools() method: {}", e);
        return Ok(vec![]);
    }
};
```

This is **equally robust** as trait-based validation would be.

### Question 5: How would dispatch work with trait objects?

**Cannot use trait objects in Rune currently.**

Current dispatch (see `struct_plugin.rs:515-563`):
1. Find plugin that provides the tool (registry index)
2. Clone the plugin instance
3. Calculate method hash for `dispatch()`
4. Call via Rune VM with `(instance, tool_name, args)`

**If traits were available**:
- Would still need registry index (traits don't provide discovery)
- Would still call via method hash (no virtual dispatch tables)
- No benefit over current approach

## Recommendation

**Do not use traits for plugin definitions.**

**Reasons**:
1. **No practical benefit** - Current convention-based approach works well
2. **Increased complexity** - Requires Rust-side trait machinery
3. **No compile-time safety** - Rune is dynamic; validation is always runtime
4. **Limited Rune trait support** - No trait objects, no user-defined traits
5. **Good enough validation** - Current runtime checks are sufficient

**Current approach strengths**:
- Simple and understandable
- Flexible (methods are optional)
- Easy to extend (just add more method conventions)
- Well-tested and working in production

**Alternative for better validation**:

If compile-time validation is desired, consider:
1. **Rune Language Server Protocol** integration for IDE support
2. **Schema validation** for plugin manifests (JSON/TOML)
3. **Documentation conventions** (already in place)
4. **Runtime validation with helpful error messages** (already implemented)

## Protocol System Notes

From [search results](https://github.com/rune-rs/rune):

> The DISPLAY_FMT protocol is a function that can be implemented by any external type which allows it to be used in a template string.

Protocols like `DISPLAY_FMT`, `NEXT`, `SIZE_HINT` are built into Rune's VM.

**Custom protocols for plugins would require**:
1. Defining new Protocol variants in Rune VM
2. Registering protocol handlers in module system
3. Teaching VM how to dispatch custom protocols
4. Complex machinery for no practical benefit

## Conclusion

Rune's trait system is designed for Rune's internal type system and Rust interop, not for user-defined plugin interfaces. The current convention-based plugin pattern is:

- **More appropriate** for Rune's dynamic nature
- **Simpler** to implement and understand
- **More flexible** for plugin authors
- **Equally robust** in terms of validation

**Recommendation**: Keep the current struct-based plugin pattern with convention-based methods.

## Sources

- [Rune Traits Documentation](https://rune-rs.github.io/book/traits.html)
- [Rune-rs GitHub Repository](https://github.com/rune-rs/rune)
- [The Rune Programming Language](https://rune-rs.github.io/)
- Crucible source code:
  - `/home/moot/crucible/crates/crucible-rune/src/struct_plugin.rs`
  - `/home/moot/crucible/crates/crucible-rune/src/plugin_types.rs`
  - `/home/moot/crucible/examples/plugins/just.rn`

## Related

- [[Help/Extending/Creating Plugins]]
- [[Help/Rune/Language Basics]]
- [[Help/Rune/Best Practices]]
