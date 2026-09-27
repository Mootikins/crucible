use super::*;
use serde_json::json;
use tempfile::TempDir;

/// Run one Bases operation as a person at a client would.
async fn run(op: BaseOperation, root: &Path, params: serde_json::Value) -> serde_json::Value {
    let root = root.canonicalize().unwrap();
    operation::execute(op, &root, params, &disposition::Writer::default())
        .await
        .unwrap()
}
fn applied(answer: &serde_json::Value) -> bool {
    matches!(answer["status"].as_str(), Some("applied" | "unchanged"))
}

fn request(yaml: &str) -> Query {
    Query {
        kiln: "test".into(),
        source: Source::Inline { yaml: yaml.into() },
        view: None,
        host: None,
    }
}
#[tokio::test]
async fn bases_query_filters_formulas_groups_sorts_limits_and_attachments() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    tokio::fs::write(
        root.join("a.md"),
        "---\nstatus: todo\nprice: 2\ntags: [book/fiction]\n---\n[[b]]\n",
    )
    .await
    .unwrap();
    tokio::fs::write(root.join("b.md"), "---\nstatus: done\nprice: 9\n---\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("photo.png"), [0, 1, 2])
        .await
        .unwrap();
    let all = query(root, &request("views: [{type: table, name: All}]"))
        .await
        .unwrap();
    assert_eq!(all.rows.len(), 3);
    let yaml="filters: 'file.hasTag(\"book\")'\nformulas:\n  double: price * 2\nviews:\n  - type: kanban\n    name: Board\n    filters: 'formula.double > 3'\n    order: [file.name, formula.double]\n    groupBy: {property: note.status, direction: ASC}\n    groupOrder: [todo, doing]\n    summaries: {formula.double: Sum}\n";
    let result = query(root, &request(yaml)).await.unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].path, "a.md");
    assert_eq!(
        result.rows[0].values["formula.double"],
        BaseValue::Number(4.0)
    );
    assert_eq!(result.groups.len(), 2);
    assert_eq!(result.summaries["formula.double"], BaseValue::Number(4.0));
    let mut r =
        request("filters: 'file.hasLink(this.file)'\nviews: [{type: table, name: Backlinks}]");
    r.host = Some("b.md".into());
    assert_eq!(query(root, &r).await.unwrap().rows.len(), 1);
    let r=request("views: [{type: table, name: Sorted, sort: [{property: note.price, direction: DESC}], limit: 1}]");
    assert_eq!(query(root, &r).await.unwrap().rows[0].path, "b.md");
}
#[tokio::test]
async fn bases_property_writes_refuse_stale_and_preserve_body() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let path = root.join("a.md");
    let text = "---\r\nstatus: [todo]\r\nother: 42\r\n---\r\n# Body\r\n\r\n";
    tokio::fs::write(&path, text).await.unwrap();
    let params = json!({"path":"a.md","key":"note.status","value":"done","ancestor_hash":crucible_core::note_edit::disk_hash(text)});
    let result = run(BaseOperation::SetProperty, root, params.clone()).await;
    assert!(applied(&result));
    let changed = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(changed.ends_with("# Body\r\n\r\n"));
    assert!(changed.contains("- done"));
    assert!(changed.contains("other: 42"));
    assert_eq!(
        run(BaseOperation::SetProperty, root, params.clone()).await["status"],
        "stale"
    );
    assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), changed);
}
#[tokio::test]
async fn bases_create_derives_properties_and_never_overwrites() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    tokio::fs::create_dir(root.join("tickets")).await.unwrap();
    let yaml="filters:\n  and:\n    - 'file.inFolder(\"tickets\")'\n    - 'status == \"todo\"'\n    - 'file.hasTag(\"work\")'\nviews: [{type: table, name: Tasks, order: [file.name, note.owner]}]";
    let p = json!({"source":{"yaml":yaml},"name":"Task","content":"Body"});
    let a = run(BaseOperation::CreateEntry, root, p.clone()).await;
    let b = run(BaseOperation::CreateEntry, root, p.clone()).await;
    assert_eq!(a["path"], "tickets/Task.md");
    assert_eq!(b["path"], "tickets/Task 1.md");
    assert_eq!(query(root, &request(yaml)).await.unwrap().rows.len(), 2);
}
#[tokio::test]
async fn bases_containment_rejects_traversal_and_symlink() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = dir.path();
    tokio::fs::write(outside.path().join("secret.base"), "views: []")
        .await
        .unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path().join("secret.base"), root.join("escape.base"))
            .unwrap();
        assert!(load(
            root,
            &Source::Path {
                path: "escape.base".into()
            }
        )
        .await
        .is_err());
    }
    assert!(contained(root, "../secret.base").is_err());
}
#[tokio::test]
async fn bases_saved_column_order_roundtrips_unknown_options_and_refuses_stale() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let path = root.join("Board.base");
    let yaml="custom: {keep: yes}\nviews:\n  - type: kanban\n    name: Board\n    columnWidth: 300\n    groupBy: {property: note.status, direction: ASC}\n";
    tokio::fs::write(&path, yaml).await.unwrap();
    let params = json!({"source":{"path":"Board.base"},"view":"Board","ancestor_hash":crucible_core::note_edit::disk_hash(yaml),"group_order":["doing","todo"]});
    assert!(applied(
        &run(BaseOperation::ReorderGroups, root, params.clone()).await
    ));
    let base = load(
        root,
        &Source::Path {
            path: "Board.base".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        base.views[0].group_order.as_ref().unwrap(),
        &vec![json!("doing"), json!("todo")]
    );
    assert!(base.extra.contains_key("custom"));
    assert_eq!(base.views[0].extra["columnWidth"].as_i64(), Some(300));
    assert_eq!(
        run(BaseOperation::ReorderGroups, root, params.clone()).await["status"],
        "stale"
    );
}

#[tokio::test]
async fn bases_named_embed_resolves_a_subfolder_source() {
    let dir = TempDir::new().unwrap();
    tokio::fs::create_dir(dir.path().join("boards"))
        .await
        .unwrap();
    tokio::fs::write(
        dir.path().join("boards/Tasks.base"),
        "views: [{type: table, name: Tasks}]",
    )
    .await
    .unwrap();
    let mut request = request("");
    request.source = Source::Path {
        path: "Tasks.base".into(),
    };
    assert!(query(dir.path(), &request).await.is_ok());
}

#[test]
#[ignore = "requires: playwright harness — Obsidian 1.14.2 running a disposable conformance vault with CLI and CDP on port 19222"]
fn bases_regenerate_live_obsidian_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = std::process::Command::new("node")
        .arg("scripts/capture-bases-conformance.mjs")
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn bases_inline_column_order_preserves_host_bytes_and_checks_host_ancestor() {
    let dir = TempDir::new().unwrap();
    let yaml = "views:\n  - type: kanban\n    name: Board\n    groupBy: {property: note.status, direction: ASC}\n";
    let text = format!(
        "---\r\ntitle: Host\r\n---\r\nBefore 🦀\r\n\r\n```base\r\n{}```\r\n\r\nAfter\r\n",
        yaml.replace('\n', "\r\n")
    );
    let path = dir.path().join("host.md");
    std::fs::write(&path, &text).unwrap();
    let mut request = request(yaml);
    request.host = Some("host.md".into());
    let result = query(dir.path(), &request).await.unwrap();
    assert_eq!(
        result.source_hash.as_deref(),
        Some(crucible_core::note_edit::disk_hash(&text).as_str())
    );
    let params = json!({"source":{"yaml":yaml},"this":"host.md","view":"Board","group_order":["done","todo"],"ancestor_hash":result.source_hash});
    assert!(applied(
        &run(BaseOperation::ReorderGroups, dir.path(), params.clone()).await
    ));
    let changed = std::fs::read_to_string(&path).unwrap();
    assert!(changed.starts_with("---\r\ntitle: Host\r\n---\r\nBefore 🦀\r\n\r\n```base\r\n"));
    assert!(changed.ends_with("```\r\n\r\nAfter\r\n"));
    assert!(changed.contains("groupOrder:"));
    assert_eq!(
        run(BaseOperation::ReorderGroups, dir.path(), params.clone()).await["status"],
        "stale"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), changed);
}

/// Rows compare in the CLI capture's order. A grouped view's capture lists
/// its rows group by group, so the concatenated groups must give that order.
#[tokio::test]
async fn bases_matches_captured_obsidian_queries() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-queries.json"
    ))
    .unwrap();
    let dir = super::engine_tests::kiln(&corpus["files"]);
    let mut failures = vec![];
    for case in corpus["cases"].as_array().unwrap() {
        // CLI evaluation supplies no embedding host. Native saved views separately
        // test the documented main-pane `this` context.
        let yaml = serde_yaml::to_string(&case["base"]).unwrap();
        let result = match query(dir.path(), &request(&yaml)).await {
            Ok(result) => result,
            Err(e) => {
                failures.push(format!("{}: {e:#}", case["id"]));
                continue;
            }
        };
        let expected: serde_json::Value =
            serde_json::from_str(case["formats"]["json"]["output"].as_str().unwrap()).unwrap();
        let shown = |rows: &[Row]| {
            rows.iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    map.insert("path".into(), json!(row.path));
                    for column in &result.columns {
                        let text = row
                            .values
                            .get(&column.property)
                            .filter(|v| !v.empty())
                            .map(BaseValue::text);
                        map.insert(column.display_name.clone(), json!(text));
                    }
                    serde_json::Value::Object(map)
                })
                .collect::<Vec<_>>()
        };
        if json!(shown(&result.rows)) != expected {
            failures.push(format!(
                "{}: expected {expected}, got {}",
                case["id"],
                json!(shown(&result.rows))
            ));
        }
        if result.group_property.is_some() {
            let grouped: Vec<Row> = result.groups.iter().flat_map(|g| g.rows.clone()).collect();
            if json!(shown(&grouped)) != expected {
                failures.push(format!(
                    "{} groups: expected {expected}, got {}",
                    case["id"],
                    json!(shown(&grouped))
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn bases_creation_matches_obsidian_frontmatter_and_body() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-creation.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("created")).unwrap();
        for (path, text) in corpus["files"].as_object().unwrap() {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let params = json!({"source":{"yaml":case["base"].to_string()},"view":"Case","name":format!("Oracle-{}",case["id"].as_str().unwrap()),"content":"Body\n"});
        let result = run(BaseOperation::CreateEntry, dir.path(), params.clone()).await;
        let actual =
            std::fs::read_to_string(dir.path().join(result["path"].as_str().unwrap())).unwrap();
        let expected = case["bytes"].as_str().unwrap();
        let parser = crucible_core::parser::CrucibleParser::new();
        let a = parser
            .parse_content(&actual, Path::new("a.md"))
            .await
            .unwrap();
        let b = parser
            .parse_content(expected, Path::new("a.md"))
            .await
            .unwrap();
        if a.frontmatter.as_ref().map(|f| f.properties())
            != b.frontmatter.as_ref().map(|f| f.properties())
            || actual[a.body_offset..] != expected[b.body_offset..]
            || result["path"] != case["path"]
        {
            failures.push(format!(
                "{}: expected {expected:?}, got {actual:?}",
                case["id"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every captured group must exist with its value, and no other group may appear.
#[tokio::test]
async fn bases_summaries_match_native_obsidian_groups_and_empty_sets() {
    let queries: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/queries.json"
    ))
    .unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-summaries.json"
    ))
    .unwrap();
    let dir = super::engine_tests::kiln(&queries["files"]);
    let mut failures = vec![];
    for case in corpus["summaries"].as_array().unwrap() {
        let name = &case["name"];
        let mut base = corpus["base"].clone();
        let property = case["property"].as_str().unwrap();
        base["views"][0]["summaries"] = json!({property:name});
        let result = query(dir.path(), &request(&base.to_string()))
            .await
            .unwrap();
        let actual = result.summaries[property].text();
        if actual != case["all"].as_str().unwrap() {
            failures.push(format!("{name}: {actual} != {}", case["all"]));
        }
        let actual: Vec<(String, String)> = result
            .groups
            .iter()
            .map(|g| (g.value.text(), g.summaries[property].text()))
            .collect();
        let expected: Vec<(String, String)> = case["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                (
                    g["group"].as_str().unwrap().to_owned(),
                    g["value"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        if actual != expected {
            failures.push(format!("{name} groups: {actual:?} != {expected:?}"));
        }
        base["filters"] = json!("false");
        let result = query(dir.path(), &request(&base.to_string()))
            .await
            .unwrap();
        if result.summaries[property].text() != case["empty"].as_str().unwrap() {
            failures.push(format!(
                "{name} empty: {} != {}",
                result.summaries[property].text(),
                case["empty"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn bases_property_moves_match_native_obsidian_file_outcomes() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-moves.json"
    ))
    .unwrap();
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
    std::fs::write(
        dir.path().join(".obsidian/types.json"),
        json!({"types":corpus["property_types"]}).to_string(),
    )
    .unwrap();
    let path = dir.path().join("item.md");
    let parser = crucible_core::parser::CrucibleParser::new();
    for case in corpus["cases"].as_array().unwrap() {
        let before = case["before"].as_str().unwrap();
        std::fs::write(&path, before).unwrap();
        let result = run(BaseOperation::SetProperty, dir.path(), json!({"path":"item.md","key":case["key"],"value":case["value"],"delete":case["value"].is_null(),"ancestor_hash":crucible_core::note_edit::disk_hash(before)})).await;
        assert!(applied(&result), "{} {result}", case["id"]);
        let actual = std::fs::read_to_string(&path).unwrap();
        let expected = case["after"].as_str().unwrap();
        let a = parser.parse_content(&actual, &path).await.unwrap();
        let b = parser.parse_content(expected, &path).await.unwrap();
        assert_eq!(
            a.frontmatter.as_ref().map(|f| f.properties()),
            b.frontmatter.as_ref().map(|f| f.properties()),
            "{}",
            case["id"]
        );
        assert_eq!(
            &actual[a.body_offset..],
            &expected[b.body_offset..],
            "{}",
            case["id"]
        );
    }
}
