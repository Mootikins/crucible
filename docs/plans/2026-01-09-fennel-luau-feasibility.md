# Fennel → Luau Fork Feasibility Analysis

**Question:** Can we fork Fennel to emit Luau type annotations?

**Answer:** Yes, highly feasible. Luau is based on Lua 5.1.4, and Fennel targets Lua 5.1.

---

## Version Compatibility

| Language | Base Version | Notes |
|----------|--------------|-------|
| Lua (original) | 5.1 | Released 2006 |
| Luau | 5.1.4 fork | Roblox, 2019+ |
| Fennel | 5.1 target | No runtime, pure transpilation |

**Key insight:** Luau is backwards-compatible with Lua 5.1. Any valid Lua 5.1 output from Fennel is valid Luau. We only need to ADD type annotation syntax to the output.

---

## Luau Type Syntax to Support

### Variable Annotations
```luau
local x: number = 5
local name: string = "hello"
```

### Function Signatures
```luau
local function add(a: number, b: number): number
    return a + b
end

-- Multiple returns
local function getData(): (string, number)
    return "result", 42
end
```

### Type Aliases
```luau
type Point = {x: number, y: number}
type Callback = (string) -> number
export type Point = {x: number, y: number}
```

### Table Types
```luau
local config: {name: string, count: number} = {name = "test", count = 1}
local nums: {number} = {1, 2, 3}  -- array shorthand
```

### Type Casts
```luau
local value = 5 :: any
```

---

## Implementation Approaches

### Option 1: Macro-Based (No Fork)

Use Fennel's existing `lua` special form to emit raw Luau:

```fennel
(macro defn-typed [name params ret-type & body]
  (let [param-str (table.concat
                    (icollect [_ p (ipairs params)]
                      (.. p.name ": " p.type)) ", ")
        lua-sig (.. "local function " name "(" param-str "): " ret-type)]
    `(lua ,lua-sig)
    ; ... emit body
    ))

;; Usage
(defn-typed add [{:name "a" :type "number"}
                 {:name "b" :type "number"}]
            "number"
  (+ a b))
```

**Pros:** No fork needed, works today
**Cons:** Ugly syntax, no IDE support, fragile string concatenation

### Option 2: Compiler Plugin (No Fork)

Fennel supports compiler plugins. A plugin could intercept AST and emit type annotations.

```fennel
;; Plugin that reads type metadata and emits Luau
{:symbol-to-expression
 (fn [ast scope parent opts]
   ;; Check for type metadata, emit annotated output
   )}
```

**Pros:** Clean separation, upstream compatible
**Cons:** Limited plugin API, may not cover all cases

### Option 3: Minimal Fork (Recommended)

Fork Fennel to add:
1. Type annotation syntax in parser
2. Type-aware code generation
3. `--target luau` flag

**Proposed Fennel syntax:**

```fennel
;; Type annotations with ^ prefix (like Clojure metadata)
(fn add [^number a ^number b] ^number
  (+ a b))

;; Or colon suffix (like typed-fennel)
(fn add [a :number b :number] :-> number
  (+ a b))

;; Type aliases
(type Point {:x number :y number})

;; Local with type
(local ^string name "hello")
```

**Output (with --target luau):**
```luau
local function add(a: number, b: number): number
    return a + b
end

type Point = {x: number, y: number}

local name: string = "hello"
```

---

## Fork Scope Estimate

### Files to Modify

| File | Changes |
|------|---------|
| `src/fennel/parser.fnl` | Add type annotation parsing |
| `src/fennel/compiler.fnl` | Emit type annotations in output |
| `src/fennel/specials.fnl` | Add `type` special form |
| `src/fennel/macros.fnl` | Type-aware macro expansion |

### New Files

| File | Purpose |
|------|---------|
| `src/fennel/luau.fnl` | Luau-specific codegen |
| `src/fennel/types.fnl` | Type representation/validation |

### Estimated Effort

| Phase | Work |
|-------|------|
| Parser changes | ~200-400 lines |
| Codegen changes | ~300-500 lines |
| Type alias support | ~100-200 lines |
| Testing | ~500 lines |
| **Total** | ~1100-1600 lines |

---

## Risk Assessment

### Low Risk
- **Backwards compatibility**: Type annotations are purely additive
- **Lua output**: Default output remains valid Lua 5.1
- **Luau compatibility**: Luau accepts all Lua 5.1

### Medium Risk
- **Upstream divergence**: Fennel updates require merge work
- **Macro interaction**: Type annotations in macro output need handling

### Mitigation
- Keep fork minimal, isolate changes to new files where possible
- Contribute type metadata hooks upstream (they're open to plugins)
- Use feature flag (`--target luau`) to keep Lua output unchanged

---

## Alternative: Contribute Upstream

The Fennel maintainer said in [issue #467](https://github.com/bakpakin/Fennel/issues/467):

> "At this time I don't think it's a good idea to include support for specialized fork syntax in Fennel itself."

But also:

> Remained open to extending the plugin interface to enable this functionality externally.

**Strategy:**
1. Propose plugin API extensions for type metadata
2. If rejected, maintain minimal fork
3. Periodically offer patches back

---

## Recommendation

**Start with Option 3 (Minimal Fork)** because:

1. **Clean syntax** - Native type annotations in Fennel
2. **IDE support** - Types visible in source, not hidden in macros
3. **Luau ecosystem** - Full type checking via Luau tooling
4. **Controlled scope** - Only add what we need

**First milestone:** Function parameter/return types only
```fennel
(fn add [a :number b :number] :-> number
  (+ a b))
```

This covers 80% of the value with minimal changes.

---

## Prototype Results (2026-01-09)

We built a working macro-based prototype to validate the syntax design:

**Files created:**
- `crates/crucible-lua/lib/luau-types.fnl` - Runtime type compilation & code generation
- `crates/crucible-lua/lib/luau-types-macros.fnl` - Macro definitions
- `crates/crucible-lua/tests/luau_types_test.fnl` - 41 passing tests

**Syntax validated:**
```fennel
;; Typed function
(defn add [a :number b :number] :-> number
  (+ a b))

;; Typed let bindings
(tlet [x :number 10
       y :string "hello"]
  (print x y))

;; Type alias (no-op in Lua mode)
(deftype Point {:x number :y number})

;; Type cast (pass-through in Lua mode)
(cast value :any)
```

**Key learnings:**
1. Keywords (`:number`) become plain strings in macro context
2. Symbols (variable names) remain as tables with metatables
3. Need to preserve original symbols (`p.sym`) for code generation
4. Macro helpers must be duplicated (can't access runtime module)

**Luau output generated by runtime functions:**
```luau
local function add(a: number, b: number): number
    return a + b
end

local x: number = 10
local y: string = "hello"

type Point = {x: number, y: number}

value :: any
```

**Conclusion:** Macro-based approach works for prototyping, but has limitations:
- No actual Luau output (types erased in Lua mode)
- Fragile string concatenation for complex types
- Can't integrate with Luau type checker

**Next step:** If Luau output is needed, fork Fennel with `--target luau` flag.

---

## References

- [Luau Compatibility](https://luau.org/compatibility/)
- [Luau Syntax](https://luau.org/syntax/)
- [Fennel Compiler Source](https://github.com/bakpakin/Fennel/blob/main/src/fennel/compiler.fnl)
- [Fennel Issue #467](https://github.com/bakpakin/Fennel/issues/467)
- [typed-fennel](https://github.com/dokutan/typed-fennel)
