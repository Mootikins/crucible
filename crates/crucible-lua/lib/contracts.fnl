;; contracts.fnl - Steel-inspired contracts for Fennel/Crucible
;;
;; Provides runtime contracts with blame tracking.
;; Use in tool definitions for validated event pipeline handlers.
;;
;; Usage:
;;   (local c (require :contracts))
;;
;;   ;; Basic predicates
;;   (c.string? x)      ;; is x a string?
;;   (c.positive? x)    ;; is x > 0?
;;   (c.non-empty? x)   ;; is x non-empty string/table?
;;
;;   ;; Combinators (Steel-style)
;;   (c.and-c c.string? c.non-empty?)     ;; both must pass
;;   (c.or-c c.string? c.number?)         ;; either can pass
;;   (c.not-c c.nil?)                     ;; negation
;;
;;   ;; Function contracts
;;   (c.check-contract {:pre [c.string?] :post c.table?} f)
;;
;;   ;; Macro form
;;   (defcontract my-handler [x]
;;     :pre [(c.string? x) (c.non-empty? x)]
;;     :post [c.table?]
;;     :preserves [:timestamp]
;;     (do-stuff x))

;; ═══════════════════════════════════════════════════════════════════════════
;; Predicates (Steel-style naming: predicate?)
;; ═══════════════════════════════════════════════════════════════════════════

(fn nil? [x] (= x nil))
(fn string? [x] (= (type x) :string))
(fn number? [x] (= (type x) :number))
(fn table? [x] (= (type x) :table))
(fn boolean? [x] (= (type x) :boolean))
(fn function? [x] (= (type x) :function))

(fn positive? [x]
  (and (number? x) (> x 0)))

(fn negative? [x]
  (and (number? x) (< x 0)))

(fn zero? [x]
  (and (number? x) (= x 0)))

(fn integer? [x]
  (and (number? x) (= x (math.floor x))))

(fn non-empty? [x]
  (if (string? x) (> (length x) 0)
      (table? x) (> (length x) 0)
      false))

(fn empty? [x]
  (not (non-empty? x)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Contract Combinators (Steel-style: and/c, or/c, not/c)
;; ═══════════════════════════════════════════════════════════════════════════

(fn and-c [...]
  "All predicates must pass"
  (let [preds [...]]
    (fn [x]
      (var result true)
      (each [_ p (ipairs preds)]
        (when (not (p x))
          (set result false)))
      result)))

(fn or-c [...]
  "Any predicate must pass"
  (let [preds [...]]
    (fn [x]
      (var result false)
      (each [_ p (ipairs preds)]
        (when (p x)
          (set result true)))
      result)))

(fn not-c [pred]
  "Negation of predicate"
  (fn [x] (not (pred x))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Field Contracts (like Steel's struct/c)
;; ═══════════════════════════════════════════════════════════════════════════

(fn field-c [path pred]
  "Check predicate on nested field"
  (fn [x]
    (if (not (table? x))
        false
        (let [val (. x path)]
          (pred val)))))

(fn has-field? [path]
  "Check that field exists"
  (fn [x]
    (and (table? x) (not= nil (. x path)))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Blame Tracking
;; ═══════════════════════════════════════════════════════════════════════════

(fn violation [expected received blame-target path]
  "Create a contract violation error"
  {:type :contract-violation
   :expected expected
   :received (if (= (type received) :table)
                 (crucible.json_encode received)
                 (tostring received))
   :blame blame-target
   :path (or path [])})

(fn format-violation [v]
  "Format violation for error message"
  (string.format "Contract violation: expected %s, got %s (blame: %s%s)"
                 v.expected
                 v.received
                 v.blame
                 (if (> (length v.path) 0)
                     (.. ", at: " (table.concat v.path "."))
                     "")))

;; ═══════════════════════════════════════════════════════════════════════════
;; Function Contracts (Steel's ->/c)
;; ═══════════════════════════════════════════════════════════════════════════

(fn check-pre [preds args blame-name]
  "Check pre-conditions on arguments"
  (each [i pred (ipairs preds)]
    (let [arg (. args i)]
      (when (not (pred arg))
        (error (format-violation
                 (violation (.. "predicate #" i) arg
                           (.. "caller of " blame-name)
                           [(.. "arg[" i "]")]))))))
  true)

(fn check-post [pred result blame-name]
  "Check post-condition on result"
  (when (not (pred result))
    (error (format-violation
             (violation "post-condition" result blame-name []))))
  true)

(fn check-preserves [keys before after blame-name]
  "Check that specified keys are preserved"
  (each [_ key (ipairs keys)]
    (let [before-val (. before key)
          after-val (. after key)]
      (when (not= before-val after-val)
        (error (format-violation
                 (violation (.. "preserved '" key "'")
                           (.. (tostring before-val) " -> " (tostring after-val))
                           blame-name
                           [key]))))))
  true)

(fn wrap-with-contract [f contract-spec]
  "Wrap function with contract checking"
  (let [{: pre : post : preserves : name} contract-spec
        blame-name (or name "anonymous")]
    (fn [...]
      (let [args [...]]
        ;; Check pre-conditions
        (when pre
          (check-pre pre args blame-name))

        ;; Snapshot for preserves check
        (local before (when (and preserves (> (length args) 0))
                        (let [t {}]
                          (each [_ k (ipairs preserves)]
                            (tset t k (. (. args 1) k)))
                          t)))

        ;; Call actual function
        (let [result (f ...)]

          ;; Check post-condition
          (when post
            (check-post post result blame-name))

          ;; Check preserves
          (when (and preserves before (table? result))
            (check-preserves preserves before result blame-name))

          result)))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Dependent Contracts (Steel's ->i)
;; ═══════════════════════════════════════════════════════════════════════════

(fn dependent-post [post-fn]
  "Create dependent post-condition based on inputs"
  (fn [args result]
    (let [pred (post-fn (unpack args))]
      (pred result))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Macro: defcontract
;; ═══════════════════════════════════════════════════════════════════════════

;; This creates a contracted function definition
;;
;; Example:
;;   (defcontract search [query limit]
;;     :pre [(string? query) (or (nil? limit) (positive? limit))]
;;     :post [table?]
;;     :preserves [:timestamp]
;;     (do-actual-search query limit))

(macro defcontract [name args ...]
  "Define a function with contracts"
  (var pre nil)
  (var post nil)
  (var preserves nil)
  (var body [])

  ;; Parse keyword arguments
  (var i 1)
  (while (<= i (select :# ...))
    (let [item (select i ...)]
      (if (= item :pre)
          (do (set pre (select (+ i 1) ...))
              (set i (+ i 2)))
          (= item :post)
          (do (set post (select (+ i 1) ...))
              (set i (+ i 2)))
          (= item :preserves)
          (do (set preserves (select (+ i 1) ...))
              (set i (+ i 2)))
          (do (table.insert body item)
              (set i (+ i 1))))))

  ;; Generate wrapped function
  `(local ,name
     (wrap-with-contract
       (fn ,args ,(unpack body))
       {:name ,(tostring name)
        :pre ,pre
        :post ,post
        :preserves ,preserves})))

;; ═══════════════════════════════════════════════════════════════════════════
;; Event Pipeline Contracts
;; ═══════════════════════════════════════════════════════════════════════════

(fn event-handler-c [event-type pattern contract]
  "Create a contracted event handler spec"
  {:event event-type
   :pattern pattern
   :contract contract
   :handler (fn [handler-fn]
              (wrap-with-contract handler-fn contract))})

;; Common event contracts
(local tool-result-event-c
  {:pre [(and-c table? (field-c :tool_name string?))]
   :post [table?]})

(local message-event-c
  {:pre [(and-c table?
               (field-c :content string?)
               (field-c :participant_id string?))]
   :post [table?]})

;; ═══════════════════════════════════════════════════════════════════════════
;; Tool Schema Registry (for RMCP tool generation)
;; ═══════════════════════════════════════════════════════════════════════════

;; Global registry for tool schemas
(global __tool_schemas__ (or __tool_schemas__ {}))

(fn type-to-json-schema [type-name]
  "Convert Fennel type name to JSON Schema type"
  (match type-name
    :string {:type :string}
    :number {:type :number}
    :integer {:type :integer}
    :boolean {:type :boolean}
    :table {:type :object}
    :array {:type :array}
    :any {}
    _ {:type :string}))

(fn param-to-json-schema [param]
  "Convert param spec to JSON Schema property"
  (let [base (type-to-json-schema param.type)]
    (when param.description
      (tset base :description param.description))
    (when param.default
      (tset base :default param.default))
    base))

(fn schema-to-json-schema [schema]
  "Convert tool schema to JSON Schema format for RMCP"
  (let [properties {}
        required []]
    (each [_ param (ipairs (or schema.params []))]
      (tset properties param.name (param-to-json-schema param))
      (when (and (not= param.required false) (not param.default))
        (table.insert required param.name)))
    {:type :object
     :properties properties
     :required required}))

(fn register-tool-schema [name schema]
  "Register a tool schema in the global registry"
  (tset __tool_schemas__ name schema))

(fn get-tool-schema [name]
  "Get a specific tool schema"
  (. __tool_schemas__ name))

(fn get-all-tool-schemas []
  "Get all registered tool schemas"
  __tool_schemas__)

(fn schemas-to-rmcp-tools []
  "Convert all schemas to RMCP tool definitions"
  (let [tools []]
    (each [name schema (pairs __tool_schemas__)]
      (table.insert tools
        {:name name
         :description (or schema.description "")
         :inputSchema (schema-to-json-schema schema)}))
    tools))

;; ═══════════════════════════════════════════════════════════════════════════
;; Macro: deftool - Define a tool with contracts AND schema
;; ═══════════════════════════════════════════════════════════════════════════

;; Usage:
;;   (deftool search
;;     {:description "Search the knowledge base"
;;      :params [{:name "query" :type "string" :required true :description "Search query"}
;;               {:name "limit" :type "number" :required false :default 10}]
;;      :returns "array"}
;;     [args]
;;     (do-search args.query (or args.limit 10)))

(fn validate-tool-params [args params tool-name]
  "Validate arguments against param schema"
  (each [_ param (ipairs params)]
    (let [val (. args param.name)
          required (if (= param.required nil) true param.required)]
      ;; Check required
      (when (and required (= val nil) (not param.default))
        (error (.. "Contract violation in " tool-name ": missing required param '" param.name "'")))
      ;; Type check (if value provided)
      (when (and (not= val nil) param.type)
        (let [actual-type (type val)
              expected (match param.type
                         :string :string
                         :number :number
                         :integer :number
                         :boolean :boolean
                         :table :table
                         :array :table
                         :object :table
                         _ nil)]
          (when (and expected (not= actual-type expected))
            (error (.. "Contract violation in " tool-name ": param '" param.name
                       "' must be " param.type ", got " actual-type))))))))

(fn apply-defaults [args params]
  "Apply default values to args"
  (let [result (or args {})]
    (each [_ param (ipairs params)]
      (when (and (= (. result param.name) nil) param.default)
        (tset result param.name param.default)))
    result))

(fn make-tool [name schema impl]
  "Create a tool with contract validation and schema registration"
  ;; Register schema
  (register-tool-schema name schema)
  ;; Return wrapped handler
  (fn [args]
    ;; Apply defaults
    (let [with-defaults (apply-defaults args (or schema.params []))]
      ;; Validate params
      (validate-tool-params with-defaults (or schema.params []) name)
      ;; Execute implementation
      (let [result (impl with-defaults)]
        ;; Validate return type if specified
        (when schema.returns
          (let [ret-type (type result)
                expected (match schema.returns
                           :string :string
                           :number :number
                           :boolean :boolean
                           :table :table
                           :array :table
                           :object :table
                           _ nil)]
            (when (and expected (not= ret-type expected))
              (error (.. "Contract violation in " name ": return must be " schema.returns ", got " ret-type)))))
        result))))

(macro deftool [name schema params ...]
  "Define a tool with schema and contracts"
  `(global ,name (make-tool ,(tostring name) ,schema (fn ,params ,...))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Export
;; ═══════════════════════════════════════════════════════════════════════════

{;; Predicates
 : nil?
 : string?
 : number?
 : table?
 : boolean?
 : function?
 : positive?
 : negative?
 : zero?
 : integer?
 : non-empty?
 : empty?

 ;; Combinators
 : and-c
 : or-c
 : not-c
 : field-c
 : has-field?

 ;; Contract checking
 : violation
 : format-violation
 : check-pre
 : check-post
 : check-preserves
 : wrap-with-contract
 : dependent-post

 ;; Event pipeline
 : event-handler-c
 : tool-result-event-c
 : message-event-c

 ;; Tool schema (RMCP integration)
 : register-tool-schema
 : get-tool-schema
 : get-all-tool-schemas
 : schemas-to-rmcp-tools
 : schema-to-json-schema
 : make-tool
 : validate-tool-params
 : apply-defaults}
