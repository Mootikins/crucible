;; luau-types-macros.fnl - Fennel macros for Luau type annotations
;;
;; These macros use the runtime functions from luau-types.fnl
;; to generate type-annotated Luau code.
;;
;; Usage:
;;   (import-macros {: defn : tlet : deftype : cast} :luau-types-macros)
;;
;;   (defn add [a :number b :number] :-> number
;;     (+ a b))
;;
;;   (tlet [x :number 10]
;;     (print x))
;;
;;   (deftype Point {:x number :y number})
;;
;;   (cast value :any)

;; ═══════════════════════════════════════════════════════════════════════════
;; Helper functions (duplicated for macro-time availability)
;; ═══════════════════════════════════════════════════════════════════════════

(fn is-type? [x]
  "Check if x is a type annotation.
   In macro context, keywords like :number become plain strings 'number'.
   Symbols (variable names) are tables with metatables."
  ;; A type annotation is a string (from keyword), not a table (symbol)
  (= (type x) :string))

(fn is-symbol? [x]
  "Check if x is a symbol (variable name)"
  (= (type x) :table))

(fn parse-typed-params [params]
  "Parse [a :number b :string] into [{:name a :type number :sym symbol} ...]
   In macro context:
   - Symbols (a, b) are tables
   - Type annotations (:number) are strings ('number')"
  (let [result []
        len (length params)]
    (var i 1)
    (while (<= i len)
      (let [current (. params i)
            next-item (. params (+ i 1))
            ;; Type annotation follows if next item is a string (not a symbol/table)
            has-type (and next-item (is-type? next-item))]
        (if has-type
            (do
              (table.insert result
                {:name (tostring current)
                 :type next-item  ; Already a string like "number"
                 :sym current     ; Keep original symbol for code gen
                 :optional (let [s (tostring current)] (s:match "^%?"))})
              (set i (+ i 2)))
            (do
              (table.insert result
                {:name (tostring current)
                 :type nil
                 :sym current
                 :optional (let [s (tostring current)] (s:match "^%?"))})
              (set i (+ i 1))))))
    result))

(fn parse-typed-bindings [bindings]
  "Parse [x :number 10 y :string 'hi'] into [{:name x :type number :value 10 :sym symbol} ...]
   In macro context:
   - Symbols (x, y) are tables
   - Type annotations (:number) are strings ('number')
   - Values are AST nodes"
  (let [result []
        len (length bindings)]
    (var i 1)
    (while (<= i len)
      (let [name (. bindings i)
            maybe-type (. bindings (+ i 1))
            ;; Type annotation follows if next item is a string (not symbol/value)
            has-type (and maybe-type (is-type? maybe-type))]
        (if has-type
            (let [value (. bindings (+ i 2))]
              (table.insert result
                {:name (tostring name)
                 :type maybe-type  ; Already a string like "number"
                 :value value
                 :sym name})       ; Keep original symbol
              (set i (+ i 3)))
            (let [value (. bindings (+ i 1))]
              (table.insert result
                {:name (tostring name)
                 :type nil
                 :value value
                 :sym name})
              (set i (+ i 2))))))
    result))

;; ═══════════════════════════════════════════════════════════════════════════
;; Macros
;; ═══════════════════════════════════════════════════════════════════════════

(fn defn [name params ...]
  "Define a typed function.

   Syntax: (defn name [param :type ...] :-> return-type body...)

   Example:
     (defn add [a :number b :number] :-> number
       (+ a b))

   In Lua mode: Compiles to regular function (types in comments)
   In Luau mode: Would emit actual type annotations"

  ;; Parse: find :-> marker and split params/return-type/body
  (var return-type nil)
  (var body-start 1)
  (let [args [...]]
    ;; Look for :-> in args
    (for [i 1 (length args)]
      (let [item (. args i)]
        (when (or (= (tostring item) ":->")
                  (= (tostring item) "->"))
          (set return-type (. args (+ i 1)))
          (set body-start (+ i 2)))))

    ;; Collect body after return type
    (local body [])
    (for [i body-start (length args)]
      (table.insert body (. args i)))

    ;; Parse params and build function
    (let [parsed (parse-typed-params params)
          ;; Use preserved symbols from parsing (p.sym), not reconstructed symbols
          param-names (icollect [_ p (ipairs parsed)] p.sym)]
      ;; Emit the function - in prototype, just use regular fn
      ;; A fork would emit actual Luau via (lua ...) special
      `(fn ,name ,param-names
         ;; Types: params={parsed}, return={return-type}
         ,(unpack body)))))

(fn tlet [bindings ...]
  "Typed let bindings.

   Syntax: (tlet [name :type value ...] body...)

   Example:
     (tlet [x :number 10
            y :string \"hello\"]
       (print x y))

   In Lua mode: Compiles to regular let
   In Luau mode: Would emit typed locals"

  (let [parsed (parse-typed-bindings bindings)
        ;; Build standard fennel let bindings (flatten pairs)
        ;; Use preserved symbols from parsing (b.sym), not reconstructed symbols
        flat-bindings (let [fb []]
                        (each [_ b (ipairs parsed)]
                          (table.insert fb b.sym)
                          (table.insert fb b.value))
                        fb)]
    `(let ,flat-bindings
       ;; Types: bindings={parsed}
       ,...)))

(fn deftype [name type-def]
  "Define a type alias.

   Syntax: (deftype Name type-expression)

   Examples:
     (deftype Point {:x number :y number})
     (deftype Callback (fn [string] :-> number))

   In Lua mode: No-op (types are erased)
   In Luau mode: Would emit 'type Name = ...'"

  ;; In Lua output: just nil (type aliases don't exist)
  ;; Comment preserves intent for documentation
  `(do
     ;; Type alias: ,name = ,type-def
     nil))

(fn cast [expr type-name]
  "Type cast expression.

   Syntax: (cast expr :type)

   Example:
     (cast some-value :any)

   In Lua mode: Pass through unchanged
   In Luau mode: Would emit 'expr :: type'"

  ;; In Lua: just return the expression unchanged
  expr)

;; ═══════════════════════════════════════════════════════════════════════════
;; Export macros
;; ═══════════════════════════════════════════════════════════════════════════

{: defn
 : tlet
 : deftype
 : cast}
