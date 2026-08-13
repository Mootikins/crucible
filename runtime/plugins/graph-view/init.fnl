;;; graph-view — inspect a kiln's link graph from a plugin
;;;
;;; Read-only: every tool here reports on the graph and none of them writes.
;;; It is also the reference for what the three graph functions on `cru.kiln`
;;; do and how they compose:
;;;
;;;   cru.kiln.outlinks(path)          one hop, forward
;;;   cru.kiln.backlinks(path)         one hop, backward
;;;   cru.kiln.neighbors(path, depth)  `depth` hops, undirected
;;;
;;; Three properties of those functions shape every line below:
;;;
;;; 1. They are async. Calling one is ordinary Fennel, but only from a context
;;;    the daemon invokes asynchronously — a tool `fn`, a command `fn`, an
;;;    event handler. Never from module top level.
;;; 2. They speak resolved note *paths* ("Meta/Product.md"), in and out. A raw
;;;    wikilink target ("Product") is not a valid argument and never comes back
;;;    as a result: dangling links are dropped, so what you get back can always
;;;    be fed to the next call.
;;; 3. Results are sorted, deduped, and filtered to the kiln this plugin is
;;;    bound to. A path outside it is invisible — passing one yields an empty
;;;    table, indistinguishable from a note that simply has no links.
;;;
;;; Implementation and exact semantics: crates/crucible-lua/src/vault/mod.rs

(local M {})

;; `[plugins.graph-view]` arrives through `setup`, which the daemon calls at
;; load time with that section (or an empty table if there is none). There is
;; no `cru.config.<plugin-name>` table to read it from.
(local cfg {:max_depth 3})

(fn depth-limit [override]
  "Traversal depth, clamped to something a BFS can act on."
  (let [d (or (tonumber override) (tonumber cfg.max_depth) 3)]
    (math.max 1 (math.floor d))))

(fn trim [s]
  (when s (s:match "^%s*(.-)%s*$")))

(fn note-exists? [path]
  "Does `path` name a note this plugin may read?

  Worth asking before reporting an empty graph: the graph functions return an
  empty table both for an unknown path and for a known note with no resolved
  links, and those two deserve different messages."
  (not= nil (cru.kiln.get path)))

(fn rings [path depth]
  "Neighbours grouped by hop distance from `path`.

  `neighbors` returns everything within `d` hops as one flat list, so ring `d`
  is the depth-`d` set minus everything nearer. If you don't need the per-hop
  split, one `neighbors path depth` call is the whole job — the walk, the
  dedup and the cycle handling are already on the other side of the boundary."
  (let [out []
        seen {}]
    (for [d 1 depth]
      (let [ring []]
        (each [_ p (ipairs (cru.kiln.neighbors path d))]
          (when (not (. seen p))
            (tset seen p true)
            (table.insert ring p)))
        (table.insert out {:depth d :notes ring})))
    out))

(fn build-graph [path depth]
  "Everything the tools and the renderer need about one note's neighbourhood."
  (if (not (note-exists? path))
      {:note path :missing true}
      {:note path
       :depth depth
       :outlinks (cru.kiln.outlinks path)
       :backlinks (cru.kiln.backlinks path)
       :rings (rings path depth)}))

;; ============================================================================
;; Text rendering — what a command result can actually be
;; ============================================================================

(fn indented [paths empty-label]
  (if (= (length paths) 0)
      (.. "  (" empty-label ")")
      (let [lines []]
        (each [_ p (ipairs paths)]
          (table.insert lines (.. "  " p)))
        (table.concat lines "\n"))))

(fn render-text [graph]
  "A command's return value renders as a system message, so plain text it is."
  (if graph.missing
      (.. "graph: no note at '" graph.note "' in this kiln")
      (let [parts [(.. "graph: " graph.note)
                   (.. "outlinks (" (length graph.outlinks) ")")
                   (indented graph.outlinks "none")
                   (.. "backlinks (" (length graph.backlinks) ")")
                   (indented graph.backlinks "none")]]
        (each [_ ring (ipairs graph.rings)]
          (table.insert parts (.. "depth " ring.depth " (+" (length ring.notes) ")"))
          (table.insert parts (indented ring.notes "nothing new")))
        (table.concat parts "\n"))))

;; ============================================================================
;; Tools (for agents)
;; ============================================================================

(fn M.graph_links [args]
  "The resolved one-hop neighbourhood of a note, in both directions."
  (let [args (or args {})
        path (trim args.note)]
    (if (or (= path nil) (= path ""))
        {:error "note is required — a note path, e.g. Meta/Product.md"}
        (if (not (note-exists? path))
            {:error (.. "no note at '" path "' in this kiln")}
            (let [out (cru.kiln.outlinks path)
                  back (cru.kiln.backlinks path)]
              {:note path
               :outlinks out
               :backlinks back
               :outlink_count (length out)
               :backlink_count (length back)})))))

(fn M.graph_stats [args]
  "How connected a note is: one-hop counts, then reach per hop."
  (let [args (or args {})
        path (trim args.note)
        depth (depth-limit args.depth)]
    (if (or (= path nil) (= path ""))
        {:error "note is required — a note path, e.g. Meta/Product.md"}
        (let [graph (build-graph path depth)]
          (if graph.missing
              {:error (.. "no note at '" path "' in this kiln")}
              ;; The rings partition the depth-`depth` neighbour set, so their
              ;; sizes sum to it — no extra `neighbors` call needed for a total.
              (let [by-depth []]
                (var reachable 0)
                (each [_ ring (ipairs graph.rings)]
                  (table.insert by-depth {:depth ring.depth :new_notes (length ring.notes)})
                  (set reachable (+ reachable (length ring.notes))))
                {:note path
                 :depth depth
                 :outlink_count (length graph.outlinks)
                 :backlink_count (length graph.backlinks)
                 :reachable reachable
                 :new_notes_by_depth by-depth}))))))

;; ============================================================================
;; Command (for users)
;; ============================================================================

(fn M.graph_command [args]
  "`/graph <note path>` — print a note's neighbourhood.

  A command `fn` receives only its argument table (`{input = \"...\"}`, or nil
  when the user typed no argument); there is no context object, so there is no
  current note to default to."
  (let [path (trim (and args args.input))]
    (if (or (= path nil) (= path ""))
        "usage: /graph <note path>   e.g. /graph Meta/Product.md"
        (render-text (build-graph path (depth-limit nil))))))

;; ============================================================================
;; View (declared, not yet rendered)
;; ============================================================================

(fn render-node [path style-fg prefix]
  (cru.oil.text (.. prefix path) {:fg style-fg}))

;; Spec-table `views` are parsed and counted, but no client renders them yet —
;; see "Providing Views" in docs/Help/Extending/Creating Plugins.md. This one is
;; kept deliberately pure: it formats a graph someone else already fetched and
;; makes no `cru.kiln` call of its own, because the calling convention a future
;; renderer will use — and therefore whether it may await — is still undecided.
(fn M.graph_view [ctx]
  (let [graph (or (and ctx ctx.state ctx.state.graph)
                  {:note "(none)" :outlinks [] :backlinks [] :rings []})
        width (or (and ctx ctx.width) 60)
        rows [(cru.oil.text (.. "Graph: " graph.note) {:bold true :fg "green"})
              (cru.oil.text (string.rep "-" width) {:fg "gray"})]]
    (each [_ path (ipairs graph.backlinks)]
      (table.insert rows (render-node path "magenta" "<- ")))
    (table.insert rows (cru.oil.text (.. "*  " graph.note) {:bold true :fg "yellow"}))
    (each [_ path (ipairs graph.outlinks)]
      (table.insert rows (render-node path "white" "-> ")))
    (table.insert rows (cru.oil.text (string.rep "-" width) {:fg "gray"}))
    (table.insert rows
                  (cru.oil.text (string.format "%d in | %d out"
                                               (length graph.backlinks)
                                               (length graph.outlinks))
                                {:fg "gray"}))
    (cru.oil.col {:gap 0} (table.unpack rows))))

;; ============================================================================
;; Plugin spec
;; ============================================================================

{:name "graph-view"
 :version "0.2.0"
 :description "Inspect a kiln's link graph"
 :capabilities ["kiln" "ui"]

 :tools {:graph_links {:desc "Resolved outlinks and backlinks of a note"
                       :params [{:name "note" :type "string" :desc "Note path, e.g. Meta/Product.md"}]
                       :fn M.graph_links}
         :graph_stats {:desc "Link counts for a note, plus how many new notes each extra hop reaches"
                       :params [{:name "note" :type "string" :desc "Note path, e.g. Meta/Product.md"}
                                {:name "depth" :type "number" :desc "Max hops (default: max_depth, 3)" :optional true}]
                       :fn M.graph_stats}}

 :commands {:graph {:desc "Print a note's link neighbourhood"
                    :hint "<note path>"
                    :fn M.graph_command}}

 :views {:graph {:desc "Link neighbourhood of a note (declared; no client renders views yet)"
                 :fn M.graph_view}}

 :setup (fn [c]
          (when (and c (not= nil c.max_depth))
            (tset cfg :max_depth c.max_depth)))}
