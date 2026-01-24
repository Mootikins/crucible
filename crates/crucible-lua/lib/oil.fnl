;; oil.fnl - Fennel macros for Oil UI DSL
;;
;; Provides idiomatic Fennel syntax for building TUI components.
;; Macros compile to cru.oil.* function calls.
;;
;; Usage:
;;   (local oil (require :oil))
;;
;;   ;; Basic components
;;   (oil.col {:gap 1}
;;     (oil.text "Hello" {:bold true})
;;     (oil.row
;;       (oil.badge "OK" {:fg :green})
;;       (oil.spacer)))
;;
;;   ;; Control flow (use oil.when for conditional, oil.map-each for iteration)
;;   (oil.when loading (oil.spinner "Loading..."))
;;   (oil.if-else connected (oil.text "Online") (oil.text "Offline"))
;;   (oil.map-each items (fn [item] (oil.text item.name)))
;;
;;   ;; Reusable components
;;   (oil.defui status-bar [{: title : status}]
;;     (oil.row {:justify :space-between}
;;       (oil.text title {:bold true})
;;       (oil.badge status)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Layout Components
;; ═══════════════════════════════════════════════════════════════════════════

(fn col [...]
  "Vertical flex container"
  (cru.oil.col ...))

(fn row [...]
  "Horizontal flex container"
  (cru.oil.row ...))

(fn fragment [...]
  "Invisible wrapper for grouping nodes"
  (cru.oil.fragment ...))

(fn spacer []
  "Flexible space filler"
  (cru.oil.spacer))

;; ═══════════════════════════════════════════════════════════════════════════
;; Text Components
;; ═══════════════════════════════════════════════════════════════════════════

(fn text [content ?style]
  "Text content with optional styling"
  (cru.oil.text content ?style))

(fn badge [label ?style]
  "Styled label badge"
  (cru.oil.badge label ?style))

(fn divider [?char ?width]
  "Horizontal divider line"
  (cru.oil.divider ?char ?width))

(fn hr []
  "Horizontal rule"
  (cru.oil.hr))

;; ═══════════════════════════════════════════════════════════════════════════
;; Interactive Components
;; ═══════════════════════════════════════════════════════════════════════════

(fn spinner [?label]
  "Loading spinner with optional label"
  (cru.oil.spinner ?label))

(fn progress [value ?width]
  "Progress bar (value 0-1)"
  (cru.oil.progress value ?width))

(fn input [opts]
  "Text input field"
  (cru.oil.input opts))

(fn popup [items ?selected ?max-visible]
  "Popup menu"
  (cru.oil.popup items ?selected ?max-visible))

;; ═══════════════════════════════════════════════════════════════════════════
;; List Components
;; ═══════════════════════════════════════════════════════════════════════════

(fn bullet-list [items]
  "Bulleted list"
  (cru.oil.bullet_list items))

(fn numbered-list [items]
  "Numbered list"
  (cru.oil.numbered_list items))

(fn kv [key value]
  "Key-value pair row"
  (cru.oil.kv key value))

;; ═══════════════════════════════════════════════════════════════════════════
;; Control Flow
;; ═══════════════════════════════════════════════════════════════════════════

(fn when* [condition node]
  "Conditional rendering - shows node if condition is truthy"
  (cru.oil.when condition node))

(fn if-else [condition true-node false-node]
  "Conditional rendering with else branch"
  (cru.oil.if_else condition true-node false-node))

(fn map-each [items render-fn]
  "Iterate items and render each"
  (cru.oil.each items render-fn))

;; ═══════════════════════════════════════════════════════════════════════════
;; Component Factory
;; ═══════════════════════════════════════════════════════════════════════════

(fn component [base-fn default-props]
  "Create reusable component with default props"
  (cru.oil.component base-fn default-props))

;; ═══════════════════════════════════════════════════════════════════════════
;; Advanced
;; ═══════════════════════════════════════════════════════════════════════════

(fn scrollback [key ...]
  "Scrollable container with key for state"
  (cru.oil.scrollback key ...))

(fn markup [markup-string]
  "Parse XML-like markup to nodes"
  (cru.oil.markup markup-string))

;; ═══════════════════════════════════════════════════════════════════════════
;; Macros
;; ═══════════════════════════════════════════════════════════════════════════

(macro defui [name params ...]
  "Define a reusable UI component function.
   
   (defui status-bar [{: title : status}]
     (row {:justify :space-between}
       (text title {:bold true})
       (badge status)))
   
   ;; Usage:
   (status-bar {:title \"Dashboard\" :status \"OK\"})"
  `(fn ,name ,params ,...))

(macro cond-ui [...]
  "Multi-branch conditional rendering.
   
   (cond-ui
     loading (spinner \"Loading...\")
     error (text error {:fg :red})
     :else (text \"Ready\"))"
  (let [clauses [...]]
    (fn build-cond [i]
      (if (> i (length clauses))
          `(cru.oil.fragment [])
          (let [cond-expr (. clauses i)
                result (. clauses (+ i 1))]
            (if (= cond-expr :else)
                result
                `(cru.oil.if_else ,cond-expr ,result ,(build-cond (+ i 2)))))))
    (build-cond 1)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Export
;; ═══════════════════════════════════════════════════════════════════════════

{;; Layout
 : col
 : row
 : fragment
 : spacer
 
 ;; Text
 : text
 : badge
 : divider
 : hr
 
 ;; Interactive
 : spinner
 : progress
 : input
 : popup
 
 ;; Lists
 :bullet-list bullet-list
 :numbered-list numbered-list
 : kv
 
 ;; Control flow (when* and map-each to avoid shadowing Fennel's when/each)
 :when when*
 :if-else if-else
 :each map-each
 :map-each map-each
 
 ;; Factory
 : component
 
 ;; Advanced
 : scrollback
 : markup}
