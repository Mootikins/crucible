;;; Graph View Plugin
;;; Knowledge graph visualization using Fennel and cru.oil

(local M {})

(fn get-config [key default]
  "Get plugin configuration value"
  (let [cfg (or cru.config.graph-view {})]
    (or (. cfg key) default)))

(fn collect-links [note-name visited depth max-depth]
  "Recursively collect links from a note"
  (when (and (< depth max-depth) (not (. visited note-name)))
    (tset visited note-name true)
    (let [outlinks (or (cru.vault.outlinks note-name) [])
          nodes [{:name note-name :depth depth}]
          edges []]
      (each [_ link (ipairs outlinks)]
        (table.insert edges {:from note-name :to link})
        (let [sub (collect-links link visited (+ depth 1) max-depth)]
          (when sub
            (each [_ n (ipairs sub.nodes)]
              (table.insert nodes n))
            (each [_ e (ipairs sub.edges)]
              (table.insert edges e)))))
      {:nodes nodes :edges edges})))

(fn build-graph [center-note]
  "Build graph data starting from a center note"
  (let [max-depth (get-config :max_depth 3)
        visited {}
        forward (or (collect-links center-note visited 0 max-depth)
                    {:nodes [] :edges []})
        backlinks (or (cru.vault.backlinks center-note) [])]
    (each [_ link (ipairs backlinks)]
      (when (not (. visited link))
        (table.insert forward.nodes {:name link :depth -1})
        (table.insert forward.edges {:from link :to center-note})))
    forward))

(fn render-node [node selected]
  "Render a single graph node"
  (let [style (if selected {:bold true :fg "cyan"}
                  (= node.depth 0) {:bold true :fg "yellow"}
                  (< node.depth 0) {:fg "magenta"}
                  {:fg "white"})
        indent (string.rep "  " (math.max 0 (+ node.depth 1)))
        prefix (if (< node.depth 0) "<- "
                   (> node.depth 0) "-> "
                   "* ")]
    (cru.oil.text (.. indent prefix node.name) style)))

(fn render-graph [ctx state]
  "Render the graph view"
  (let [graph (or state.graph {:nodes [] :edges []})
        selected (or state.selected 0)
        title (.. "Graph: " (or state.center "unknown"))
        header (cru.oil.text title {:bold true :fg "green"})
        separator (cru.oil.text (string.rep "-" ctx.width) {:fg "gray"})
        stats (cru.oil.text
                (string.format "Nodes: %d | Edges: %d | Depth: %d"
                               (length graph.nodes)
                               (length graph.edges)
                               (get-config :max_depth 3))
                {:fg "gray"})
        node-views []]
    (each [i node (ipairs graph.nodes)]
      (table.insert node-views (render-node node (= i (+ selected 1)))))
    (cru.oil.col {:gap 0}
      header
      stats
      separator
      (cru.oil.col {:gap 0} (unpack node-views))
      separator
      (cru.oil.text "j/k: navigate | Enter: focus | q: quit" {:fg "gray"}))))

;;; Graph visualization view
(fn M.graph_view [ctx]
  (let [state (or ctx.state {:center (or ctx.current_note "index")
                              :selected 0
                              :graph nil})]
    (when (not state.graph)
      (tset state :graph (build-graph state.center)))
    (render-graph ctx state)))

;;; Handle keyboard input for graph view
(fn M.graph_handler [key ctx]
  (let [state ctx.state
        nodes (or state.graph.nodes [])]
    (if (= key "q") (ctx:close_view)
        (= key "j") (tset state :selected (math.min (+ state.selected 1) (- (length nodes) 1)))
        (= key "k") (tset state :selected (math.max (- state.selected 1) 0))
        (= key "Enter") (let [node (. nodes (+ state.selected 1))]
                          (when node
                            (tset state :center node.name)
                            (tset state :graph (build-graph node.name))
                            (tset state :selected 0)))
        (= key "r") (do (tset state :graph (build-graph state.center))
                        (tset state :selected 0)))
    (ctx:refresh)))

;;; Get graph statistics
(fn M.graph_stats [args]
  (let [center (or args.note "index")
        depth (or args.depth (get-config :max_depth 3))
        graph (build-graph center)]
    {:center center
     :total_nodes (length graph.nodes)
     :total_edges (length graph.edges)
     :max_depth depth
     :nodes_by_depth (do
                       (local counts {})
                       (each [_ node (ipairs graph.nodes)]
                         (let [d node.depth]
                           (tset counts d (+ (or (. counts d) 0) 1))))
                       counts)}))

;;; Open graph view command
(fn M.graph_command [args ctx]
  (let [note (or (and args._positional (. args._positional 1)) ctx.current_note "index")]
    (ctx:open_view "graph" {:center note :selected 0 :graph nil})))

{:name "graph-view"
 :version "1.0.0"
 :description "Interactive knowledge graph visualization"

 :tools {:graph_stats {:desc "Get knowledge graph statistics"
                       :params [{:name "note" :type "string" :desc "Center note (default: current note)" :optional true}
                                {:name "depth" :type "number" :desc "Max traversal depth (default: 3)" :optional true}]
                       :fn M.graph_stats}}

 :commands {:graph {:desc "Open knowledge graph view"
                    :hint "[note]"
                    :fn M.graph_command}}

 :views {:graph {:desc "Interactive knowledge graph visualization"
                 :fn M.graph_view
                 :handler M.graph_handler}}}
