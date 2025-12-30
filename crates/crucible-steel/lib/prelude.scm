;;; Crucible Prelude for Steel Scripts
;;;
;;; Provides common utilities and helpers for working with
;;; knowledge graphs, notes, and the Crucible ecosystem.
;;;
;;; This file is automatically loaded by the Steel executor.

;;; ============================================================================
;;; Core Utilities
;;; ============================================================================

;; Identity function
(define (identity x) x)

;; Compose two functions
(define (compose f g)
  (lambda (x) (f (g x))))

;; Flip argument order of a binary function
(define (flip f)
  (lambda (a b) (f b a)))

;; Apply a function n times
(define (iterate n f x)
  (if (<= n 0)
      x
      (iterate (- n 1) f (f x))))

;; Const function - ignores second argument
(define (const x)
  (lambda (_) x))

;;; ============================================================================
;;; List Utilities
;;; ============================================================================

;; Check if a list is empty
(define (empty? lst)
  (null? lst))

;; Get the first element or default
(define (first-or lst default)
  (if (null? lst)
      default
      (car lst)))

;; Get the last element of a list
(define (last lst)
  (cond
    [(null? lst) (error "last: empty list")]
    [(null? (cdr lst)) (car lst)]
    [else (last (cdr lst))]))

;; Take first n elements
(define (take n lst)
  (cond
    [(or (<= n 0) (null? lst)) '()]
    [else (cons (car lst) (take (- n 1) (cdr lst)))]))

;; Drop first n elements
(define (drop n lst)
  (cond
    [(or (<= n 0) (null? lst)) lst]
    [else (drop (- n 1) (cdr lst))]))

;; Partition list by predicate
(define (partition pred lst)
  (let loop ([remaining lst] [yes '()] [no '()])
    (cond
      [(null? remaining)
       (list (reverse yes) (reverse no))]
      [(pred (car remaining))
       (loop (cdr remaining) (cons (car remaining) yes) no)]
      [else
       (loop (cdr remaining) yes (cons (car remaining) no))])))

;; Find first element matching predicate
(define (find pred lst)
  (cond
    [(null? lst) #f]
    [(pred (car lst)) (car lst)]
    [else (find pred (cdr lst))]))

;; Check if any element matches predicate
(define (any? pred lst)
  (cond
    [(null? lst) #f]
    [(pred (car lst)) #t]
    [else (any? pred (cdr lst))]))

;; Check if all elements match predicate
(define (all? pred lst)
  (cond
    [(null? lst) #t]
    [(not (pred (car lst))) #f]
    [else (all? pred (cdr lst))]))

;; Flatten nested lists one level
(define (flatten lst)
  (cond
    [(null? lst) '()]
    [(list? (car lst)) (append (car lst) (flatten (cdr lst)))]
    [else (cons (car lst) (flatten (cdr lst)))]))

;; Zip two lists
(define (zip lst1 lst2)
  (cond
    [(or (null? lst1) (null? lst2)) '()]
    [else (cons (list (car lst1) (car lst2))
                (zip (cdr lst1) (cdr lst2)))]))

;; Interpose separator between elements
(define (interpose sep lst)
  (cond
    [(null? lst) '()]
    [(null? (cdr lst)) lst]
    [else (cons (car lst) (cons sep (interpose sep (cdr lst))))]))

;;; ============================================================================
;;; Hash/Object Utilities
;;; ============================================================================

;; Get value from hash with default
(define (hash-get h key default)
  (if (hash-contains? h key)
      (hash-ref h key)
      default))

;; Update a hash key with a function
(define (hash-update h key f default)
  (let ([old-val (hash-get h key default)])
    (hash-insert h key (f old-val))))

;; Get nested value from hash using a list of keys
(define (hash-get-in h keys default)
  (cond
    [(null? keys) h]
    [(not (hash? h)) default]
    [(not (hash-contains? h (car keys))) default]
    [else (hash-get-in (hash-ref h (car keys)) (cdr keys) default)]))

;; Merge two hashes (second overrides first)
(define (hash-merge h1 h2)
  (let loop ([keys (hash-keys->list h2)] [result h1])
    (if (null? keys)
        result
        (loop (cdr keys)
              (hash-insert result (car keys) (hash-ref h2 (car keys)))))))

;; Convert hash to association list
(define (hash->alist h)
  (map (lambda (k) (list k (hash-ref h k)))
       (hash-keys->list h)))

;;; ============================================================================
;;; String Utilities
;;; ============================================================================

;; Check if string is empty
(define (blank? s)
  (or (not (string? s))
      (string=? s "")))

;; Join strings with separator
(define (string-join lst sep)
  (cond
    [(null? lst) ""]
    [(null? (cdr lst)) (car lst)]
    [else (string-append (car lst) sep (string-join (cdr lst) sep))]))

;; Repeat a string n times (tail-recursive)
(define (string-repeat s n)
  (let loop ([n n] [acc ""])
    (if (<= n 0)
        acc
        (loop (- n 1) (string-append acc s)))))

;;; ============================================================================
;;; Knowledge Graph Helpers
;;; ============================================================================

;; Create a note structure
(define (make-note title path)
  (hash 'title title 'path path 'links '() 'tags '()))

;; Create a note with links
(define (make-note-with-links title path links)
  (hash 'title title 'path path 'links links 'tags '()))

;; Add a tag to a note
(define (note-add-tag note tag)
  (hash-update note 'tags (lambda (tags) (cons tag tags)) '()))

;; Add a link to a note
(define (note-add-link note link)
  (hash-update note 'links (lambda (links) (cons link links)) '()))

;; Check if note has a specific tag
(define (note-has-tag? note tag)
  (member tag (hash-get note 'tags '())))

;; Check if note links to another note
(define (note-links-to? note target-title)
  (member target-title (hash-get note 'links '())))

;;; ============================================================================
;;; Result/Option Helpers
;;; ============================================================================

;; Create success result
(define (ok value)
  (hash 'success #t 'value value 'error #f))

;; Create error result
(define (err message)
  (hash 'success #f 'value #f 'error message))

;; Check if result is success
(define (ok? result)
  (and (hash? result) (hash-get result 'success #f)))

;; Check if result is error
(define (err? result)
  (and (hash? result) (not (hash-get result 'success #t))))

;; Map over result value (if success)
(define (result-map f result)
  (if (ok? result)
      (ok (f (hash-ref result 'value)))
      result))

;; Chain results (monadic bind)
(define (result-bind f result)
  (if (ok? result)
      (f (hash-ref result 'value))
      result))

;; Get result value or default
(define (result-get result default)
  (if (ok? result)
      (hash-ref result 'value)
      default))

;;; ============================================================================
;;; Pipeline/Threading
;;; ============================================================================

;; Thread value through a series of single-argument functions
;; Usage: (pipeline 5 inc double) => (double (inc 5))
(define (pipeline value . fns)
  (if (null? fns)
      value
      (apply pipeline
             ((car fns) value)
             (cdr fns))))

;;; ============================================================================
;;; Debug/Logging Helpers
;;; ============================================================================

;; Print value and return it (for debugging)
(define (tap x)
  (displayln x)
  x)

;; Print with label
(define (tap-with label x)
  (display label)
  (display ": ")
  (displayln x)
  x)

;; Time an expression (returns pair of result and time)
;; TODO: Implement timing when Steel's time functions are available
;; For now, this is a stub that just returns the result
(define (timed thunk)
  (let ([result (thunk)])
    result))
