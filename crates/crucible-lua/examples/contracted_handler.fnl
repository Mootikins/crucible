;; Example: Event handler with Steel-style contracts
;;
;; This shows how to use contracts.fnl for validated event pipeline handlers.

(local c (require :contracts))

;; ═══════════════════════════════════════════════════════════════════════════
;; Basic usage: wrap-with-contract
;; ═══════════════════════════════════════════════════════════════════════════

;; Define a simple search handler with contracts
(local search-handler
  (c.wrap-with-contract
    (fn [event]
      ;; Actual implementation
      (let [query event.query
            limit (or event.limit 10)]
        {:results [{:title "Note 1" :score 0.95}
                   {:title "Note 2" :score 0.87}]
         :total 2
         :query query}))
    {:name "search-handler"
     :pre [(c.and-c c.table? (c.field-c :query c.string?))]
     :post [c.table?]}))

;; ═══════════════════════════════════════════════════════════════════════════
;; Using the defcontract macro
;; ═══════════════════════════════════════════════════════════════════════════

;; Note: The macro form is cleaner but requires Fennel macro support
;; For now, use wrap-with-contract directly

;; (defcontract enrich-context [event]
;;   :pre [(c.table? event) (c.has-field? :content)]
;;   :post [c.table?]
;;   :preserves [:timestamp :tool_name]
;;   (do
;;     (tset event :enriched true)
;;     (tset event :enriched_at (os.time))
;;     event))

;; Manual equivalent:
(local enrich-context
  (c.wrap-with-contract
    (fn [event]
      (tset event :enriched true)
      (tset event :enriched_at (os.time))
      event)
    {:name "enrich-context"
     :pre [(c.and-c c.table? (c.has-field? :content))]
     :post [c.table?]
     :preserves [:timestamp :tool_name]}))

;; ═══════════════════════════════════════════════════════════════════════════
;; Pipeline composition with contracts
;; ═══════════════════════════════════════════════════════════════════════════

(fn pipeline [handlers]
  "Compose multiple contracted handlers into a pipeline"
  (fn [event]
    (var current event)
    (each [_ h (ipairs handlers)]
      (set current (h current)))
    current))

;; Create a contracted pipeline
(local my-pipeline
  (pipeline
    [;; Step 1: Validate and normalize
     (c.wrap-with-contract
       (fn [e]
         (tset e :normalized true)
         e)
       {:name "normalize"
        :pre [(c.table?)]
        :post [c.table?]})

     ;; Step 2: Enrich
     enrich-context

     ;; Step 3: Format output
     (c.wrap-with-contract
       (fn [e]
         {:status :ok
          :data e})
       {:name "format-output"
        :pre [(c.table?)]
        :post [(c.field-c :status c.string?)]})]))

;; ═══════════════════════════════════════════════════════════════════════════
;; Main handler for Crucible
;; ═══════════════════════════════════════════════════════════════════════════

(fn handler [args]
  ;; Route to appropriate handler based on event type
  (let [event-type (or args.type :unknown)]
    (if (= event-type :search)
        (search-handler args)
        (= event-type :tool_result)
        (my-pipeline args)
        ;; Default: pass through
        args)))

;; Export for Crucible
{: handler
 : search-handler
 : enrich-context
 : my-pipeline}
