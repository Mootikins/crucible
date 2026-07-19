use super::*;

/// The `kiln.graph` handler returns every note and every deduped link edge of
/// a real (processed) kiln: resolved edges name a note path that joins on
/// `notes[].path`, the unresolved edge keeps its dangling target key, tags
/// come through, and self-links are excluded.
#[tokio::test]
async fn kiln_graph_returns_nodes_and_deduped_edges() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let files: &[(&str, &str)] = &[
        (
            "Alpha.md",
            "# Alpha\n\n#rust #notes\n\nlinks [[Beta]] and [[Ghost]] and self [[Alpha]]\n",
        ),
        ("Beta.md", "# Beta\n\npoints [[Gamma]]\n"),
        ("Gamma.md", "# Gamma\n"),
    ];
    for (rel, content) in files {
        std::fs::write(root.join(rel), content).unwrap();
    }

    let km = Arc::new(KilnManager::new());
    km.open_and_process(&root, false).await.unwrap();

    let req = Request {
        jsonrpc: "2.0".to_string(),
        id: Some(RequestId::Number(1)),
        method: "kiln.graph".to_string(),
        params: json!({ "kiln": root.to_string_lossy() }),
    };

    let resp = crate::server::kiln::handle_kiln_graph(req, &km).await;
    assert!(resp.error.is_none(), "handler errored: {:?}", resp.error);
    let result = resp.result.expect("result present");

    let notes = result["notes"].as_array().expect("notes array");
    let mut note_paths: Vec<&str> = notes.iter().map(|n| n["path"].as_str().unwrap()).collect();
    note_paths.sort_unstable();
    assert_eq!(note_paths, vec!["Alpha.md", "Beta.md", "Gamma.md"]);

    // Tags flow through from frontmatter.
    let alpha = notes
        .iter()
        .find(|n| n["path"] == "Alpha.md")
        .expect("Alpha present");
    let mut tags: Vec<&str> = alpha["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    tags.sort_unstable();
    assert_eq!(tags, vec!["notes", "rust"]);

    let links = result["links"].as_array().expect("links array");
    let mut edges: Vec<(String, String, bool)> = links
        .iter()
        .map(|l| {
            (
                l["source"].as_str().unwrap().to_string(),
                l["target"].as_str().unwrap().to_string(),
                l["resolved"].as_bool().unwrap(),
            )
        })
        .collect();
    edges.sort();

    assert_eq!(
        edges,
        vec![
            ("Alpha.md".into(), "Beta.md".into(), true),
            ("Alpha.md".into(), "ghost".into(), false),
            ("Beta.md".into(), "Gamma.md".into(), true),
        ],
        "self-link excluded; resolved edges use note paths; dangling keeps target_key"
    );

    // Every resolved target must be joinable against a node path.
    let node_set: std::collections::HashSet<&str> =
        note_paths.iter().copied().collect();
    for (_, target, resolved) in &edges {
        if *resolved {
            assert!(
                node_set.contains(target.as_str()),
                "resolved target {target} has no matching node"
            );
        }
    }
}
