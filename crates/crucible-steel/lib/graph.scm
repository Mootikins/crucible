;;; Graph traversal module for Crucible
;;;
;;; Provides functions for traversing note graphs using idiomatic Scheme.
;;;
;;; Usage:
;;;   ;; Build a graph as a list of notes
;;;   (define notes
;;;     (list
;;;       (hash 'title "Index" 'path "Index.md" 'links '("Project A" "Project B"))
;;;       (hash 'title "Project A" 'path "a.md" 'links '("Index"))
;;;       (hash 'title "Project B" 'path "b.md" 'links '())))
;;;
;;;   ;; Find a note by title
;;;   (graph-find notes "Index")  ; => note hash or #f
;;;
;;;   ;; Get outlinks (notes this note links to)
;;;   (graph-outlinks notes "Index")  ; => list of notes
;;;
;;;   ;; Get inlinks (notes linking to this note)
;;;   (graph-inlinks notes "Index")   ; => list of notes
;;;
;;;   ;; Get all neighbors (union of outlinks and inlinks)
;;;   (graph-neighbors notes "Index") ; => list of notes

;;; ============================================================================
;;; Helper functions (must be defined first)
;;; ============================================================================

;; Helper: get from hash with default value
;; Steel's hash-ref doesn't support defaults like Racket
(define (hash-try-get h key default)
  (if (hash-contains? h key)
      (hash-ref h key)
      default))

;; Filter and map in one pass - applies f to each element,
;; keeps results that are not #f
(define (filter-map f lst)
  (cond
    [(null? lst) '()]
    [else
     (let ([result (f (car lst))])
       (if result
           (cons result (filter-map f (cdr lst)))
           (filter-map f (cdr lst))))]))

;; Remove duplicates based on a key function
(define (remove-duplicates-by key-fn lst)
  (let loop ([remaining lst] [seen '()] [result '()])
    (cond
      [(null? remaining) (reverse result)]
      [else
       (let ([key (key-fn (car remaining))])
         (if (member key seen)
             (loop (cdr remaining) seen result)
             (loop (cdr remaining)
                   (cons key seen)
                   (cons (car remaining) result))))])))

;;; ============================================================================
;;; Note accessors (work with hash tables)
;;; ============================================================================

;; Get the title from a note hash
(define (note-title note)
  (hash-ref note 'title))

;; Get the path from a note hash
(define (note-path note)
  (hash-ref note 'path))

;; Get the links from a note hash (list of strings)
(define (note-links note)
  (hash-try-get note 'links '()))

;; Get the tags from a note hash (list of strings)
(define (note-tags note)
  (hash-try-get note 'tags '()))

;;; ============================================================================
;;; Graph traversal
;;; ============================================================================

;; Find a note by title in a list of notes
;; Returns the note hash or #f if not found
(define (graph-find notes title)
  (cond
    [(null? notes) #f]
    [(equal? (note-title (car notes)) title) (car notes)]
    [else (graph-find (cdr notes) title)]))

;; Get all notes that a given note links to (outlinks)
;; Returns a list of note hashes
(define (graph-outlinks notes title)
  (let ([source (graph-find notes title)])
    (if source
        (filter-map
         (lambda (link-title)
           (graph-find notes link-title))
         (note-links source))
        '())))

;; Get all notes that link to a given note (inlinks/backlinks)
;; Returns a list of note hashes
(define (graph-inlinks notes title)
  (filter
   (lambda (note)
     (member title (note-links note)))
   notes))

;; Get all neighbors (both outlinks and inlinks, deduplicated)
(define (graph-neighbors notes title)
  (let* ([out (graph-outlinks notes title)]
         [in (graph-inlinks notes title)]
         [all (append out in)])
    ;; Remove duplicates by title
    (remove-duplicates-by note-title all)))

;; Get all titles from a list of notes
(define (graph-titles notes)
  (map note-title notes))

;; Filter notes by tag
(define (graph-filter-by-tag notes tag)
  (filter
   (lambda (note)
     (member tag (note-tags note)))
   notes))
