;; luau_types_test.fnl - Tests for Luau type annotation library
;;
;; Tests both:
;; 1. Runtime functions (luau-types.fnl) - type compilation, parsing, code generation
;; 2. Macros (luau-types-macros.fnl) - defn, tlet, deftype, cast
;;
;; Run with Crucible's Lua runtime or standalone:
;;   lua -e "require('fennel').install(); dofile('tests/luau_types_test.fnl')"

(local t (require :luau-types))
(import-macros {: defn : tlet : deftype : cast} :luau-types-macros)

;; ═══════════════════════════════════════════════════════════════════════════
;; Test Helpers
;; ═══════════════════════════════════════════════════════════════════════════

(var tests-run 0)
(var tests-passed 0)
(var tests-failed 0)

(fn test [name expected actual]
  "Run a single test comparing expected to actual"
  (set tests-run (+ tests-run 1))
  (if (= expected actual)
      (do
        (set tests-passed (+ tests-passed 1))
        (print (.. "  PASS: " name)))
      (do
        (set tests-failed (+ tests-failed 1))
        (print (.. "  FAIL: " name))
        (print (.. "    expected: " (tostring expected)))
        (print (.. "    actual:   " (tostring actual))))))

(fn test-match [name pattern actual]
  "Test that actual matches pattern"
  (set tests-run (+ tests-run 1))
  (if (actual:match pattern)
      (do
        (set tests-passed (+ tests-passed 1))
        (print (.. "  PASS: " name)))
      (do
        (set tests-failed (+ tests-failed 1))
        (print (.. "  FAIL: " name))
        (print (.. "    pattern: " pattern))
        (print (.. "    actual:  " actual)))))

(fn section [name]
  (print "")
  (print (.. "=== " name " ===")))

;; ═══════════════════════════════════════════════════════════════════════════
;; Type Compilation Tests (Runtime)
;; ═══════════════════════════════════════════════════════════════════════════

(section "Type Compilation - Primitives")

(test "keyword :number"
      "number"
      (t.compile-type-def ":number"))

(test "keyword :string"
      "string"
      (t.compile-type-def ":string"))

(test "keyword :boolean"
      "boolean"
      (t.compile-type-def ":boolean"))

(test "string literal"
      "number"
      (t.compile-type-def "number"))

(test "nil value"
      "nil"
      (t.compile-type-def nil))

;; ═══════════════════════════════════════════════════════════════════════════
;; Table Type Tests
;; ═══════════════════════════════════════════════════════════════════════════

(section "Type Compilation - Tables")

(test-match "simple table has x field"
            "x: number"
            (t.compile-type-def {:x "number"}))

(test-match "simple table has braces"
            "^{"
            (t.compile-type-def {:x "number"}))

(test-match "multi-field table has y"
            "y: string"
            (t.compile-type-def {:x "number" :y "string"}))

;; ═══════════════════════════════════════════════════════════════════════════
;; Parameter Parsing Tests
;; ═══════════════════════════════════════════════════════════════════════════

(section "Parameter Parsing")

(test "single typed param count"
      1
      (length (t.parse-typed-params ["a" ":number"])))

(test "single typed param name"
      "a"
      (. (. (t.parse-typed-params ["a" ":number"]) 1) :name))

(test "single typed param type"
      "number"
      (. (. (t.parse-typed-params ["a" ":number"]) 1) :type))

(test "multiple typed params count"
      2
      (length (t.parse-typed-params ["a" ":number" "b" ":string"])))

(test "untyped param has nil type"
      nil
      (. (. (t.parse-typed-params ["x"]) 1) :type))

(test "mixed params count"
      3
      (length (t.parse-typed-params ["a" ":number" "b" "c" ":string"])))

;; ═══════════════════════════════════════════════════════════════════════════
;; Parameter String Building Tests
;; ═══════════════════════════════════════════════════════════════════════════

(section "Parameter String Building")

(test "single typed param string"
      "a: number"
      (t.build-param-string (t.parse-typed-params ["a" ":number"])))

(test "multiple typed params string"
      "a: number, b: string"
      (t.build-param-string (t.parse-typed-params ["a" ":number" "b" ":string"])))

(test "untyped param string"
      "x"
      (t.build-param-string (t.parse-typed-params ["x"])))

;; ═══════════════════════════════════════════════════════════════════════════
;; Binding Parsing Tests
;; ═══════════════════════════════════════════════════════════════════════════

(section "Binding Parsing")

(test "single typed binding count"
      1
      (length (t.parse-typed-bindings ["x" ":number" 10])))

(test "single typed binding name"
      "x"
      (. (. (t.parse-typed-bindings ["x" ":number" 10]) 1) :name))

(test "single typed binding type"
      "number"
      (. (. (t.parse-typed-bindings ["x" ":number" 10]) 1) :type))

(test "single typed binding value"
      10
      (. (. (t.parse-typed-bindings ["x" ":number" 10]) 1) :value))

(test "multiple bindings count"
      2
      (length (t.parse-typed-bindings ["x" ":number" 10 "y" ":string" "hello"])))

;; ═══════════════════════════════════════════════════════════════════════════
;; Luau Code Generation Tests
;; ═══════════════════════════════════════════════════════════════════════════

(section "Luau Code Generation")

(test-match "function has signature"
            "local function add"
            (t.make-defn-lua "add" ["a" ":number" "b" ":number"] "number" "return a + b"))

(test-match "function has typed params"
            "a: number, b: number"
            (t.make-defn-lua "add" ["a" ":number" "b" ":number"] "number" "return a + b"))

(test-match "function has return type"
            ": number"
            (t.make-defn-lua "add" ["a" ":number" "b" ":number"] "number" "return a + b"))

(test-match "let has typed local"
            "local x: number = 10"
            (t.make-let-lua ["x" ":number" 10] "print(x)"))

(test-match "type alias format"
            "^type Point = {"
            (t.make-deftype-lua "Point" {:x "number" :y "number"}))

(test-match "type alias has fields"
            "x: number"
            (t.make-deftype-lua "Point" {:x "number" :y "number"}))

(test "type cast format"
      "\"value\" :: any"
      (t.make-cast-lua "value" ":any"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Macro Tests (Compile-time)
;; ═══════════════════════════════════════════════════════════════════════════

(section "Macro Tests")

;; Test defn creates working function
(defn multiply [a :number b :number] :-> number
  (* a b))

(test "defn creates working function"
      6
      (multiply 2 3))

;; Test defn with no return type
(defn greet [name :string]
  (.. "Hello, " name))

(test "defn without return type"
      "Hello, World"
      (greet "World"))

;; Test tlet creates working bindings
(test "tlet creates working bindings"
      15
      (tlet [x :number 10
             y :number 5]
        (+ x y)))

;; Test tlet with mixed types
(test "tlet with mixed types"
      "Count: 42"
      (tlet [msg :string "Count: "
             num :number 42]
        (.. msg (tostring num))))

;; Test deftype (should be no-op in Lua mode)
;; In Luau mode this would create a type alias
(test "deftype returns nil"
      nil
      (deftype MyPoint {:x number :y number}))

;; Test cast (should pass through)
(test "cast passes through value"
      42
      (cast 42 :any))

(test "cast passes through string"
      "hello"
      (cast "hello" :string))

;; ═══════════════════════════════════════════════════════════════════════════
;; Complex Scenarios
;; ═══════════════════════════════════════════════════════════════════════════

(section "Complex Scenarios")

;; Function with optional param (? prefix)
(defn maybe-add [a :number ?b :number] :-> number
  (+ a (or ?b 0)))

(test "optional param with value"
      15
      (maybe-add 10 5))

(test "optional param without value"
      10
      (maybe-add 10))

;; Nested tlet
(test "nested tlet"
      30
      (tlet [x :number 10]
        (tlet [y :number 20]
          (+ x y))))

;; Function returning table
(defn make-point [x :number y :number] :-> table
  {:x x :y y})

(test "function returning table x"
      5
      (. (make-point 5 10) :x))

(test "function returning table y"
      10
      (. (make-point 5 10) :y))

;; ═══════════════════════════════════════════════════════════════════════════
;; Summary
;; ═══════════════════════════════════════════════════════════════════════════

(print "")
(print "═══════════════════════════════════════")
(print (.. "Tests run:    " tests-run))
(print (.. "Tests passed: " tests-passed))
(print (.. "Tests failed: " tests-failed))
(print "═══════════════════════════════════════")

(when (> tests-failed 0)
  (os.exit 1))
