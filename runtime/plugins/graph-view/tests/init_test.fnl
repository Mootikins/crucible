;;; Tests for graph-view.
;;;
;;; Also the repo's only Fennel suite, so it is what proves the runner's
;;; `.fnl` compile path works end to end rather than merely being claimed by
;;; the docs.
;;;
;;; Required by DIRECTORY NAME, never by `init`: the runner's package.path
;;; mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.

(local plugin (require :graph-view))

;; `cru.kiln`'s generated mock ignores the depth argument, which would make
;; every ring past the first empty and the traversal untestable. These stubs
;; stand in a small fixed graph instead:
;;
;;   A -> B -> C        D -> A
;;
;; so A has one outlink (B), one backlink (D), and reaches B at depth 1 and C
;; at depth 2.
(fn install-graph []
  (set cru.kiln.get (fn [path]
                      (if (or (= path "A.md") (= path "B.md")
                              (= path "C.md") (= path "D.md"))
                          {:path path}
                          nil)))
  (set cru.kiln.outlinks (fn [path]
                           (case path
                             "A.md" ["B.md"]
                             "B.md" ["C.md"]
                             "D.md" ["A.md"]
                             _ [])))
  (set cru.kiln.backlinks (fn [path]
                            (case path
                              "A.md" ["D.md"]
                              "B.md" ["A.md"]
                              "C.md" ["B.md"]
                              _ [])))
  ;; Undirected reach within `depth` hops, cumulative, as the real API is.
  (set cru.kiln.neighbors (fn [path depth]
                            (if (not= path "A.md")
                                []
                                (if (<= depth 1) ["B.md" "D.md"]
                                    ["B.md" "D.md" "C.md"])))))

(describe "graph-view"
  (fn []
    (before_each (fn []
                   (test_mocks.setup)
                   (install-graph)
                   ;; `cfg` is module state that survives require caching.
                   (plugin.setup {:max_depth 3})))

    (after_each (fn [] (test_mocks.reset)))

    (describe "graph_links"
      (fn []
        (it "requires a note"
            (fn []
              (assert.truthy (. (plugin.tools.graph_links.fn {}) :error))))

        (it "rejects a blank note"
            (fn []
              (assert.truthy (. (plugin.tools.graph_links.fn {:note "   "}) :error))))

        (it "rejects a nil argument table"
            (fn []
              (assert.truthy (. (plugin.tools.graph_links.fn nil) :error))))

        (it "reports a note that is not in the kiln"
            (fn []
              (let [result (plugin.tools.graph_links.fn {:note "Missing.md"})]
                (assert.truthy (result.error:find "no note at" 1 true)))))

        (it "returns outlinks and backlinks with their counts"
            (fn []
              (let [result (plugin.tools.graph_links.fn {:note "A.md"})]
                (assert.equal result.note "A.md")
                (assert.deep_equal result.outlinks ["B.md"])
                (assert.deep_equal result.backlinks ["D.md"])
                (assert.equal result.outlink_count 1)
                (assert.equal result.backlink_count 1))))

        (it "trims surrounding whitespace off the note path"
            (fn []
              (let [result (plugin.tools.graph_links.fn {:note "  A.md  "})]
                (assert.equal result.note "A.md"))))

        (it "reports a note with no links as empty rather than missing"
            (fn []
              (let [result (plugin.tools.graph_links.fn {:note "C.md"})]
                (assert.falsy result.error)
                (assert.equal result.outlink_count 0)
                (assert.equal result.backlink_count 1))))))

    (describe "graph_stats"
      (fn []
        (it "requires a note"
            (fn []
              (assert.truthy (. (plugin.tools.graph_stats.fn {}) :error))))

        (it "reports a note that is not in the kiln"
            (fn []
              (assert.truthy (. (plugin.tools.graph_stats.fn {:note "Missing.md"}) :error))))

        (it "counts what each extra hop newly reaches"
            (fn []
              (let [result (plugin.tools.graph_stats.fn {:note "A.md" :depth 2})]
                (assert.equal result.depth 2)
                ;; Depth 1 brings B and D; depth 2 adds only C.
                (assert.equal (. result.new_notes_by_depth 1 :new_notes) 2)
                (assert.equal (. result.new_notes_by_depth 2 :new_notes) 1)
                (assert.equal result.reachable 3))))

        (it "never counts the same note in two rings"
            (fn []
              (let [result (plugin.tools.graph_stats.fn {:note "A.md" :depth 3})
                    total (accumulate [sum 0 _ ring (ipairs result.new_notes_by_depth)]
                            (+ sum ring.new_notes))]
                (assert.equal total result.reachable))))

        (it "uses the configured max_depth when none is given"
            (fn []
              (plugin.setup {:max_depth 1})
              (let [result (plugin.tools.graph_stats.fn {:note "A.md"})]
                (assert.equal result.depth 1)
                (assert.equal (length result.new_notes_by_depth) 1))))

        (it "lets an explicit depth override the configured one"
            (fn []
              (plugin.setup {:max_depth 1})
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md" :depth 2}) :depth) 2)))

        (it "clamps a depth below one up to one"
            (fn []
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md" :depth 0}) :depth) 1)
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md" :depth -5}) :depth) 1)))

        (it "floors a fractional depth"
            (fn []
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md" :depth 2.7}) :depth) 2)))

        (it "falls back to the default when depth is not a number"
            (fn []
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md" :depth "deep"}) :depth) 3)))))

    (describe "setup"
      (fn []
        (it "ignores a nil config"
            (fn []
              (plugin.setup nil)
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md"}) :depth) 3)))

        (it "ignores a config with no max_depth"
            (fn []
              (plugin.setup {})
              (assert.equal (. (plugin.tools.graph_stats.fn {:note "A.md"}) :depth) 3)))))

    (describe "the /graph command"
      (fn []
        (it "prints usage when given no argument"
            (fn []
              (assert.truthy ((. plugin.commands :graph :fn) nil))))

        (it "prints usage when given a blank argument"
            (fn []
              (let [out ((. plugin.commands :graph :fn) {:input "  "})]
                (assert.truthy (out:find "usage:" 1 true)))))

        (it "says so when the note is not in the kiln"
            (fn []
              (let [out ((. plugin.commands :graph :fn) {:input "Missing.md"})]
                (assert.truthy (out:find "no note at" 1 true)))))

        (it "renders outlinks and backlinks as text"
            (fn []
              (let [out ((. plugin.commands :graph :fn) {:input "A.md"})]
                (assert.truthy (out:find "graph: A.md" 1 true))
                (assert.truthy (out:find "outlinks (1)" 1 true))
                (assert.truthy (out:find "backlinks (1)" 1 true))
                (assert.truthy (out:find "B.md" 1 true))
                (assert.truthy (out:find "D.md" 1 true)))))

        (it "labels an empty side rather than leaving a blank line"
            (fn []
              (let [out ((. plugin.commands :graph :fn) {:input "C.md"})]
                (assert.truthy (out:find "(none)" 1 true)))))))

    (describe "plugin metadata"
      (fn []
        (it "exports the correct name"
            (fn [] (assert.equal plugin.name "graph-view")))

        (it "exports a version string"
            (fn [] (assert.equal (type plugin.version) "string")))

        (it "exports a setup function so its config is applied"
            (fn [] (assert.equal (type plugin.setup) "function")))

        (it "exports both graph tools"
            (fn []
              (assert.truthy plugin.tools.graph_links)
              (assert.truthy plugin.tools.graph_stats)))

        (it "exports the /graph command"
            (fn []
              (assert.truthy plugin.commands.graph)
              (assert.equal (type plugin.commands.graph.fn) "function")))

        (it "declares the graph view"
            (fn []
              (assert.truthy plugin.views.graph)
              (assert.equal (type plugin.views.graph.fn) "function")))))))
