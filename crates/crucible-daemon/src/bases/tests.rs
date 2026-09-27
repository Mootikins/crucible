use super::*;
use serde_json::json;
use tempfile::TempDir;

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
    assert_eq!(result.rows[0].values["formula.double"], Value::Number(4.0));
    assert_eq!(result.groups.len(), 2);
    assert_eq!(result.summaries["formula.double"], Value::Number(4.0));
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
    let result = write::set_property(root, &params).await.unwrap();
    assert_eq!(result["ok"], true);
    let changed = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(changed.ends_with("# Body\r\n\r\n"));
    assert!(changed.contains("- done"));
    assert!(changed.contains("other: 42"));
    assert_eq!(
        write::set_property(root, &params).await.unwrap()["ok"],
        false
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
    let a = write::create_entry(root, &p).await.unwrap();
    let b = write::create_entry(root, &p).await.unwrap();
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
#[test]
fn bases_expression_examples() {
    let entry = Entry::default();
    let formulas = BTreeMap::from([
        ("a".into(), "formula.b".into()),
        ("b".into(), "formula.a".into()),
    ]);
    let mut eval = Eval::new(&[], &entry, None, &formulas, 0);
    for (source, expected) in [
        ("1 + 2 * 3", Value::Number(7.0)),
        (
            "[1,2,3,4].filter(value > 2).map(value * 2).reduce(acc + value, 0)",
            Value::Number(14.0),
        ),
        ("if(false, number('bad'), 'ok')", Value::String("ok".into())),
        ("'a:b:c'.replace(':', '-')", Value::String("a-b-c".into())),
        ("/abc/i.matches('ABC')", Value::Boolean(true)),
        (
            "(date('2024-12-01') + '1M').format('YYYY-MM-DD')",
            Value::String("2025-01-01".into()),
        ),
    ] {
        assert_eq!(
            eval.eval(&Expr::parse(source).unwrap()).unwrap(),
            expected,
            "{source}"
        );
    }
    assert!(eval.eval(&Expr::parse("formula.a").unwrap()).is_err());
}

#[test]
fn bases_every_declared_function_has_a_working_example() {
    use eval::Function::*;
    use strum::IntoEnumIterator;
    let entry = Entry {
        path: "notes/a.md".into(),
        tags: vec!["work/deep".into()],
        properties: BTreeMap::from([("status".into(), Value::String("todo".into()))]),
        links: vec!["notes/a.md".into()],
        ..Entry::default()
    };
    let entries = [entry.clone()];
    let formulas = BTreeMap::new();
    let mut eval = Eval::new(&entries, &entries[0], None, &formulas, 0);
    for function in eval::Function::iter() {
        // Exhaustive on purpose: a newly advertised function needs an executable example.
        let expression = match function {
            Date => "date('2025-01-01').year == 2025",
            Duration => "duration('1h') == duration('60m')",
            File => "file('notes/a.md').path == 'notes/a.md'",
            Link => "link('notes/a.md') == file",
            List => "list('one').length == 1",
            Image => "image('a.png').isType('image')",
            Icon => "icon('plus').isType('icon')",
            Html => "html('<b>x</b>').isType('html')",
            EscapeHTML => "escapeHTML('<a>') == '&lt;a&gt;'",
            If => "if(true, 1, 0) == 1",
            Max => "max(1,4,2) == 4",
            Min => "min(1,4,2) == 1",
            Now => "now().isType('date')",
            Today => "today().hour == 0",
            Number => "number('4.2') == 4.2",
            Random => "random() >= 0 && random() < 1",
            IsTruthy => "1.isTruthy()",
            IsType => "true.isType('boolean')",
            ToString => "123.toString() == '123'",
            Format => "date('2025-01-01').format('YYYY-MM-DD') == '2025-01-01'",
            Time => "date('2025-01-01 12:34:56').time() == '12:34:56'",
            Relative => "now().relative().contains('ago')",
            IsEmpty => "null.isEmpty()",
            Contains => "[1,2].contains(2)",
            ContainsAll => "'hello'.containsAll('h','e')",
            ContainsAny => "[1,2].containsAny(9,2)",
            StartsWith => "'hello'.startsWith('he')",
            EndsWith => "'hello'.endsWith('lo')",
            Lower => "'ABC'.lower() == 'abc'",
            Title => "'hello world'.title() == 'Hello World'",
            Trim => "' hi '.trim() == 'hi'",
            Replace => "'a:b:c'.replace(/:/g,'-') == 'a-b-c'",
            Repeat => "'ab'.repeat(2) == 'abab'",
            Reverse => "[1,2].reverse()[0] == 2",
            Slice => "'hello'.slice(1,-1) == 'ell'",
            Split => "'a,b,c'.split(',',2).length == 2",
            Abs => "(-5).abs() == 5",
            Ceil => "2.1.ceil() == 3",
            Floor => "2.9.floor() == 2",
            Round => "(-2.5).round() == -2",
            ToFixed => "3.14159.toFixed(2) == '3.14'",
            Filter => "[1,2,3].filter(value > 1).length == 2",
            Map => "[1,2].map(value + index)[1] == 3",
            Reduce => "[1,2,3].reduce(acc + value,0) == 6",
            Flat => "[1,[2,3]].flat().length == 3",
            Join => "[1,2].join('-') == '1-2'",
            Sort => "[10,2,1].sort()[0] == 1",
            Unique => "[1,1,2].unique().length == 2",
            AsFile => "link('notes/a.md').asFile() == file",
            LinksTo => "link('notes/a.md').linksTo(file)",
            AsLink => "file.asLink('A') == file",
            HasLink => "file.hasLink('notes/a.md')",
            HasProperty => "file.hasProperty('status')",
            HasTag => "file.hasTag('work')",
            InFolder => "file.inFolder('notes')",
            Keys => "{a:1,b:2}.keys().length == 2",
            Values => "{a:1,b:2}.values().contains(2)",
            Matches => "/abc/i.matches('ABC')",
            Mean => "[1,2,3].mean() == 2",
        };
        assert_eq!(
            eval.eval(&Expr::parse(expression).unwrap()).unwrap(),
            Value::Boolean(true),
            "{function:?}: {expression}"
        );
    }
}

#[tokio::test]
async fn bases_saved_column_order_roundtrips_unknown_options_and_refuses_stale() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let path = root.join("Board.base");
    let yaml="custom: {keep: yes}\nviews:\n  - type: kanban\n    name: Board\n    columnWidth: 300\n    groupBy: {property: note.status, direction: ASC}\n";
    tokio::fs::write(&path, yaml).await.unwrap();
    let params = json!({"source":{"path":"Board.base"},"view":"Board","ancestor_hash":crucible_core::note_edit::disk_hash(yaml),"group_order":["doing","todo"]});
    assert_eq!(
        write::reorder_groups(root, &params).await.unwrap()["ok"],
        true
    );
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
        write::reorder_groups(root, &params).await.unwrap()["ok"],
        false
    );
}
