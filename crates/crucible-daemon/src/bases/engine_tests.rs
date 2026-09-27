//! Evaluator and query-engine behavior: the Obsidian captures, JavaScript
//! semantics, per-cell errors and robustness against individual bad files.
use super::*;
use crucible_core::bases::{Expr, Formula, Function};
use crucible_core::test_support::create_kiln_with_files;
use eval::Context as EvalContext;
use serde_json::json;
use strum::IntoEnumIterator;
use tempfile::TempDir;

fn request(yaml: &str) -> Query {
    Query {
        kiln: "test".into(),
        source: Source::Inline { yaml: yaml.into() },
        view: None,
        host: None,
    }
}
/// A kiln from a JSON object of path → text.
pub(super) fn kiln(files: &serde_json::Value) -> TempDir {
    let files: Vec<(&str, &str)> = files
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str().unwrap()))
        .collect();
    create_kiln_with_files(&files).unwrap()
}
/// The capture's seed note: `notes/a.md` with `status: todo`,
/// `tags: [work/deep]` and a link to itself.
fn seed() -> Vec<Entry> {
    vec![Entry {
        path: "notes/a.md".into(),
        title: "a".into(),
        tags: vec!["work/deep".into()],
        properties: BTreeMap::from([
            ("status".into(), BaseValue::String("todo".into())),
            (
                "tags".into(),
                BaseValue::List(vec![BaseValue::String("#work/deep".into())]),
            ),
        ]),
        links: vec!["notes/a".into()],
        ..Entry::default()
    }]
}
/// One formula cell on the first entry, as the captures evaluate each case.
fn cell(entries: &[Entry], formulas: &[(&str, &str)], property: &str) -> BaseValue {
    let formulas: BTreeMap<String, Formula> = formulas
        .iter()
        .map(|(name, source)| ((*name).to_owned(), Formula::new(source)))
        .collect();
    let now = chrono::Utc::now().timestamp_millis();
    let ctx = EvalContext::new(entries, None, &formulas, now);
    Eval::new(&ctx, &entries[0]).cell(property)
}
fn formula_cell(expression: &str) -> BaseValue {
    cell(&seed(), &[("result", expression)], "formula.result")
}
/// The capture timezone; local date parsing reads TZ, and nextest isolates each test.
fn capture_timezone() -> crucible_core::test_support::EnvVarGuard {
    crucible_core::test_support::EnvVarGuard::set("TZ", "America/Chicago".into())
}

fn expression_corpus() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-expressions.json"
    ))
    .unwrap()
}

#[test]
fn bases_matches_captured_obsidian_expressions() {
    let corpus = expression_corpus();
    assert_eq!(corpus["timezone"], "America/Chicago");
    let _timezone = capture_timezone();
    let mut failures = vec![];
    for case in corpus["cases"].as_array().unwrap() {
        let actual = formula_cell(case["expression"].as_str().unwrap());
        let expected = &case["output"][0]["result"];
        let pass = match expected.as_str() {
            _ if case.get("error").is_some() => matches!(actual, BaseValue::Error(_)),
            Some(text) if text.starts_with("Error: ") => matches!(actual, BaseValue::Error(_)),
            // Obsidian prints null for an empty cell; an error is never empty.
            None => actual.empty() && !matches!(actual, BaseValue::Error(_)),
            Some(text) => !matches!(actual, BaseValue::Error(_)) && actual.text() == text,
        };
        if !pass {
            failures.push(format!(
                "{}: expected {expected}, got {actual:?}",
                case["id"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every declared function is called by a captured Obsidian case, so the
/// capture comparison above exercises each one.
#[test]
fn bases_every_declared_function_has_a_captured_example() {
    fn calls(e: &Expr, out: &mut Vec<Function>) {
        match e {
            Expr::Literal(_) | Expr::Name(_) => {}
            Expr::List(xs) => xs.iter().for_each(|x| calls(x, out)),
            Expr::Object(xs) => xs.iter().for_each(|(_, x)| calls(x, out)),
            Expr::Get(a, b) | Expr::Binary(_, a, b) => {
                calls(a, out);
                calls(b, out);
            }
            Expr::Unary(_, a) => calls(a, out),
            Expr::Call {
                function,
                receiver,
                args,
            } => {
                out.push(*function);
                receiver.iter().for_each(|x| calls(x, out));
                args.iter().for_each(|x| calls(x, out));
            }
        }
    }
    let corpus = expression_corpus();
    let cases = corpus["cases"].as_array().unwrap();
    for function in Function::iter() {
        let id = format!("{function:?}");
        let case = cases
            .iter()
            .find(|c| c["id"] == id.as_str())
            .unwrap_or_else(|| panic!("{id} needs a captured case"));
        let mut found = vec![];
        calls(
            &Expr::parse(case["expression"].as_str().unwrap()).unwrap(),
            &mut found,
        );
        assert!(found.contains(&function), "case {id} does not call {id}");
    }
}

/// JavaScript semantics recorded by `scripts/capture-bases-js-reference.mjs`.
#[test]
fn bases_matches_javascript_reference_semantics() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/js-reference.json"
    ))
    .unwrap();
    fn plain(v: &BaseValue) -> serde_json::Value {
        match v {
            BaseValue::Number(n) if !n.is_finite() => json!(v.text()),
            BaseValue::Number(n) => json!(n),
            BaseValue::Boolean(b) => json!(b),
            BaseValue::String(s) => json!(s),
            BaseValue::List(xs) => xs.iter().map(plain).collect(),
            v => json!(format!("{v:?}")),
        }
    }
    let mut failures = vec![];
    for case in corpus["cases"].as_array().unwrap() {
        let actual = plain(&formula_cell(case["expression"].as_str().unwrap()));
        let expected = &case["expected"];
        let same = match (expected.as_f64(), actual.as_f64()) {
            (Some(a), Some(b)) => a == b,
            _ => *expected == actual,
        };
        if !same {
            failures.push(format!("{}: expected {expected}, got {actual}", case["id"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn bases_expression_examples() {
    let _timezone = capture_timezone();
    let formulas = [("a", "formula.b"), ("b", "formula.a")];
    for (source, expected) in [
        ("1 + 2 * 3", BaseValue::Number(7.0)),
        (
            "[1,2,3,4].filter(value > 2).map(value * 2).reduce(acc + value, 0)",
            BaseValue::Number(14.0),
        ),
        (
            "if(false, number(duration('1M')), 'ok')",
            BaseValue::String("ok".into()),
        ),
        ("/abc/i.matches('ABC')", BaseValue::Boolean(true)),
        (
            "(date('2024-12-01') + '1M').format('YYYY-MM-DD')",
            BaseValue::String("2025-01-01".into()),
        ),
        // Obsidian's datetime property text, and naive forms, read as local time.
        (
            "date('2024-01-15T14:30').format('YYYY-MM-DD HH:mm')",
            BaseValue::String("2024-01-15 14:30".into()),
        ),
        (
            "date('2024-01-01T10:00:00').format('HH:mm:ss')",
            BaseValue::String("10:00:00".into()),
        ),
        (
            "date('2024-01-01 12:30').format('HH:mm')",
            BaseValue::String("12:30".into()),
        ),
        // `date()` reads what a date value prints.
        (
            "date(date('2024-01-01 12:30').toString()).format('HH:mm')",
            BaseValue::String("12:30".into()),
        ),
        ("date(now()) == now()", BaseValue::Boolean(true)),
        ("date(file.mtime) == file.mtime", BaseValue::Boolean(true)),
        ("max()", BaseValue::Null),
        ("min()", BaseValue::Null),
        ("'😀a'.slice(1, 3)", BaseValue::String("\u{FFFD}a".into())),
        (
            "date('2024-01-01').format('\\\\YYYY [YYYY]')",
            BaseValue::String("YYYY YYYY".into()),
        ),
    ] {
        let mut entries = seed();
        entries[0].mtime = Some(1_700_000_000_000);
        let mut all: Vec<(&str, &str)> = formulas.to_vec();
        all.push(("x", source));
        assert_eq!(cell(&entries, &all, "formula.x"), expected, "{source}");
    }
    assert!(matches!(
        cell(&seed(), &formulas, "formula.a"),
        BaseValue::Error(e) if e.contains("Circular")
    ));
}

/// A left-associative chain parses and evaluates; a tree that is too deep
/// fails to parse and never reaches evaluation.
#[test]
fn bases_parse_and_evaluation_agree_on_depth() {
    let chain = vec!["1"; 300].join(" + ");
    assert_eq!(formula_cell(&chain), BaseValue::Number(300.0));
    let deepest = format!(
        "{}1{}",
        "[".repeat(crucible_core::bases::MAX_DEPTH - 1),
        "]".repeat(crucible_core::bases::MAX_DEPTH - 1)
    );
    assert!(!matches!(formula_cell(&deepest), BaseValue::Error(_)));
    let deeper = cell(
        &seed(),
        &[("deep", &deepest), ("uses", "[formula.deep]")],
        "formula.uses",
    );
    assert!(!matches!(deeper, BaseValue::Error(_)), "{deeper:?}");
    let too_deep = format!("[{deepest}]");
    assert!(Expr::parse(&too_deep).is_err());
}

/// Each formula runs once per row, so a doubling chain stays linear.
#[test]
fn bases_formulas_run_once_per_row() {
    let mut formulas = vec![("f0".to_owned(), "1".to_owned())];
    for i in 1..=40 {
        formulas.push((
            format!("f{i}"),
            format!("formula.f{0} + formula.f{0}", i - 1),
        ));
    }
    let formulas: Vec<(&str, &str)> = formulas
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        cell(&seed(), &formulas, "formula.f40"),
        BaseValue::Number(2f64.powi(40))
    );
}

#[test]
fn bases_file_fields_use_the_row_itself_when_names_differ_only_in_case() {
    let entry = |path: &str, status: &str| Entry {
        path: path.into(),
        title: path.trim_end_matches(".md").into(),
        properties: BTreeMap::from([("status".into(), BaseValue::String(status.into()))]),
        ..Entry::default()
    };
    let entries = vec![entry("a.md", "lower"), entry("A.md", "upper")];
    let formulas = [("s", "file.properties.status + file.path + this.file.path")];
    for (i, expected) in [(0, "lowera.mdA.md"), (1, "upperA.mdA.md")] {
        let formulas: BTreeMap<String, Formula> = formulas
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Formula::new(v)))
            .collect();
        let ctx = EvalContext::new(&entries, Some(&entries[1]), &formulas, 0);
        assert_eq!(
            Eval::new(&ctx, &entries[i]).cell("formula.s"),
            BaseValue::String(expected.into())
        );
    }
}

#[tokio::test]
async fn bases_one_bad_cell_or_file_does_not_stop_the_query() {
    let dir = create_kiln_with_files(&[
        ("a.md", "---\nscore: 1\ndue: 2024-01-15T14:30\n---\n"),
        ("b.md", "---\nscore: '1M'\ndue: not a date\n---\n"),
        ("c.md", "---\nscore: 3\n---\n"),
        (".obsidian/types.json", r#"{"types":{"due":"datetime"}}"#),
    ])
    .unwrap();
    std::fs::write(dir.path().join("latin1.md"), [0xE9, b'\n']).unwrap();
    let yaml = "filters: 'file.ext == \"md\"'\nformulas:\n  n: 'score * 2'\nviews:\n  - type: table\n    name: T\n    order: [file.name, formula.n, note.due]\n    sort: [{property: formula.n, direction: DESC}]\n    groupBy: {property: formula.n, direction: ASC}\n    summaries: {formula.n: Sum}\n";
    let result = query(dir.path(), &request(yaml)).await.unwrap();
    let paths: Vec<_> = result.rows.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["c.md", "a.md", "b.md"], "the error sorts as null");
    let b = &result.rows[2];
    assert!(
        matches!(&b.values["formula.n"], BaseValue::Error(e) if e.contains("Expected number")),
        "{:?}",
        b.values
    );
    assert!(b.values["formula.n"].text().starts_with("Error: "));
    assert_eq!(
        b.values["note.due"],
        BaseValue::String("not a date".into()),
        "unparseable typed text stays text"
    );
    assert!(matches!(
        result.rows[1].values["note.due"],
        BaseValue::Date(_)
    ));
    assert_eq!(result.groups.last().unwrap().value, BaseValue::Null);
    assert_eq!(result.summaries["formula.n"], BaseValue::Number(8.0));
}

#[tokio::test]
async fn bases_summaries_preserve_dates_and_reject_unknown_names() {
    let dir = create_kiln_with_files(&[
        ("a.md", "---\nday: 2024-01-02\n---\n"),
        ("b.md", "---\nday: 2024-03-04\n---\n"),
        (".obsidian/types.json", r#"{"types":{"day":"date"}}"#),
    ])
    .unwrap();
    let yaml = "views: [{type: table, name: T, summaries: {note.day: Latest}}]";
    let result = query(dir.path(), &request(yaml)).await.unwrap();
    assert_eq!(
        result.summaries["note.day"],
        BaseValue::DateOnly(chrono::NaiveDate::from_ymd_opt(2024, 3, 4).unwrap())
    );
    let unknown = "filters: 'false'\nviews: [{type: table, name: T, summaries: {note.day: Bogus}}]";
    assert!(query(dir.path(), &request(unknown)).await.is_err());
}

#[test]
fn bases_summary_kinds_match_the_captured_obsidian_set() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets/fixtures/bases/obsidian-1.14.2-summaries.json"
    ))
    .unwrap();
    let mut captured: Vec<&str> = corpus["summaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    let mut declared: Vec<&str> = crucible_core::bases::SummaryKind::iter()
        .map(|k| k.name())
        .collect();
    captured.sort_unstable();
    captured.dedup();
    declared.sort_unstable();
    assert_eq!(captured, declared);
}

#[tokio::test]
async fn bases_types_json_list_types_make_lists() {
    let dir = create_kiln_with_files(&[
        (
            "a.md",
            "---\nlabels: one\naliases: Alpha\ntags: solo\n---\n",
        ),
        (
            ".obsidian/types.json",
            r#"{"types":{"labels":"multitext"}}"#,
        ),
    ])
    .unwrap();
    let yaml =
        "views: [{type: table, name: T, order: [note.labels, note.aliases, note.tags, file.tags]}]";
    let row = &query(dir.path(), &request(yaml)).await.unwrap().rows[0];
    let list =
        |xs: &[&str]| BaseValue::List(xs.iter().map(|x| BaseValue::String((*x).into())).collect());
    assert_eq!(row.values["note.labels"], list(&["one"]));
    assert_eq!(row.values["note.aliases"], list(&["Alpha"]));
    assert_eq!(row.values["note.tags"], list(&["#solo"]));
    assert_eq!(row.values["file.tags"], list(&["solo"]));
}

#[tokio::test]
async fn bases_links_resolve_partial_paths_and_backlinks() {
    let dir = create_kiln_with_files(&[
        ("a/sub/note.md", "target"),
        ("x.md", "[[sub/note]]"),
        ("y.md", "[[note]] [[SUB/NOTE]]"),
        ("root.md", ""),
    ])
    .unwrap();
    let yaml = "formulas:\n  back: 'file.backlinks.length'\n  links: 'file.hasLink(link(\"a/sub/note\"))'\n  folder: 'file.folder'\nviews: [{type: table, name: T, order: [formula.back, formula.links, formula.folder]}]";
    let result = query(dir.path(), &request(yaml)).await.unwrap();
    let row = |p: &str| &result.rows.iter().find(|r| r.path == p).unwrap().values;
    assert_eq!(row("a/sub/note.md")["formula.back"], BaseValue::Number(2.0));
    assert_eq!(row("x.md")["formula.links"], BaseValue::Boolean(true));
    assert_eq!(row("y.md")["formula.links"], BaseValue::Boolean(true));
    assert_eq!(
        row("root.md")["formula.folder"],
        BaseValue::String("/".into())
    );
    assert_eq!(
        row("a/sub/note.md")["formula.folder"],
        BaseValue::String("a/sub".into())
    );
}

/// Only returned rows hash their file, and an unreadable file or folder
/// drops out with a warning instead of failing the query.
#[cfg(unix)]
#[tokio::test]
async fn bases_unreadable_paths_and_escaping_symlinks_are_skipped() {
    use std::os::unix::fs::PermissionsExt;
    let dir = create_kiln_with_files(&[("a.md", "x"), ("locked/b.md", "y"), ("secret.png", "z")])
        .unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("out.md"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.path().join("out.md"), dir.path().join("link.md")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linkdir")).unwrap();
    let lock = |path: &str, mode| {
        std::fs::set_permissions(dir.path().join(path), std::fs::Permissions::from_mode(mode))
            .unwrap()
    };
    lock("locked", 0o000);
    lock("secret.png", 0o000);
    let md = query(
        dir.path(),
        &request("filters: 'file.ext == \"md\"'\nviews: [{type: table, name: T}]"),
    )
    .await;
    let all = query(dir.path(), &request("views: [{type: table, name: T}]")).await;
    lock("locked", 0o755);
    lock("secret.png", 0o644);
    let md = md.unwrap();
    assert_eq!(md.rows.len(), 1);
    assert_eq!(md.rows[0].path, "a.md");
    let all = all.unwrap();
    let secret = all.rows.iter().find(|r| r.path == "secret.png").unwrap();
    assert_eq!(
        secret.ancestor_hash, "",
        "an unreadable returned file has no hash"
    );
    assert!(all.rows.iter().all(|r| !r.path.starts_with("link")));
}

#[tokio::test]
async fn bases_rows_and_groups_carry_the_move_contract() {
    let dir = create_kiln_with_files(&[
        (
            "a.md",
            "---\nstatus: todo\ndue: 2024-01-15T14:30\nday: 2024-02-03\nref: '[[b|B]]'\n---\n",
        ),
        ("b.md", "---\nstatus: [x, y]\n---\n"),
        ("sub/c.md", ""),
        ("pic.png", "p"),
        (
            ".obsidian/types.json",
            r#"{"types":{"due":"datetime","day":"date"}}"#,
        ),
    ])
    .unwrap();
    let groups = |property: &str| {
        let yaml = format!(
            "views: [{{type: kanban, name: K, groupBy: {{property: {property}, direction: ASC}}}}]"
        );
        let dir = dir.path().to_owned();
        async move { query(&dir, &request(&yaml)).await.unwrap() }
    };
    let status = groups("note.status").await;
    let movable: BTreeMap<_, _> = status
        .rows
        .iter()
        .map(|r| (r.path.as_str(), r.movable))
        .collect();
    assert!(movable["a.md"]);
    assert!(!movable["pic.png"], "only notes carry frontmatter");
    let writes: Vec<_> = status
        .groups
        .iter()
        .map(|g| g.write_value.clone())
        .collect();
    assert_eq!(writes, [json!("todo"), json!(["x", "y"]), json!(null)]);
    for (property, expected) in [
        ("note.due", json!("2024-01-15T14:30")),
        ("note.day", json!("2024-02-03")),
        ("note.ref", json!("[[b|B]]")),
    ] {
        let result = groups(property).await;
        assert_eq!(result.groups[0].write_value, expected, "{property}");
    }
    let folders = groups("file.folder").await;
    assert!(folders.rows.iter().all(|r| r.movable));
    let root = folders
        .groups
        .iter()
        .find(|g| g.value == BaseValue::String("/".into()))
        .unwrap();
    assert_eq!(root.write_value, json!(""));
    assert!(groups("formula.x").await.rows.iter().all(|r| !r.movable));
    let table = query(dir.path(), &request("views: [{type: table, name: T}]"))
        .await
        .unwrap();
    assert!(table.rows.iter().all(|r| !r.movable));
    assert_eq!(table.view_type, crucible_core::bases::ViewType::Table);
}
