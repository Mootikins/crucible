;; luau-types.fnl - Fennel macros that emit Luau type annotations
;;
;; Prototype for Fennel->Luau type annotation support.
;; These macros compile to Luau-annotated Lua code.
;;
;; Usage:
;;   (local t (require :luau-types))
;;
;;   ;; Typed function
;;   (t.defn add [a :number b :number] :-> number
;;     (+ a b))
;;
;;   ;; Typed locals
;;   (t.tlet [x :number 10]
;;     (print x))
;;
;;   ;; Type alias
;;   (t.deftype Point {:x number :y number})
;;
;;   ;; Type cast
;;   (t.cast value :any)

;; ═══════════════════════════════════════════════════════════════════════════
;; Type Compilation - Convert Fennel type syntax to Luau strings
;; ═══════════════════════════════════════════════════════════════════════════

(fn list? [x]
  "Check if x is a Fennel list (not a table)"
  (and (= (type x) :table)
       (not= nil (getmetatable x))
       (. (getmetatable x) :__fennellist)))

(fn sym? [x]
  "Check if x is a Fennel symbol"
  (and (= (type x) :table)
       (not= nil (getmetatable x))
       (. (getmetatable x) :__fennelsym)))

(fn kw->str [kw]
  "Convert keyword to string, stripping leading colon"
  (if (= (type kw) :string)
      (kw:gsub "^:" "")
      (tostring kw)))

(fn compile-type-def [type-def]
  "Convert Fennel type syntax to Luau type string"
  (if
    ;; nil -> "nil"
    (= type-def nil)
    "nil"

    ;; Keyword: :number -> "number"
    (and (= (type type-def) :string) (type-def:match "^:"))
    (type-def:gsub "^:" "")

    ;; Symbol: number -> "number"
    (sym? type-def)
    (tostring type-def)

    ;; String literal (type name): "number" -> "number"
    (= (type type-def) :string)
    type-def

    ;; Table type: {:x number :y string} -> "{x: number, y: string}"
    (and (= (type type-def) :table) (not (list? type-def)))
    (let [fields []]
      (each [k v (pairs type-def)]
        (let [key-str (kw->str k)
              val-str (compile-type-def v)]
          (table.insert fields (.. key-str ": " val-str))))
      (.. "{" (table.concat fields ", ") "}"))

    ;; Function type: (fn [string number] :-> boolean)
    ;; -> "(string, number) -> boolean"
    (and (list? type-def)
         (= (tostring (. type-def 1)) "fn"))
    (let [params (. type-def 2)
          ;; Find :-> and get return type after it
          ret-idx (do
                    (var idx nil)
                    (for [i 3 (length type-def)]
                      (when (or (= (. type-def i) :->)
                                (= (tostring (. type-def i)) ":->"))
                        (set idx (+ i 1))))
                    idx)
          ret (when ret-idx (. type-def ret-idx))
          param-strs (icollect [_ p (ipairs params)]
                       (compile-type-def p))
          ret-str (if ret (compile-type-def ret) "()")]
      (.. "(" (table.concat param-strs ", ") ") -> " ret-str))

    ;; Array type: (array number) -> "{number}"
    (and (list? type-def)
         (= (tostring (. type-def 1)) "array"))
    (.. "{" (compile-type-def (. type-def 2)) "}")

    ;; Union type: (union string number nil) -> "string | number | nil"
    (and (list? type-def)
         (= (tostring (. type-def 1)) "union"))
    (let [types (icollect [i t (ipairs type-def)]
                  (when (> i 1) (compile-type-def t)))]
      (table.concat types " | "))

    ;; Optional/nullable type: (? string) -> "string?"
    (and (list? type-def)
         (= (tostring (. type-def 1)) "?"))
    (.. (compile-type-def (. type-def 2)) "?")

    ;; Tuple type: (tuple string number) -> "(string, number)"
    (and (list? type-def)
         (= (tostring (. type-def 1)) "tuple"))
    (let [types (icollect [i t (ipairs type-def)]
                  (when (> i 1) (compile-type-def t)))]
      (.. "(" (table.concat types ", ") ")"))

    ;; Generic type: (Map string number) -> "Map<string, number>"
    (and (list? type-def) (sym? (. type-def 1)))
    (let [name (tostring (. type-def 1))
          args (icollect [i t (ipairs type-def)]
                 (when (> i 1) (compile-type-def t)))]
      (if (> (length args) 0)
          (.. name "<" (table.concat args ", ") ">")
          name))

    ;; Fallback: stringify
    (tostring type-def)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Parameter Parsing - Parse [a :number b :string] format
;; ═══════════════════════════════════════════════════════════════════════════

(fn is-type? [x]
  "Check if x looks like a type annotation (keyword starting with :)"
  (and (= (type x) :string) (x:match "^:")))

(fn parse-typed-params [params]
  "Parse [a :number b :string] into [{:name a :type number} ...]"
  (let [result []
        len (length params)]
    (var i 1)
    (while (<= i len)
      (let [current (. params i)
            next-item (. params (+ i 1))
            ;; Check if next item is a type annotation
            has-type (and next-item (is-type? next-item))]
        (if has-type
            (do
              (table.insert result
                {:name (tostring current)
                 :type (kw->str next-item)
                 :optional (let [s (tostring current)] (s:match "^%?"))})
              (set i (+ i 2)))
            (do
              (table.insert result
                {:name (tostring current)
                 :type nil
                 :optional (let [s (tostring current)] (s:match "^%?"))})
              (set i (+ i 1))))))
    result))

(fn build-param-string [parsed-params]
  "Build Luau parameter string: 'a: number, b: string'"
  (let [parts (icollect [_ p (ipairs parsed-params)]
                (if p.type
                    (.. p.name ": " p.type)
                    p.name))]
    (table.concat parts ", ")))

(fn build-param-names [parsed-params]
  "Extract just parameter names for Fennel function def (returns strings)"
  (icollect [_ p (ipairs parsed-params)]
    p.name))

;; ═══════════════════════════════════════════════════════════════════════════
;; Binding Parsing - Parse [x :number 10 y :string "hi"] format
;; ═══════════════════════════════════════════════════════════════════════════

(fn parse-typed-bindings [bindings]
  "Parse [x :number 10 y :string 'hi'] into [{:name x :type number :value 10} ...]"
  (let [result []
        len (length bindings)]
    (var i 1)
    (while (<= i len)
      (let [name (. bindings i)
            maybe-type (. bindings (+ i 1))
            has-type (and maybe-type (is-type? maybe-type))]
        (if has-type
            (let [value (. bindings (+ i 2))]
              (table.insert result
                {:name (tostring name)
                 :type (kw->str maybe-type)
                 :value value})
              (set i (+ i 3)))
            (let [value (. bindings (+ i 1))]
              (table.insert result
                {:name (tostring name)
                 :type nil
                 :value value})
              (set i (+ i 2))))))
    result))

;; ═══════════════════════════════════════════════════════════════════════════
;; Code Generation Helpers
;; ═══════════════════════════════════════════════════════════════════════════

(fn fennel-to-lua [expr]
  "Convert Fennel expression to Lua string (simplified)"
  ;; This is a simplified version - in real impl would use fennel.compile
  (if (= (type expr) :string)
      (.. "\"" expr "\"")
      (= (type expr) :number)
      (tostring expr)
      (= (type expr) :boolean)
      (tostring expr)
      (= (type expr) :nil)
      "nil"
      (= (type expr) :table)
      (if (sym? expr)
          (tostring expr)
          ;; Table literal - simple version
          (do
            (var parts [])
            (each [k v (pairs expr)]
              (let [key-str (if (= (type k) :string)
                               (.. "[\"" k "\"]")
                               (.. "[" (tostring k) "]"))
                    val-str (fennel-to-lua v)]
                (table.insert parts (.. key-str " = " val-str))))
            (.. "{" (table.concat parts ", ") "}")))
      ;; Fallback
      (tostring expr)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Code Generation - Generate Luau code strings
;; ═══════════════════════════════════════════════════════════════════════════

(fn make-defn-lua [name params return-type body-str]
  "Generate Luau function definition string"
  (let [parsed (parse-typed-params params)
        param-str (build-param-string parsed)
        ret-str (if return-type
                    (.. ": " (compile-type-def return-type))
                    "")]
    (.. "local function " name "(" param-str ")" ret-str "\n"
        "    " body-str "\n"
        "end")))

(fn make-let-lua [bindings body-str]
  "Generate Luau let binding string"
  (let [parsed (parse-typed-bindings bindings)
        decls (icollect [_ b (ipairs parsed)]
                (let [type-str (if b.type (.. ": " b.type) "")
                      val-str (fennel-to-lua b.value)]
                  (.. "local " b.name type-str " = " val-str)))]
    (.. (table.concat decls "\n") "\n" body-str)))

(fn make-deftype-lua [name type-def]
  "Generate Luau type alias string"
  (let [type-str (compile-type-def type-def)]
    (.. "type " name " = " type-str)))

(fn make-cast-lua [expr type-name]
  "Generate Luau type cast string"
  (let [expr-str (fennel-to-lua expr)
        type-str (compile-type-def type-name)]
    (.. expr-str " :: " type-str)))

;; ═══════════════════════════════════════════════════════════════════════════
;; NOTE: Macros are in separate file: luau-types-macros.fnl
;; Use: (import-macros {: defn : tlet : deftype : cast} :luau-types-macros)
;; ═══════════════════════════════════════════════════════════════════════════

;; ═══════════════════════════════════════════════════════════════════════════
;; Testing Helpers
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-compile-type [type-def]
  "Test helper: compile type and return string"
  (compile-type-def type-def))

(fn test-parse-params [params]
  "Test helper: parse params and return result"
  (parse-typed-params params))

(fn test-parse-bindings [bindings]
  "Test helper: parse bindings and return result"
  (parse-typed-bindings bindings))

(fn test-make-function [name params return-type body-str]
  "Test helper: generate Luau function definition"
  (make-defn-lua name params return-type body-str))

;; ═══════════════════════════════════════════════════════════════════════════
;; Export
;; ═══════════════════════════════════════════════════════════════════════════

{;; Type compilation
 : compile-type-def
 : kw->str

 ;; Parsing
 : parse-typed-params
 : parse-typed-bindings
 : build-param-string

 ;; Code generation
 : make-defn-lua
 : make-let-lua
 : make-deftype-lua
 : make-cast-lua
 : fennel-to-lua

 ;; Helpers
 : list?
 : sym?
 : is-type?

 ;; Testing
 : test-compile-type
 : test-parse-params
 : test-parse-bindings
 : test-make-function}
