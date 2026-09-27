//! Daemon-owned Bases queries. Files remain the source of truth, including attachments.
mod date_format;
mod disposition;
#[cfg(test)]
mod engine_tests;
mod eval;
mod inline;
pub mod operation;
pub(crate) mod plugin_api;
#[cfg(test)]
mod plugin_tests;
mod policy;
mod regexp;
#[cfg(test)]
mod tests;
mod view_options;
mod write;
use anyhow::{ensure, Context, Result};
use chrono::Timelike;
use crucible_core::bases::{
    BaseFile, BaseValue, Direction, Filter, FilterTree, Summary, SummaryKind, View, ViewType,
};
use crucible_core::kiln::KilnFileKind;
use eval::{compare, Context as EvalContext, Eval};
pub use operation::{
    BaseOperation, CreateEntryParams, Failure, ListParams, QueryParams, ReorderGroupsParams,
    SetPropertyParams, ViewsParams, WriteOutcome,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub use view_options::ViewOptions;

#[derive(Debug, Clone, Default)]
pub(super) struct Entry {
    path: String,
    title: String,
    /// The note text hash. Other files hash only when a query returns them.
    ancestor_hash: Option<String>,
    properties: BTreeMap<String, BaseValue>,
    tags: Vec<String>,
    links: Vec<String>,
    embeds: Vec<String>,
    size: u64,
    mtime: Option<i64>,
    ctime: Option<i64>,
}
/// The row of a custom summary formula, which reads only `values`.
static NO_ENTRY: Entry = Entry {
    path: String::new(),
    title: String::new(),
    ancestor_hash: None,
    properties: BTreeMap::new(),
    tags: Vec::new(),
    links: Vec::new(),
    embeds: Vec::new(),
    size: 0,
    mtime: None,
    ctime: None,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged, deny_unknown_fields)]
pub enum Source {
    Path { path: String },
    Inline { yaml: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Query {
    pub kiln: String,
    pub source: Source,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default, rename = "this")]
    pub host: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Column {
    pub property: String,
    pub display_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Row {
    pub path: String,
    pub ancestor_hash: String,
    /// Column property → value. A cell that failed holds [`BaseValue::Error`].
    pub values: BTreeMap<String, BaseValue>,
    /// Whether this row can move between the groups of this view.
    pub movable: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Group {
    pub value: BaseValue,
    /// The `value` of the `base.set_property` call that puts a row into this
    /// group, in the form the note stores. Null means "delete the property".
    pub write_value: serde_json::Value,
    pub rows: Vec<Row>,
    pub summaries: BTreeMap<String, BaseValue>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct QueryResult {
    pub options: ViewOptions,
    pub source_hash: Option<String>,
    pub source_path: Option<String>,
    pub view: String,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub view_type: ViewType,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub groups: Vec<Group>,
    pub summaries: BTreeMap<String, BaseValue>,
    pub group_property: Option<String>,
    pub views: Vec<ViewSummary>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ViewSummary {
    pub name: String,
    #[serde(rename = "type")]
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub kind: ViewType,
}
impl From<&View> for ViewSummary {
    fn from(view: &View) -> Self {
        Self {
            name: view.name.clone(),
            kind: view.kind.clone(),
        }
    }
}

pub(crate) fn contained(root: &Path, path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "Path traversal is not allowed"
    );
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    };
    let canonical = path.canonicalize()?;
    ensure!(canonical.starts_with(root), "Path escapes the kiln");
    ensure!(
        !canonical
            .strip_prefix(root)?
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.')),
        "Hidden kiln files are excluded"
    );
    Ok(canonical)
}
/// Files under the kiln root, without hidden names or excluded directories.
/// The walk skips an unreadable directory with a warning; it does not follow
/// symlinks, so every path it gives stays inside the root.
fn kiln_files(root: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !crucible_core::kiln::is_excluded_name(e.file_name()))
        .filter_map(|item| {
            item.map_err(|error| tracing::warn!(%error, "Bases skipped an unreadable kiln path"))
                .ok()
        })
        .filter(|e| e.file_type().is_file())
}
/// The kiln-relative text of a path inside `root`, with `/` separators.
fn relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/"))
}
fn source_path(root: &Path, raw: &str) -> Result<PathBuf> {
    let direct = root.join(raw);
    if direct.exists()
        || Path::new(raw).is_absolute()
        || Path::new(raw)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return contained(root, raw);
    }
    let candidates = kiln_files(root)
        .filter(|e| KilnFileKind::of(e.path()) == KilnFileKind::Base)
        .filter_map(|e| relative(root, e.path()).ok())
        .collect::<Vec<_>>();
    let resolved = crate::storage::sqlite::link_index::resolve_candidates(
        raw,
        candidates.iter().map(|p| (p.as_str(), "")),
    );
    ensure!(!resolved.is_ambiguous, "Ambiguous base reference: {raw}");
    contained(
        root,
        &resolved
            .resolved_target
            .ok_or_else(|| operation::not_found(format!("Base not found: {raw}")))?,
    )
}
fn checked_source(yaml: String) -> Result<String> {
    ensure!(yaml.len() <= 1_000_000, "Base source exceeds 1 MB");
    Ok(yaml)
}
/// The text of a resolved `.base` file.
async fn read_source(path: &Path) -> Result<String> {
    ensure!(
        KilnFileKind::of(path) == KilnFileKind::Base,
        "Expected a .base file"
    );
    checked_source(tokio::fs::read_to_string(path).await?)
}
async fn source_text(root: &Path, source: &Source) -> Result<String> {
    match source {
        Source::Path { path } => read_source(&source_path(root, path)?).await,
        Source::Inline { yaml } => checked_source(yaml.clone()),
    }
}
async fn load(root: &Path, source: &Source) -> Result<BaseFile> {
    BaseFile::parse(&source_text(root, source).await?)
}
async fn property_types(root: &Path) -> Result<BTreeMap<String, String>> {
    let path = root.join(".obsidian/types.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let path = path.canonicalize()?;
    ensure!(path.starts_with(root), "Property types escape the kiln");
    ensure!(
        tokio::fs::metadata(&path).await?.len() <= 1_000_000,
        "Property types exceed 1 MB"
    );
    let value: serde_json::Value = serde_json::from_slice(&tokio::fs::read(path).await?)?;
    Ok(serde_json::from_value(
        value.get("types").cloned().unwrap_or(serde_json::json!({})),
    )?)
}
/// Applies an Obsidian property type. Text that does not read as the type
/// stays text, as Obsidian shows it.
fn apply_type(value: &mut BaseValue, kind: &str) {
    match (kind, &*value) {
        ("date" | "datetime", BaseValue::String(text)) => {
            let typed = eval::parse_date_text(text).ok().and_then(|(t, _)| {
                if kind == "date" {
                    eval::calendar_date(t).ok().map(BaseValue::DateOnly)
                } else {
                    Some(BaseValue::Date(t))
                }
            });
            if let Some(typed) = typed {
                *value = typed;
            }
        }
        ("multitext" | "tags" | "aliases", v)
            if !matches!(v, BaseValue::List(_) | BaseValue::Null) =>
        {
            *value = BaseValue::List(vec![std::mem::replace(value, BaseValue::Null)]);
        }
        _ => {}
    }
}
/// The types Obsidian gives these properties when `types.json` names none.
const DEFAULT_TYPES: [(&str, &str); 3] = [
    ("tags", "tags"),
    ("aliases", "aliases"),
    ("cssclasses", "multitext"),
];

async fn entries_scoped(
    root: &Path,
    scope: Option<&crate::tools::fs_scope::FsScope>,
) -> Result<Vec<Entry>> {
    let mut types = property_types(root).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "Bases ignored an unreadable .obsidian/types.json");
        BTreeMap::new()
    });
    for (key, kind) in DEFAULT_TYPES {
        types.entry(key.into()).or_insert_with(|| kind.into());
    }
    let parser = crucible_core::parser::CrucibleParser::new();
    let mut out = vec![];
    for item in kiln_files(root) {
        let Ok(rel) = relative(root, item.path()) else {
            continue;
        };
        if scope.is_some_and(|s| s.resolve(&rel).is_err()) {
            continue;
        }
        match entry(&parser, &types, item.path(), rel.clone()).await {
            Ok(e) => out.push(e),
            Err(error) => {
                tracing::warn!(path = %rel, error = %format!("{error:#}"), "Bases skipped a file it cannot read");
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
async fn entry(
    parser: &crucible_core::parser::CrucibleParser,
    types: &BTreeMap<String, String>,
    path: &Path,
    rel: String,
) -> Result<Entry> {
    let meta = tokio::fs::metadata(path).await?;
    let millis = |t: std::io::Result<std::time::SystemTime>| {
        t.ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
    };
    let mut e = Entry {
        path: rel,
        size: meta.len(),
        mtime: millis(meta.modified()),
        ctime: millis(meta.created()),
        ..Entry::default()
    };
    if !crucible_core::kiln::is_note_file(path) {
        return Ok(e);
    }
    let content = String::from_utf8(tokio::fs::read(path).await?).context("Note is not UTF-8")?;
    e.ancestor_hash = Some(crucible_core::note_edit::disk_hash(&content));
    let parsed = parser.parse_content(&content, path).await?;
    e.title = parsed.title();
    e.tags = parsed.all_tags();
    if let Some(fm) = &parsed.frontmatter {
        e.properties = fm
            .properties()
            .iter()
            .map(|(k, v)| (k.clone(), BaseValue::from_json(v)))
            .collect();
    }
    for (key, value) in &mut e.properties {
        if let Some(kind) = types.get(key) {
            apply_type(value, kind);
        }
        // Obsidian shows `tags` values with their `#`.
        if key == "tags" {
            let mark = |v: &mut BaseValue| {
                if let BaseValue::String(s) = v {
                    if !s.starts_with('#') {
                        s.insert(0, '#');
                    }
                }
            };
            match value {
                BaseValue::List(xs) => xs.iter_mut().for_each(mark),
                v => mark(v),
            }
        }
    }
    e.links = parsed.wikilinks.iter().map(|l| l.target.clone()).collect();
    e.embeds = parsed
        .wikilinks
        .iter()
        .filter(|l| l.is_embed)
        .map(|l| l.target.clone())
        .collect();
    e.links.extend(
        parsed
            .inline_links
            .into_iter()
            .filter(|l| !l.url.contains("://") && !l.url.starts_with('#'))
            .map(|l| l.url),
    );
    fn links(v: &BaseValue, out: &mut Vec<String>) {
        match v {
            BaseValue::Link { path, .. } => out.push(path.clone()),
            BaseValue::List(xs) => xs.iter().for_each(|x| links(x, out)),
            BaseValue::Object(o) => o.values().for_each(|x| links(x, out)),
            _ => {}
        }
    }
    for value in e.properties.values() {
        links(value, &mut e.links)
    }
    Ok(e)
}
/// A filter whose evaluation fails excludes the row; it does not stop the query.
fn matches(filter: &Filter, eval: &mut Eval<'_, '_>) -> bool {
    match filter {
        Filter::Expression(e) => eval.test(e.expr()).unwrap_or_else(|error| {
            tracing::debug!(filter = e.source(), %error, "Bases filter failed for a row");
            false
        }),
        Filter::Tree(FilterTree::And(xs)) => xs.iter().all(|x| matches(x, eval)),
        Filter::Tree(FilterTree::Or(xs)) => xs.iter().any(|x| matches(x, eval)),
        Filter::Tree(FilterTree::Not(xs)) => !xs.iter().any(|x| matches(x, eval)),
    }
}
pub async fn query(root: &Path, request: &Query) -> Result<QueryResult> {
    query_scoped(root, request, None).await
}
fn scope_check(scope: Option<&crate::tools::fs_scope::FsScope>, rel: &str) -> Result<()> {
    if let Some(scope) = scope {
        scope.resolve(rel)?;
    }
    Ok(())
}
async fn query_scoped(
    root: &Path,
    request: &Query,
    scope: Option<&crate::tools::fs_scope::FsScope>,
) -> Result<QueryResult> {
    let root = root.canonicalize()?;
    let host = match &request.host {
        Some(host) => {
            let path = contained(&root, host)?;
            let rel = relative(&root, &path)?;
            scope_check(scope, &rel)?;
            Some((path, rel))
        }
        None => None,
    };
    let (yaml, source_rel) = match &request.source {
        Source::Path { path } => {
            let path = source_path(&root, path)?;
            let rel = relative(&root, &path)?;
            scope_check(scope, &rel)?;
            (read_source(&path).await?, Some(rel))
        }
        Source::Inline { yaml } => (checked_source(yaml.clone())?, None),
    };
    let source_hash = match (&request.source, &host) {
        (Source::Path { .. }, _) => Some(crucible_core::note_edit::disk_hash(&yaml)),
        (Source::Inline { yaml }, Some((path, _))) => {
            let text = tokio::fs::read_to_string(path).await?;
            inline::range(path, &text, yaml)
                .await
                .ok()
                .map(|_| crucible_core::note_edit::disk_hash(&text))
        }
        (Source::Inline { .. }, None) => None,
    };
    let base = BaseFile::parse(&yaml)?;
    let view = base.view(request.view.as_deref())?;
    let options = ViewOptions::from(view);
    let all = entries_scoped(&root, scope).await?;
    // A saved base is its own `this` when no host embeds it.
    let host_rel = host.map(|(_, rel)| rel).or(source_rel.clone());
    let host = host_rel
        .as_ref()
        .and_then(|p| all.iter().find(|e| &e.path == p));
    let ctx = EvalContext::new(
        &all,
        host,
        &base.formulas,
        chrono::Utc::now().timestamp_millis(),
    );
    let mut selected = select_rows(&ctx, &all, &base, view, &options);
    selected.sort_by(|a, b| {
        view.sort
            .iter()
            .zip(a.sort.iter().zip(&b.sort))
            .map(|(s, (a, b))| directed(compare(a, b), s.direction))
            .find(|ord| !ord.is_eq())
            .unwrap_or_else(|| a.entry.path.cmp(&b.entry.path))
    });
    if let Some(limit) = view.limit {
        selected.truncate(limit)
    }
    let mut rows = Vec::with_capacity(selected.len());
    for s in &selected {
        rows.push(row(&root, view, s).await);
    }
    let groups = match &view.group_by {
        Some(group_by) => group_rows(&ctx, &base, view, &options, group_by, &selected, &rows),
        None => vec![],
    };
    Ok(QueryResult {
        summaries: summaries(&ctx, &base, view, &rows),
        options,
        source_hash,
        source_path: source_rel,
        view: view.name.clone(),
        view_type: view.kind.clone(),
        columns: columns(&base, view),
        rows,
        groups,
        group_property: view.group_by.as_ref().map(|g| g.property.clone()),
        views: base.views.iter().map(ViewSummary::from).collect(),
        root,
    })
}
fn directed(ord: std::cmp::Ordering, direction: Direction) -> std::cmp::Ordering {
    match direction {
        Direction::ASC => ord,
        Direction::DESC => ord.reverse(),
    }
}
/// The shown columns: the view's `order`, or the file name.
fn view_order(view: &View) -> Vec<String> {
    if view.order.is_empty() {
        vec!["file.name".into()]
    } else {
        view.order.clone()
    }
}
fn columns(base: &BaseFile, view: &View) -> Vec<Column> {
    view_order(view)
        .into_iter()
        .map(|p| Column {
            display_name: base
                .properties
                .get(&p)
                .or_else(|| base.properties.get(p.strip_prefix("note.").unwrap_or(&p)))
                .and_then(|p| p.display_name.clone())
                .unwrap_or_else(|| {
                    p.strip_prefix("note.")
                        .or_else(|| p.strip_prefix("formula."))
                        .map(str::to_owned)
                        .unwrap_or_else(|| p.replace("file.", "file "))
                }),
            property: p,
        })
        .collect()
}
/// One row that passed the filters, with its cells, sort keys and group key.
struct Selected<'a> {
    entry: &'a Entry,
    values: BTreeMap<String, BaseValue>,
    sort: Vec<BaseValue>,
    /// A cell error groups with the empty values.
    group: BaseValue,
}
fn select_rows<'a>(
    ctx: &EvalContext<'a>,
    entries: &'a [Entry],
    base: &BaseFile,
    view: &View,
    options: &ViewOptions,
) -> Vec<Selected<'a>> {
    let order = view_order(view);
    let mut selected = vec![];
    for entry in entries {
        let mut eval = Eval::new(ctx, entry);
        let visible = base
            .filters
            .iter()
            .chain(&view.filters)
            .all(|f| matches(f, &mut eval));
        if !visible {
            continue;
        }
        let values = order
            .iter()
            .chain(options.image.iter())
            .chain(view.summaries.keys())
            .map(|p| (p.clone(), eval.cell(p)))
            .collect();
        let sort = view.sort.iter().map(|s| eval.cell(&s.property)).collect();
        let group = match view.group_by.as_ref().map(|g| eval.cell(&g.property)) {
            None | Some(BaseValue::Error(_)) => BaseValue::Null,
            Some(v) => v,
        };
        selected.push(Selected {
            entry,
            values,
            sort,
            group,
        });
    }
    selected
}
/// Whether a row of this view can move to another group: a folder group moves
/// any file; a note property group moves a markdown note's frontmatter.
fn movable(view: &View, path: &str) -> bool {
    view.group_by.as_ref().is_some_and(|g| {
        g.property == "file.folder"
            || (crucible_core::kiln::is_note_file(Path::new(path))
                && !g.property.starts_with("file.")
                && !g.property.starts_with("formula."))
    })
}
async fn row(root: &Path, view: &View, s: &Selected<'_>) -> Row {
    let ancestor_hash = match &s.entry.ancestor_hash {
        Some(hash) => hash.clone(),
        None => file_hash(&root.join(&s.entry.path))
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(path = %s.entry.path, %error, "Bases could not hash a file");
                String::new()
            }),
    };
    Row {
        path: s.entry.path.clone(),
        ancestor_hash,
        values: s.values.clone(),
        movable: movable(view, &s.entry.path),
    }
}
/// The frontmatter form of a group value: date-only and local date-time
/// text, links as wikilink text, lists as arrays.
fn note_json(v: &BaseValue) -> serde_json::Value {
    use serde_json::json;
    match v {
        BaseValue::Null | BaseValue::Error(_) => serde_json::Value::Null,
        BaseValue::Boolean(b) => json!(b),
        BaseValue::Number(n) => json!(n),
        BaseValue::DateOnly(d) => json!(d.to_string()),
        BaseValue::Date(t) | BaseValue::RelativeDate(t) => eval::local_datetime(*t)
            .map(|d| {
                let format = match (d.second(), d.timestamp_subsec_millis()) {
                    (0, 0) => "%Y-%m-%dT%H:%M",
                    (_, 0) => "%Y-%m-%dT%H:%M:%S",
                    _ => "%Y-%m-%dT%H:%M:%S%.3f",
                };
                json!(d.format(format).to_string())
            })
            .unwrap_or(serde_json::Value::Null),
        BaseValue::List(xs) => serde_json::Value::Array(xs.iter().map(note_json).collect()),
        BaseValue::Object(o) => {
            serde_json::Value::Object(o.iter().map(|(k, v)| (k.clone(), note_json(v))).collect())
        }
        v => json!(v.text()),
    }
}
fn write_value(property: &str, value: &BaseValue) -> serde_json::Value {
    match value {
        // `base.set_property` names the kiln root "", and Obsidian shows it as "/".
        BaseValue::String(folder) if property == "file.folder" && folder == "/" => {
            serde_json::json!("")
        }
        v => note_json(v),
    }
}
fn group_rows(
    ctx: &EvalContext<'_>,
    base: &BaseFile,
    view: &View,
    options: &ViewOptions,
    group_by: &crucible_core::bases::GroupBy,
    selected: &[Selected<'_>],
    rows: &[Row],
) -> Vec<Group> {
    let order: Vec<BaseValue> = view
        .group_order
        .iter()
        .flatten()
        .map(BaseValue::from_json)
        .collect();
    let mut groups: Vec<(BaseValue, Vec<Row>)> = vec![];
    for (s, row) in selected.iter().zip(rows) {
        match groups.iter_mut().find(|(v, _)| *v == s.group) {
            Some((_, members)) => members.push(row.clone()),
            None => groups.push((s.group.clone(), vec![row.clone()])),
        }
    }
    for value in &order {
        if !groups.iter().any(|(v, _)| v == value) {
            groups.push((value.clone(), vec![]))
        }
    }
    let rank = |v: &BaseValue| order.iter().position(|x| x == v).unwrap_or(usize::MAX);
    groups.sort_by(|(a, _), (b, _)| {
        // Empty values come last in either direction.
        a.empty()
            .cmp(&b.empty())
            .then_with(|| {
                if a.empty() {
                    std::cmp::Ordering::Equal
                } else {
                    rank(a).cmp(&rank(b))
                }
            })
            .then_with(|| directed(compare(a, b), group_by.direction))
    });
    if options.hide_empty_groups {
        groups.retain(|(_, rows)| !rows.is_empty())
    }
    groups
        .into_iter()
        .map(|(value, rows)| Group {
            write_value: write_value(&group_by.property, &value),
            summaries: summaries(ctx, base, view, &rows),
            value,
            rows,
        })
        .collect()
}
fn summaries(
    ctx: &EvalContext<'_>,
    base: &BaseFile,
    view: &View,
    rows: &[Row],
) -> BTreeMap<String, BaseValue> {
    view.summaries
        .iter()
        .map(|(key, summary)| {
            let values: Vec<BaseValue> = rows
                .iter()
                .map(|r| match r.values.get(key) {
                    None | Some(BaseValue::Error(_)) => BaseValue::Null,
                    Some(v) => v.clone(),
                })
                .collect();
            let result = match summary {
                Summary::Custom(name) => {
                    let mut eval = Eval::new(ctx, &NO_ENTRY);
                    eval.locals.insert("values".into(), BaseValue::List(values));
                    match base.summaries.get(name) {
                        Some(formula) => eval
                            .eval(formula.expr())
                            .unwrap_or_else(|e| BaseValue::Error(format!("{e:#}"))),
                        None => BaseValue::Error(format!("Unknown summary: {name}")),
                    }
                }
                Summary::Builtin(kind) => builtin_summary(*kind, &values),
            };
            (key.clone(), result)
        })
        .collect()
}
/// A built-in summary. Every built-in gives null for no rows, as Obsidian does.
fn builtin_summary(kind: SummaryKind, values: &[BaseValue]) -> BaseValue {
    use SummaryKind::*;
    if values.is_empty() {
        return BaseValue::Null;
    }
    let count = |predicate: &dyn Fn(&BaseValue) -> bool| {
        BaseValue::Number(values.iter().filter(|v| predicate(v)).count() as f64)
    };
    let mut nums: Vec<f64> = values
        .iter()
        .filter_map(|v| match v {
            BaseValue::Number(n) => Some(*n),
            BaseValue::Date(_) | BaseValue::DateOnly(_) => eval::date_num(v).ok().map(|t| t as f64),
            _ => None,
        })
        .collect();
    nums.sort_by(f64::total_cmp);
    let mean = || nums.iter().sum::<f64>() / nums.len() as f64;
    let hundredths = |n: f64| (n * 100.0).round() / 100.0;
    let numeric = |n: &dyn Fn(&[f64], f64) -> f64| {
        if nums.is_empty() {
            BaseValue::Null
        } else {
            BaseValue::Number(n(&nums, mean()))
        }
    };
    // Earliest and Latest return the date itself, so a date stays a date.
    let dated = |latest: bool| {
        let dates = values.iter().filter(|v| v.is_date());
        let key = |v: &&BaseValue| eval::date_num(v).unwrap_or_default();
        if latest {
            dates.max_by_key(key)
        } else {
            dates.min_by_key(key)
        }
        .cloned()
        .unwrap_or(BaseValue::Null)
    };
    match kind {
        Empty => count(&BaseValue::empty),
        Filled => count(&|v| !v.empty()),
        Checked => count(&|v| *v == BaseValue::Boolean(true)),
        Unchecked => count(&|v| *v == BaseValue::Boolean(false)),
        Unique => {
            let mut unique: Vec<&BaseValue> = vec![];
            for v in values {
                if !unique.contains(&v) {
                    unique.push(v)
                }
            }
            BaseValue::Number(unique.len() as f64)
        }
        Sum => BaseValue::Number(nums.iter().sum()),
        Average => numeric(&|_, mean| hundredths(mean)),
        Min => numeric(&|n, _| n[0]),
        Max => numeric(&|n, _| n[n.len() - 1]),
        Range => numeric(&|n, _| n[n.len() - 1] - n[0]),
        Median => numeric(&|n, _| {
            let mid = n.len() / 2;
            if n.len().is_multiple_of(2) {
                (n[mid - 1] + n[mid]) / 2.0
            } else {
                n[mid]
            }
        }),
        Stddev => numeric(&|n, mean| {
            hundredths((n.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n.len() as f64).sqrt())
        }),
        Earliest => dated(false),
        Latest => dated(true),
    }
}

/// The canonical root of the kiln `name`, open in the index. RPC and Lua
/// callers both resolve a kiln here, so both compare paths in one form.
pub(crate) async fn kiln_root(
    ctx: &crate::rpc::RpcContext,
    name: &crucible_core::config::KilnName,
) -> Result<PathBuf> {
    let root = ctx
        .kiln_registry
        .resolve(name)
        .path()
        .ok_or_else(|| operation::not_found(format!("Kiln is not available: {name}")))?
        .canonicalize()?;
    ctx.kiln.open(&root).await?;
    Ok(root)
}

/// Answer one `base.*` RPC. A person at a client writes for no session.
pub(crate) async fn handle(
    op: BaseOperation,
    req: crate::protocol::Request,
    ctx: &crate::rpc::RpcContext,
) -> crate::protocol::Response {
    match handle_inner(op, &req, ctx).await {
        Ok(value) => crate::protocol::Response::success(req.id, value),
        Err(e) => crate::protocol::Response::error(
            req.id,
            operation::Failure::of(&e).rpc_code(),
            format!("{e:#}"),
        ),
    }
}
async fn handle_inner(
    op: BaseOperation,
    req: &crate::protocol::Request,
    ctx: &crate::rpc::RpcContext,
) -> Result<serde_json::Value> {
    let name = req
        .params
        .get("kiln")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            ctx.config_default_kiln
                .clone()
                .or_else(|| ctx.kiln_state.default_kiln())
        })
        .ok_or_else(|| anyhow::anyhow!("Specify a kiln"))?;
    let name = crucible_core::config::KilnName::parse(&name)?;
    let root = kiln_root(ctx, &name).await?;
    let mut params = match &req.params {
        serde_json::Value::Null => serde_json::json!({}),
        params if params.is_object() => params.clone(),
        _ => anyhow::bail!("Bases parameters must be an object"),
    };
    params["kiln"] = serde_json::json!(name);
    let writer = disposition::Writer {
        ctx: Some(ctx),
        session: None,
    };
    operation::execute(op, &root, params, &writer).await
}

async fn file_hash(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; 65536];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
