//! Daemon-owned Bases queries. Files remain the source of truth, including attachments.
use crucible_core::bases::BaseValue;
mod eval;
#[cfg(test)]
mod tests;
mod write;
use anyhow::{ensure, Context, Result};
use crucible_core::bases::{BaseFile, Direction, Expr, Filter, FilterTree, Value, View};
use eval::{compare, Eval};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default)]
pub(super) struct Entry {
    path: String,
    ancestor_hash: String,
    properties: BTreeMap<String, Value>,
    tags: Vec<String>,
    links: Vec<String>,
    embeds: Vec<String>,
    size: u64,
    mtime: Option<i64>,
    ctime: Option<i64>,
}
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
    pub values: BTreeMap<String, BaseValue>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Group {
    pub value: BaseValue,
    pub rows: Vec<Row>,
    pub summaries: BTreeMap<String, BaseValue>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct QueryResult {
    pub source_hash: Option<String>,
    pub view: String,
    pub view_type: String,
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
    pub kind: String,
}
impl From<&View> for ViewSummary {
    fn from(view: &View) -> Self {
        Self {
            name: view.name.clone(),
            kind: view.kind.clone().into(),
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
async fn source_text(root: &Path, source: &Source) -> Result<String> {
    let yaml = match source {
        Source::Path { path } => {
            let path = contained(root, path)?;
            ensure!(
                crucible_core::kiln::KilnFileKind::of(&path)
                    == crucible_core::kiln::KilnFileKind::Base,
                "Expected a .base file"
            );
            tokio::fs::read_to_string(path).await?
        }
        Source::Inline { yaml } => yaml.clone(),
    };
    ensure!(yaml.len() <= 1_000_000, "Base source exceeds 1 MB");
    Ok(yaml)
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

async fn entries(root: &Path) -> Result<Vec<Entry>> {
    let mut out = vec![];
    let types = property_types(root).await?;
    let parser = crucible_core::parser::CrucibleParser::new();
    let paths = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0
                || (!e.file_name().to_string_lossy().starts_with('.')
                    && !crucible_core::kiln::EXCLUDED_DIRS
                        .contains(&e.file_name().to_string_lossy().as_ref()))
        });
    for item in paths {
        let item = item?;
        if !item.file_type().is_file() {
            continue;
        }
        let path = contained(root, &item.path().to_string_lossy())?;
        let meta = tokio::fs::metadata(&path).await?;
        let millis = |t: std::io::Result<std::time::SystemTime>| {
            t.ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
        };
        let mut e = Entry {
            path: path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/"),
            size: meta.len(),
            mtime: millis(meta.modified()),
            ctime: millis(meta.created()),
            ..Entry::default()
        };
        if crucible_core::kiln::is_note_file(&path) {
            let content = tokio::fs::read_to_string(&path).await?;
            e.ancestor_hash = crucible_core::note_edit::disk_hash(&content);
            let parsed = parser.parse_content(&content, &path).await?;
            if let Some(fm) = parsed.frontmatter {
                e.properties = fm
                    .properties()
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from_json(v)))
                    .collect();
            }
            for (key, value) in &mut e.properties {
                if matches!(
                    types.get(key).map(String::as_str),
                    Some("date" | "datetime")
                ) {
                    if let Value::String(text) = value {
                        *value = Value::Date(eval::parse_date(text)?);
                    }
                }
            }
            e.tags = parsed
                .tags
                .into_iter()
                .map(|t| t.name.trim_start_matches('#').to_owned())
                .collect();
            if let Some(tags) = e.properties.get("tags") {
                let values = match tags {
                    Value::List(v) => v.clone(),
                    v => vec![v.clone()],
                };
                e.tags.extend(
                    values
                        .iter()
                        .map(|v| v.text().trim_start_matches('#').to_owned()),
                );
            }
            e.tags.sort();
            e.tags.dedup();
            e.links = parsed.wikilinks.iter().map(|l| l.target.clone()).collect();
            e.embeds = parsed
                .wikilinks
                .iter()
                .filter(|l| l.is_embed)
                .map(|l| l.target.clone())
                .collect();
            for link in parsed.inline_links {
                if !link.url.contains("://") && !link.url.starts_with('#') {
                    e.links.push(link.url)
                }
            }
            fn links(v: &Value, out: &mut Vec<String>) {
                match v {
                    Value::Link { path, .. } => out.push(path.clone()),
                    Value::List(xs) => {
                        for x in xs {
                            links(x, out)
                        }
                    }
                    Value::Object(o) => {
                        for x in o.values() {
                            links(x, out)
                        }
                    }
                    _ => {}
                }
            }
            for value in e.properties.values() {
                links(value, &mut e.links)
            }
        }
        if e.ancestor_hash.is_empty() {
            e.ancestor_hash = file_hash(&path).await?;
        }
        out.push(e);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
fn matches(filter: Option<&Filter>, eval: &mut Eval<'_>) -> Result<bool> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    match filter {
        Filter::Expression(s) => Ok(eval.eval(&Expr::parse(s)?)?.truthy()),
        Filter::Tree(FilterTree::And(xs)) => {
            for x in xs {
                if !matches(Some(x), eval)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Filter::Tree(FilterTree::Or(xs)) => {
            for x in xs {
                if matches(Some(x), eval)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Filter::Tree(FilterTree::Not(xs)) => {
            for x in xs {
                if matches(Some(x), eval)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}
pub async fn query(root: &Path, request: &Query) -> Result<QueryResult> {
    let root = root.canonicalize()?;
    let yaml = source_text(&root, &request.source).await?;
    let source_hash = matches!(request.source, Source::Path { .. })
        .then(|| crucible_core::note_edit::disk_hash(&yaml));
    let base = BaseFile::parse(&yaml)?;
    let view = base.view(request.view.as_deref())?;
    let all = entries(&root).await?;
    let host_path = request.host.as_deref().or(match &request.source {
        Source::Path { path } => Some(path.as_str()),
        _ => None,
    });
    let host_path = host_path
        .map(|p| {
            contained(&root, p)
                .and_then(|p| Ok(p.strip_prefix(&root)?.to_string_lossy().into_owned()))
        })
        .transpose()?;
    let host = host_path
        .as_ref()
        .and_then(|p| all.iter().find(|e| &e.path == p));
    let now = chrono::Utc::now().timestamp_millis();
    let order = if view.order.is_empty() {
        vec!["file.name".into()]
    } else {
        view.order.clone()
    };
    let columns = order
        .iter()
        .map(|p| Column {
            property: p.clone(),
            display_name: base
                .properties
                .get(p)
                .or_else(|| base.properties.get(p.strip_prefix("note.").unwrap_or(p)))
                .and_then(|p| p.display_name.clone())
                .unwrap_or_else(|| p.clone()),
        })
        .collect();
    let mut selected = vec![];
    for entry in &all {
        let mut eval = Eval::new(&all, entry, host, &base.formulas, now);
        if !matches(base.filters.as_ref(), &mut eval).with_context(|| entry.path.clone())?
            || !matches(view.filters.as_ref(), &mut eval)?
        {
            continue;
        }
        let values = order
            .iter()
            .chain(view.summaries.keys())
            .map(|p| Ok((p.clone(), eval.property(p)?)))
            .collect::<Result<_>>()?;
        let sort = view
            .sort
            .iter()
            .map(|s| eval.property(&s.property))
            .collect::<Result<Vec<_>>>()?;
        let group = view
            .group_by
            .as_ref()
            .map(|g| eval.property(&g.property))
            .transpose()?
            .unwrap_or(Value::Null);
        selected.push((
            Row {
                path: entry.path.clone(),
                ancestor_hash: entry.ancestor_hash.clone(),
                values,
            },
            sort,
            group,
        ));
    }
    selected.sort_by(|a, b| {
        for (i, s) in view.sort.iter().enumerate() {
            let cmp = compare(&a.1[i], &b.1[i]);
            let cmp = if s.direction == Direction::DESC {
                cmp.reverse()
            } else {
                cmp
            };
            if !cmp.is_eq() {
                return cmp;
            }
        }
        a.0.path.cmp(&b.0.path)
    });
    if let Some(limit) = view.limit {
        selected.truncate(limit)
    }
    let rows: Vec<_> = selected.iter().map(|(r, _, _)| r.clone()).collect();
    let mut groups: Vec<Group> = vec![];
    if view.group_by.is_some() {
        for (row, _, value) in selected {
            if let Some(g) = groups.iter_mut().find(|g| g.value == value) {
                g.rows.push(row)
            } else {
                groups.push(Group {
                    value,
                    rows: vec![row],
                    summaries: BTreeMap::new(),
                })
            }
        }
        if let Some(order) = &view.group_order {
            for v in order {
                let value = Value::from_json(v);
                if !groups.iter().any(|g| g.value == value) {
                    groups.push(Group {
                        value,
                        rows: vec![],
                        summaries: BTreeMap::new(),
                    })
                }
            }
        }
        groups.sort_by(|a, b| {
            if a.value.empty() || b.value.empty() {
                return b.value.empty().cmp(&a.value.empty());
            }
            if let Some(order) = &view.group_order {
                let rank = |v: &Value| {
                    order
                        .iter()
                        .position(|x| Value::from_json(x) == *v)
                        .unwrap_or(usize::MAX)
                };
                let cmp = rank(&a.value).cmp(&rank(&b.value));
                if !cmp.is_eq() {
                    return cmp;
                }
            }
            let cmp = compare(&a.value, &b.value);
            if view
                .group_by
                .as_ref()
                .is_some_and(|g| g.direction == Direction::DESC)
            {
                cmp.reverse()
            } else {
                cmp
            }
        });
        if view.extra.get("hideEmptyGroups").and_then(|v| v.as_bool()) == Some(true) {
            groups.retain(|g| !g.rows.is_empty())
        }
        for g in &mut groups {
            g.summaries = summaries(&base, view, &g.rows, now)?
        }
    }
    Ok(QueryResult {
        source_hash,
        summaries: summaries(&base, view, &rows, now)?,
        view: view.name.clone(),
        view_type: view.kind.clone().into(),
        columns,
        rows,
        groups,
        group_property: view.group_by.as_ref().map(|g| g.property.clone()),
        views: base.views.iter().map(ViewSummary::from).collect(),
        root,
    })
}
fn summaries(
    base: &BaseFile,
    view: &View,
    rows: &[Row],
    now: i64,
) -> Result<BTreeMap<String, Value>> {
    view.summaries
        .iter()
        .map(|(key, name)| {
            let values: Vec<Value> = rows
                .iter()
                .map(|r| r.values.get(key).cloned().unwrap_or(Value::Null))
                .collect();
            let mut nums: Vec<f64> = values
                .iter()
                .filter_map(|v| match v {
                    Value::Number(n) => Some(*n),
                    Value::Date(t) => Some(*t as f64),
                    _ => None,
                })
                .collect();
            nums.sort_by(f64::total_cmp);
            let count = |predicate: fn(&Value) -> bool| {
                Value::Number(values.iter().filter(|v| predicate(v)).count() as f64)
            };
            let result = if let Some(source) = base.summaries.get(name) {
                let entry = Entry::default();
                let mut eval = Eval::new(&[], &entry, None, &base.formulas, now);
                eval.locals
                    .insert("values".into(), Value::List(values.clone()));
                eval.eval(&Expr::parse(source)?)?
            } else {
                match name.as_str() {
                    "Empty" => count(Value::empty),
                    "Filled" => count(|v| !v.empty()),
                    "Checked" => count(|v| *v == Value::Boolean(true)),
                    "Unchecked" => count(|v| *v == Value::Boolean(false)),
                    "Unique" => {
                        let mut unique = vec![];
                        for v in &values {
                            if !unique.contains(v) {
                                unique.push(v.clone())
                            }
                        }
                        Value::Number(unique.len() as f64)
                    }
                    "Sum" => Value::Number(nums.iter().sum()),
                    "Average" | "Min" | "Max" | "Range" | "Median" | "Stddev" | "Earliest"
                    | "Latest" => {
                        if nums.is_empty() {
                            Value::Null
                        } else {
                            let avg = nums.iter().sum::<f64>() / nums.len() as f64;
                            let n = match name.as_str() {
                                "Average" => avg,
                                "Min" | "Earliest" => nums[0],
                                "Max" | "Latest" => *nums.last().unwrap(),
                                "Range" => nums.last().unwrap() - nums[0],
                                "Median" => {
                                    if nums.len().is_multiple_of(2) {
                                        (nums[nums.len() / 2 - 1] + nums[nums.len() / 2]) / 2.0
                                    } else {
                                        nums[nums.len() / 2]
                                    }
                                }
                                _ => (nums.iter().map(|n| (n - avg).powi(2)).sum::<f64>()
                                    / nums.len() as f64)
                                    .sqrt(),
                            };
                            if matches!(name.as_str(), "Earliest" | "Latest") {
                                Value::Date(n as i64)
                            } else {
                                Value::Number(n)
                            }
                        }
                    }
                    _ => anyhow::bail!("Unknown summary: {name}"),
                }
            };
            Ok((key.clone(), result))
        })
        .collect()
}

pub(crate) async fn handle(
    req: crate::protocol::Request,
    ctx: &crate::rpc::RpcContext,
) -> crate::protocol::Response {
    let result = handle_inner(&req, ctx).await;
    match result {
        Ok(value) => crate::protocol::Response::success(req.id, value),
        Err(e) => crate::protocol::Response::error(
            req.id,
            crate::protocol::INVALID_PARAMS,
            format!("{e:#}"),
        ),
    }
}
async fn handle_inner(
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
    let root = ctx
        .kiln_registry
        .resolve(&name)
        .path()
        .ok_or_else(|| anyhow::anyhow!("Kiln is not available"))?;
    ctx.kiln.open(&root).await?;
    match req.method.as_str() {
        "base.list" => {
            let files = entries(&root)
                .await?
                .into_iter()
                .filter(|e| {
                    crucible_core::kiln::KilnFileKind::of(Path::new(&e.path))
                        == crucible_core::kiln::KilnFileKind::Base
                })
                .map(|e| e.path)
                .collect::<Vec<_>>();
            Ok(serde_json::json!(files))
        }
        "base.views" => {
            let source: Source = serde_json::from_value(req.params["source"].clone())?;
            let base = load(&root, &source).await?;
            Ok(serde_json::json!(base
                .views
                .iter()
                .map(ViewSummary::from)
                .collect::<Vec<_>>()))
        }
        "base.query" => {
            let mut params = req.params.clone();
            params["kiln"] = serde_json::json!(name);
            let query_request: Query = serde_json::from_value(params)?;
            Ok(serde_json::to_value(query(&root, &query_request).await?)?)
        }
        "base.set_property" if req.params["key"] == "file.folder" => {
            write::move_entry(&root, &req.params, &ctx.kiln).await
        }
        "base.set_property" => write::set_property(&root, &req.params).await,
        "base.reorder_groups" => write::reorder_groups(&root, &req.params).await,
        "base.create_entry" => write::create_entry(&root, &req.params).await,
        _ => anyhow::bail!("Unknown Bases method"),
    }
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
