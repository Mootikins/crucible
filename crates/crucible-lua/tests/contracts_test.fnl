;; Test: contracts.fnl functionality

(local c (require :contracts))

;; ═══════════════════════════════════════════════════════════════════════════
;; Test predicates
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-predicates []
  (assert (c.string? "hello") "string? should match strings")
  (assert (not (c.string? 42)) "string? should not match numbers")

  (assert (c.positive? 5) "positive? should match positive numbers")
  (assert (not (c.positive? -1)) "positive? should not match negative")
  (assert (not (c.positive? 0)) "positive? should not match zero")

  (assert (c.non-empty? "hi") "non-empty? should match non-empty strings")
  (assert (not (c.non-empty? "")) "non-empty? should not match empty strings")
  (assert (c.non-empty? [1 2]) "non-empty? should match non-empty tables")

  (crucible.log "info" "Predicates: PASS"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Test combinators
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-combinators []
  ;; and-c
  (let [string-and-non-empty (c.and-c c.string? c.non-empty?)]
    (assert (string-and-non-empty "hello") "and-c should pass when both pass")
    (assert (not (string-and-non-empty "")) "and-c should fail when one fails")
    (assert (not (string-and-non-empty 42)) "and-c should fail for wrong type"))

  ;; or-c
  (let [string-or-number (c.or-c c.string? c.number?)]
    (assert (string-or-number "hi") "or-c should pass for string")
    (assert (string-or-number 42) "or-c should pass for number")
    (assert (not (string-or-number {})) "or-c should fail for table"))

  ;; not-c
  (let [not-nil (c.not-c c.nil?)]
    (assert (not-nil 1) "not-c nil? should pass for non-nil")
    (assert (not (not-nil nil)) "not-c nil? should fail for nil"))

  (crucible.log "info" "Combinators: PASS"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Test field contracts
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-field-contracts []
  (let [has-name (c.field-c :name c.string?)
        event {:name "test" :value 42}]
    (assert (has-name event) "field-c should pass for matching field")
    (assert (not (has-name {:value 42})) "field-c should fail for missing field")
    (assert (not (has-name {:name 123})) "field-c should fail for wrong type"))

  (crucible.log "info" "Field contracts: PASS"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Test function contracts
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-function-contracts []
  ;; Working contract
  (let [double (c.wrap-with-contract
                 (fn [x] (* x 2))
                 {:name "double"
                  :pre [c.number?]
                  :post [c.number?]})]
    (assert (= (double 5) 10) "Contracted function should work"))

  ;; Pre-condition failure
  (let [double (c.wrap-with-contract
                 (fn [x] (* x 2))
                 {:name "double"
                  :pre [c.number?]
                  :post [c.number?]})
        (ok err) (pcall double "not a number")]
    (assert (not ok) "Should fail pre-condition")
    (assert (string.find err "Contract violation") "Should have violation message")
    (assert (string.find err "caller") "Should blame caller"))

  (crucible.log "info" "Function contracts: PASS"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Test preserves
;; ═══════════════════════════════════════════════════════════════════════════

(fn test-preserves []
  ;; Should pass: timestamp preserved
  (let [handler (c.wrap-with-contract
                  (fn [event]
                    (tset event :processed true)
                    event)
                  {:name "preserve-test"
                   :pre [c.table?]
                   :post [c.table?]
                   :preserves [:timestamp]})
        event {:timestamp 12345 :data "hello"}
        result (handler event)]
    (assert (= result.timestamp 12345) "Should preserve timestamp")
    (assert result.processed "Should add processed flag"))

  ;; Should fail: timestamp modified
  (let [bad-handler (c.wrap-with-contract
                      (fn [event]
                        (tset event :timestamp 99999)  ;; BAD: changes timestamp
                        event)
                      {:name "bad-preserve-test"
                       :pre [c.table?]
                       :post [c.table?]
                       :preserves [:timestamp]})
        event {:timestamp 12345}
        (ok err) (pcall bad-handler event)]
    (assert (not ok) "Should fail when preserve violated")
    (assert (string.find err "preserved") "Should mention preserved in error"))

  (crucible.log "info" "Preserves: PASS"))

;; ═══════════════════════════════════════════════════════════════════════════
;; Main test runner
;; ═══════════════════════════════════════════════════════════════════════════

(fn handler [args]
  (test-predicates)
  (test-combinators)
  (test-field-contracts)
  (test-function-contracts)
  (test-preserves)

  {:success true
   :tests [:predicates :combinators :field-contracts :function-contracts :preserves]})

{: handler}
