//! Bases writes: property edits, entry creation, folder moves and saved group
//! order. Each edit changes only the bytes it means to change.
use super::disposition::{Admission, Landed, Writer};
use super::operation::{
    CreateEntryParams, EnsureBaseParams, ReorderGroupsParams, SetPropertyParams, WriteOutcome,
};
use super::*;
use crate::file_write::lock;
use crucible_core::bases::{BinaryOp, Expr, Function, Namespace};
use crucible_core::note_frontmatter::{frontmatter_mapping, newline_of, set_frontmatter_key};
use crucible_core::proposal::ProposedWrite;
use crucible_core::{file_write::ExpectedBase, note_edit::disk_hash};
use serde_json::{json, Value as Json};

fn relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path.strip_prefix(root)?.to_string_lossy().into_owned())
}

/// `text` with `key` set to `value`, or deleted when `value` is `None`. A
/// value written over a list becomes a one-item list, as Obsidian does.
/// `None` when the text already says so.
fn property_text(text: &str, key: &str, value: Option<Json>) -> Result<Option<String>> {
    let value = match value {
        Some(value)
            if !value.is_array()
                && frontmatter_mapping(text)?
                    .get(key)
                    .is_some_and(serde_yaml::Value::is_sequence) =>
        {
            Some(json!([value]))
        }
        other => other,
    };
    Ok(set_frontmatter_key(text, key, value.as_ref())?)
}

pub(super) async fn set_property(
    root: &Path,
    params: &SetPropertyParams,
    writer: &Writer<'_>,
) -> Result<WriteOutcome> {
    if params.key == "file.folder" {
        return move_entry(root, params, writer).await;
    }
    let value = match (&params.value, params.delete) {
        (Some(value), false) => Some(value.clone()),
        (None | Some(Json::Null), true) => None,
        (Some(_), true) => anyhow::bail!("Give a value or delete, not both"),
        (None, false) => anyhow::bail!("A property write needs a value or delete"),
    };
    let _order = writer.serialize(root).await?;
    let path = contained(root, &params.path)?;
    ensure!(
        crucible_core::kiln::is_note_file(&path),
        "Only note properties can be edited"
    );
    let key = params.key.as_str();
    ensure!(
        !key.starts_with("file.") && !key.starts_with("formula."),
        "Computed properties cannot be edited"
    );
    let key = key.strip_prefix("note.").unwrap_or(key);
    let _guard = lock(&path).await;
    writer.read_path(root, &path)?;
    let text = tokio::fs::read_to_string(&path).await?;
    let current_hash = disk_hash(&text);
    if current_hash != params.ancestor_hash {
        return Ok(WriteOutcome::Stale {
            path: relative(root, &path)?,
            current_hash,
        });
    }
    let text = writer.proposed_text(root, &path)?.unwrap_or(text);
    let types = property_types(root).await?;
    let value = value.map(|v| normalize_property(key, v, &types));
    let Some(content) = property_text(&text, key, value)? else {
        return Ok(WriteOutcome::Unchanged {
            path: relative(root, &path)?,
            ancestor_hash: current_hash,
        });
    };
    writer
        .put(
            root,
            &path,
            content,
            ExpectedBase::Hash { hash: current_hash },
        )
        .await
}
fn plain(v: &BaseValue) -> Json {
    match v {
        BaseValue::Null => Json::Null,
        BaseValue::Boolean(b) => json!(b),
        BaseValue::Number(n) => json!(n),
        BaseValue::List(xs) => Json::Array(xs.iter().map(plain).collect()),
        BaseValue::Object(o) => json!(o
            .iter()
            .map(|(k, v)| (k.clone(), plain(v)))
            .collect::<BTreeMap<_, _>>()),
        _ => json!(v.text()),
    }
}
fn namespace(e: &Expr) -> Option<Namespace> {
    match e {
        Expr::Name(n) => n.parse().ok(),
        _ => None,
    }
}
/// The note property that `e` reads: a bare name or `note.key`.
fn property(e: &Expr) -> Option<String> {
    match e {
        Expr::Name(n) if namespace(e).is_none() => Some(n.clone()),
        Expr::Get(o, k) if namespace(o) == Some(Namespace::Note) => match k.as_ref() {
            Expr::Literal(BaseValue::String(k)) => Some(k.clone()),
            _ => None,
        },
        _ => None,
    }
}
/// Whether `e` reads the field `key` of `namespace`.
fn field(e: &Expr, of: Namespace, key: &str) -> bool {
    matches!(e, Expr::Get(o, k) if namespace(o) == Some(of)
        && matches!(k.as_ref(), Expr::Literal(BaseValue::String(s)) if s == key))
}
/// Record in `props` and `folder` what an entry needs so that `e` admits it.
fn derive(e: &Expr, props: &mut BTreeMap<String, Json>, folder: &mut String) {
    match e {
        Expr::Binary(BinaryOp::Equal | BinaryOp::GreaterEqual | BinaryOp::LessEqual, l, r) => {
            if let Expr::Literal(v) = r.as_ref() {
                if let Some(k) = property(l) {
                    props.insert(k, plain(v));
                } else if field(l, Namespace::File, "folder") {
                    *folder = v.text();
                }
            }
        }
        Expr::Binary(BinaryOp::And | BinaryOp::Or, l, r) => {
            derive(l, props, folder);
            derive(r, props, folder);
        }
        Expr::Call {
            function,
            receiver: Some(receiver),
            args,
        } => {
            let literal = args
                .iter()
                .filter_map(|e| match e {
                    Expr::Literal(v) => Some(plain(v)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let first = |literal: Vec<Json>| literal.into_iter().take(1).collect::<Vec<_>>();
            if let Some(key) = property(receiver) {
                match function {
                    Function::IsEmpty => {
                        props.insert(key, json!(""));
                    }
                    Function::ContainsAll | Function::ContainsAny => {
                        let values = if *function == Function::ContainsAny {
                            first(literal)
                        } else {
                            literal
                        };
                        if !values.is_empty() {
                            props.insert(key, json!(values));
                        }
                    }
                    Function::Contains | Function::StartsWith | Function::EndsWith => {
                        if let Some(v) = literal.into_iter().next() {
                            props.insert(key, v);
                        }
                    }
                    _ => {}
                }
            } else if namespace(receiver) == Some(Namespace::File) {
                match (function, literal.first()) {
                    (Function::InFolder, Some(Json::String(v))) => *folder = v.clone(),
                    (Function::HasProperty, Some(Json::String(v))) => {
                        props.entry(v.clone()).or_insert(Json::Null);
                    }
                    (Function::HasTag, _) if args.len() == 1 => {
                        let tags = props.entry("tags".into()).or_insert(json!([]));
                        if let Some(xs) = tags.as_array_mut() {
                            xs.extend(literal)
                        }
                    }
                    _ => {}
                }
            } else if field(receiver, Namespace::File, "tags")
                && matches!(
                    function,
                    Function::Contains | Function::ContainsAll | Function::ContainsAny
                )
            {
                let values = if *function == Function::ContainsAny {
                    first(literal)
                } else {
                    literal
                };
                props.insert("tags".into(), json!(values));
            }
        }
        _ => {}
    }
}
fn derive_filter(filter: Option<&Filter>, props: &mut BTreeMap<String, Json>, folder: &mut String) {
    match filter {
        Some(Filter::Expression(e)) => derive(e.expr(), props, folder),
        Some(Filter::Tree(FilterTree::And(xs) | FilterTree::Or(xs))) => {
            for x in xs {
                derive_filter(Some(x), props, folder)
            }
        }
        Some(Filter::Tree(FilterTree::Not(_))) | None => {}
    }
}

/// Whether a file or a symlink, dangling or not, holds `path`.
async fn occupied(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

pub(super) async fn create_entry(
    root: &Path,
    params: &CreateEntryParams,
    writer: &Writer<'_>,
) -> Result<WriteOutcome> {
    let source = &params.source;
    if let Source::Path { path } = source {
        writer.read_path(root, &source_path(root, path)?)?;
    }
    let base = load(root, source).await?;
    let view = base.view(params.view.as_deref())?;
    let mut props = BTreeMap::new();
    let mut folder = String::new();
    derive_filter(base.filters.as_ref(), &mut props, &mut folder);
    derive_filter(view.filters.as_ref(), &mut props, &mut folder);
    for key in &view.order {
        if let Some(k) = key.strip_prefix("note.") {
            props.entry(k.into()).or_insert(Json::Null);
        }
    }
    if let Some(f) = &base.new_item_folder {
        folder = f.clone()
    }
    if let (Some(g), Some(value)) = (&view.group_by, &params.group) {
        if g.property == "file.folder" {
            folder = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Folder group must be text"))?
                .into()
        } else {
            let key = g.property.strip_prefix("note.").unwrap_or(&g.property);
            ensure!(
                !key.starts_with("file.") && !key.starts_with("formula."),
                "Cannot create in a computed group"
            );
            if value.is_null() {
                props.remove(key);
            } else {
                props.insert(key.into(), value.clone());
            }
        }
    }
    let parent = if folder.is_empty() {
        root.to_owned()
    } else {
        contained(root, &folder)?
    };
    ensure!(parent.is_dir(), "Entry folder must exist");
    let name = params.name.as_deref().unwrap_or("Untitled");
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', '\0']) && !name.starts_with('.'),
        "Entry name must be a filename"
    );
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let mut body = if let Some(content) = &params.content {
        content.clone()
    } else if let Some(template) = &base.new_item_template {
        let path = contained(root, template)?;
        writer.read_path(root, &path)?;
        tokio::fs::read_to_string(path).await?
    } else {
        String::new()
    };
    let types = property_types(root).await?;
    for (key, value) in props {
        let value = normalize_property(&key, value, &types);
        if let Some(text) = property_text(&body, &key, Some(value))? {
            body = text;
        }
    }
    let _order = writer.serialize(root).await?;
    for i in 0..10_000 {
        let filename = if i == 0 {
            format!("{stem}.md")
        } else {
            format!("{stem} {i}.md")
        };
        let path = parent.join(filename);
        let _guard = lock(&path).await;
        if occupied(&path).await || writer.proposed_text(root, &path)?.is_some() {
            continue;
        }
        return writer.put(root, &path, body, ExpectedBase::Absent).await;
    }
    anyhow::bail!("No unused entry name")
}

/// Create the `.base` file at `params.path` only when nothing holds that
/// path. An existing file, or a symlink to one, is left as it is.
pub(super) async fn ensure_base(
    root: &Path,
    params: &EnsureBaseParams,
    writer: &Writer<'_>,
) -> Result<WriteOutcome> {
    ensure!(
        Path::new(&params.path)
            .extension()
            .is_some_and(|x| x == "base"),
        "Expected a .base file"
    );
    BaseFile::parse(&params.yaml)?;
    let path = match writer.scope(root) {
        Some(scope) => scope.resolve_for_write(&params.path)?.as_path().to_owned(),
        None => {
            ensure!(
                !Path::new(&params.path)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_))),
                "Path traversal is not allowed"
            );
            root.join(&params.path)
        }
    };
    let _order = writer.serialize(root).await?;
    let _guard = lock(&path).await;
    if occupied(&path).await {
        return Ok(WriteOutcome::Unchanged {
            path: params.path.clone(),
            ancestor_hash: Writer::current_hash(&path).await.unwrap_or_default(),
        });
    }
    writer
        .put(root, &path, params.yaml.clone(), ExpectedBase::Absent)
        .await
}

fn normalize_property(key: &str, value: Json, types: &BTreeMap<String, String>) -> Json {
    if !value.is_array()
        && !value.is_null()
        && (key == "tags"
            || matches!(
                types.get(key).map(String::as_str),
                Some("multitext" | "tags" | "aliases")
            ))
    {
        json!([value])
    } else {
        value
    }
}

/// One line of a YAML document, with its indentation.
struct Line<'a> {
    start: usize,
    text: &'a str,
    indent: usize,
}
impl Line<'_> {
    fn content(&self) -> &str {
        self.text[self.indent..].trim_end_matches(['\r', '\n'])
    }
    /// A blank line or a comment, which belongs to no key.
    fn filler(&self) -> bool {
        let c = self.content().trim();
        c.is_empty() || c.starts_with('#')
    }
}
fn lines(yaml: &str) -> Vec<Line<'_>> {
    let mut start = 0;
    yaml.split_inclusive('\n')
        .map(|text| {
            let line = Line {
                start,
                text,
                indent: text.len() - text.trim_start_matches(' ').len(),
            };
            start += text.len();
            line
        })
        .collect()
}
/// The key a mapping line starts with, plain or quoted.
fn key_of(content: &str) -> Option<&str> {
    let (key, _) = content.split_once(':')?;
    let key = key.trim();
    Some(
        key.strip_prefix('"')
            .and_then(|k| k.strip_suffix('"'))
            .or_else(|| key.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))
            .unwrap_or(key),
    )
}
/// The end (exclusive line index) of the block that starts at line `i`,
/// whose key sits at column `indent`: deeper lines, fillers, and sequence
/// items at the key's own column.
fn block_end(lines: &[Line<'_>], i: usize, indent: usize) -> usize {
    let mut end = i + 1;
    for (j, line) in lines.iter().enumerate().skip(i + 1) {
        if line.filler() {
            continue;
        }
        if line.indent > indent || (line.indent == indent && line.content().starts_with("- ")) {
            end = j + 1;
        } else {
            break;
        }
    }
    end
}

/// Set `groupOrder` of the view at `index` in the block-style `yaml`,
/// changing only that key's lines. `None` when the views are not written
/// as a block sequence.
fn splice_group_order(yaml: &str, index: usize, order: Option<&[Json]>) -> Option<String> {
    let newline = newline_of(yaml);
    let lines = lines(yaml);
    let views = lines
        .iter()
        .position(|l| l.indent == 0 && l.content().trim_end() == "views:")?;
    // The items of the views sequence, as (first line, key column).
    let mut items = vec![];
    let mut end = lines.len();
    let mut item_indent = None;
    for (j, line) in lines.iter().enumerate().skip(views + 1) {
        if line.filler() {
            continue;
        }
        let dash = line.content().starts_with("- ") || line.content() == "-";
        if dash && item_indent.is_none_or(|i| i == line.indent) {
            item_indent = Some(line.indent);
            let after = &line.content()[1..];
            items.push((j, line.indent + 1 + after.len() - after.trim_start().len()));
        } else if item_indent.is_none_or(|i| line.indent <= i) {
            end = j;
            break;
        }
    }
    let (first, key_indent) = *items.get(index)?;
    let item_end = items.get(index + 1).map_or(end, |(j, _)| *j);
    let rendered = match order {
        Some(order) => format!(
            "{}groupOrder: {}{newline}",
            " ".repeat(key_indent),
            serde_json::to_string(order).ok()?
        ),
        None => String::new(),
    };
    // The first key shares the dash's line; the others start their own.
    if key_of(&lines[first].text[key_indent..]) == Some("groupOrder") {
        return None;
    }
    let own = (first + 1..item_end).find(|&j| {
        let l = &lines[j];
        !l.filler() && l.indent == key_indent && key_of(l.content()) == Some("groupOrder")
    });
    let (from, to) = match own {
        Some(j) => {
            let e = block_end(&lines[..item_end], j, key_indent);
            (lines[j].start, lines.get(e).map_or(yaml.len(), |l| l.start))
        }
        None => {
            // After the item's last line that is not filler.
            let last = (first..item_end).rev().find(|&j| !lines[j].filler())?;
            let at = lines[last].start + lines[last].text.len();
            let needs_break = !lines[last].text.ends_with('\n');
            let rendered = if needs_break {
                format!("{newline}{rendered}")
            } else {
                rendered
            };
            return Some(format!("{}{}{}", &yaml[..at], rendered, &yaml[at..]));
        }
    };
    Some(format!("{}{}{}", &yaml[..from], rendered, &yaml[to..]))
}

/// `yaml` with the whole `views` key rewritten, for a base whose views are
/// not a block sequence. Other top-level keys keep their bytes.
fn replace_views(yaml: &str, views: &serde_yaml::Value) -> Result<String> {
    let newline = newline_of(yaml);
    let lines = lines(yaml);
    let at = lines
        .iter()
        .position(|l| l.indent == 0 && key_of(l.content()) == Some("views"))
        .ok_or_else(|| anyhow::anyhow!("The base defines no views"))?;
    let end = block_end(&lines, at, 0);
    let mut block = serde_yaml::Mapping::new();
    block.insert("views".into(), views.clone());
    let rendered = serde_yaml::to_string(&block)?.replace('\n', newline);
    let to = lines.get(end).map_or(yaml.len(), |l| l.start);
    Ok(format!(
        "{}{}{}",
        &yaml[..lines[at].start],
        rendered,
        &yaml[to..]
    ))
}

/// The base text `yaml` with the saved group order of the view at `index`
/// set to `order`. Checked by parsing: the result must equal the old
/// document with only that key changed.
fn group_order_text(yaml: &str, index: usize, order: Option<&[Json]>) -> Result<String> {
    let mut expected: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let view = expected
        .get_mut("views")
        .and_then(|v| v.get_mut(index))
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| anyhow::anyhow!("View not found"))?;
    match order {
        Some(order) => {
            view.insert("groupOrder".into(), serde_yaml::to_value(order)?);
        }
        None => {
            view.remove("groupOrder");
        }
    }
    let text = match splice_group_order(yaml, index, order) {
        Some(text) => text,
        None => replace_views(yaml, &expected["views"])?,
    };
    ensure!(
        serde_yaml::from_str::<serde_yaml::Value>(&text)? == expected,
        "Could not change groupOrder without changing other options"
    );
    Ok(text)
}

pub(super) async fn reorder_groups(
    root: &Path,
    params: &ReorderGroupsParams,
    writer: &Writer<'_>,
) -> Result<WriteOutcome> {
    let source = &params.source;
    let path = match source {
        Source::Path { path } => source_path(root, path)?,
        Source::Inline { .. } => contained(
            root,
            params
                .host
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Inline writes require the host note"))?,
        )?,
    };
    let _order = writer.serialize(root).await?;
    let _guard = lock(&path).await;
    writer.read_path(root, &path)?;
    let text = tokio::fs::read_to_string(&path).await?;
    let current_hash = disk_hash(&text);
    if current_hash != params.ancestor_hash {
        return Ok(WriteOutcome::Stale {
            path: relative(root, &path)?,
            current_hash,
        });
    }
    let range = match source {
        Source::Inline { yaml } => super::inline::range(&path, &text, yaml).await?,
        Source::Path { .. } => 0..text.len(),
    };
    let yaml = &text[range.clone()];
    let base = BaseFile::parse(yaml)?;
    // The same refusal as `BaseFile::view`, so every operation answers an
    // unknown view alike.
    let index = match params.view.as_deref() {
        Some(name) => base
            .views
            .iter()
            .position(|v| v.name == name)
            .ok_or_else(|| anyhow::anyhow!("Unknown base view: {name}"))?,
        None => 0,
    };
    // `BaseFile::parse` supplies a default view that the file does not hold,
    // and that view has no groups to order.
    ensure!(
        base.views[index].group_by.is_some(),
        "Group order needs a view with groupBy"
    );
    if base.views[index].group_order == params.group_order {
        return Ok(WriteOutcome::Unchanged {
            path: relative(root, &path)?,
            ancestor_hash: current_hash,
        });
    }
    let yaml = group_order_text(yaml, index, params.group_order.as_deref())?;
    let content = format!("{}{}{}", &text[..range.start], yaml, &text[range.end..]);
    writer
        .put(
            root,
            &path,
            content,
            ExpectedBase::Hash { hash: current_hash },
        )
        .await
}

/// Move a note into the folder `params.value` names: the `file.folder`
/// property of Obsidian. Inbound links follow the note, and every file that
/// the move writes is locked, admitted and attributed like any Bases write.
async fn move_entry(
    root: &Path,
    params: &SetPropertyParams,
    writer: &Writer<'_>,
) -> Result<WriteOutcome> {
    let km = writer
        .ctx
        .map(|ctx| &ctx.kiln)
        .ok_or_else(|| anyhow::anyhow!("Folder moves need the daemon's kiln index"))?;
    let folder = params
        .value
        .as_ref()
        .and_then(Json::as_str)
        .ok_or_else(|| anyhow::anyhow!("Folder must be text"))?;
    let _order = writer.serialize(root).await?;
    let from = contained(root, &params.path)?;
    let folder = if folder.is_empty() {
        root.to_owned()
    } else {
        contained(root, folder)?
    };
    ensure!(folder.is_dir(), "Folder does not exist");
    let to = folder.join(
        from.file_name()
            .ok_or_else(|| anyhow::anyhow!("Expected file"))?,
    );
    let (from_rel, to_rel) = (relative(root, &from)?, relative(root, &to)?);
    let note = crucible_core::kiln::is_indexable_file(&from);
    let plan = if note && from != to {
        Some(crate::server::note_refactor::plan_rename(km, root, &from_rel, &to_rel).await?)
    } else {
        None
    };
    let rewritten = plan
        .as_ref()
        .map(|p| p.rewritten(&from_rel))
        .unwrap_or_default();
    let mut paths = [from.clone(), to.clone()]
        .into_iter()
        .chain(rewritten.iter().map(|r| root.join(r)))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let mut guards = vec![];
    for path in &paths {
        guards.push(lock(path).await)
    }
    let current = file_hash(&from).await?;
    if current != params.ancestor_hash {
        return Ok(WriteOutcome::Stale {
            path: from_rel,
            current_hash: current,
        });
    }
    if from == to {
        return Ok(WriteOutcome::Unchanged {
            path: from_rel,
            ancestor_hash: current,
        });
    }
    // Planned again under the locks, so that the checked bytes are the bytes
    // that land. A source that the first plan did not lock is not written.
    let plan = match plan {
        Some(_) => {
            let plan =
                crate::server::note_refactor::plan_rename(km, root, &from_rel, &to_rel).await?;
            ensure!(
                plan.rewritten(&from_rel)
                    .iter()
                    .all(|r| paths.contains(&root.join(r))),
                "The links to this note changed during the move; try again"
            );
            Some(plan)
        }
        None => None,
    };
    // The text the moved file holds after the move: self-links follow it.
    let moved_text = match plan.as_ref().and_then(|p| p.moved_bytes(&from_rel)) {
        Some(bytes) => Some(String::from_utf8(bytes.to_vec())?),
        None => match tokio::fs::read_to_string(&from).await {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => None,
            Err(e) => return Err(e.into()),
        },
    };
    let content = moved_text
        .as_deref()
        .filter(|_| crucible_core::kiln::is_note_file(&from));
    let sources = match &plan {
        Some(plan) => plan.source_edits(root, &from_rel, &to_rel)?,
        None => vec![],
    };
    let refused = |path: &str, reason| {
        Ok(WriteOutcome::Refused {
            path: path.to_owned(),
            reason,
        })
    };
    let mut payloads = vec![];
    match writer.admit(root, &to, &from, content).await? {
        Admission::Admitted(payload) => payloads.push(payload),
        Admission::Refused(reason) => return refused(&from_rel, reason),
    }
    if let Some(reason) = writer.permit(root, &from_rel).await? {
        return refused(&from_rel, reason);
    }
    // Each rewritten source is a write of its own: the policy sees its
    // final text, and the permission and scope rules apply to it.
    for edit in &sources {
        let path = root.join(&edit.path);
        match writer.admit(root, &path, &path, Some(&edit.text)).await? {
            Admission::Admitted(payload) => payloads.push(payload),
            Admission::Refused(reason) => return refused(&edit.path, reason),
        }
    }
    let proposed_root = crucible_core::session::PhysicalRoot::from_top_level(root.to_path_buf());
    let write = |path: &str, base, new_text: String| ProposedWrite {
        root: proposed_root.clone(),
        path: path.to_owned(),
        base,
        new_text,
        remove: false,
        moved_from: None,
    };
    // A proposal holds text, so a file that is not text moves only on disk.
    // The deletion keeps the old text as its base, so a review shows the
    // move as a rename with its diff.
    let original = match &moved_text {
        Some(_) => Some(tokio::fs::read_to_string(&from).await?),
        None => None,
    };
    let changes = moved_text.zip(original).map(|(moved, original)| {
        [
            ProposedWrite {
                remove: true,
                ..write(
                    &from_rel,
                    ExpectedBase::Text {
                        hash: disk_hash(&original),
                        text: original,
                    },
                    String::new(),
                )
            },
            ProposedWrite {
                moved_from: Some(from_rel.clone()),
                ..write(&to_rel, ExpectedBase::Absent, moved)
            },
        ]
        .into_iter()
        .chain(sources.into_iter().map(|edit| {
            write(
                &edit.path,
                ExpectedBase::Text {
                    hash: disk_hash(&edit.original),
                    text: edit.original,
                },
                edit.text,
            )
        }))
        .collect()
    });
    let apply = async {
        match plan {
            Some(plan) => {
                crate::server::note_refactor::apply_rename(km, root, &from_rel, &to_rel, plan)
                    .await?;
            }
            None => {
                crate::server::fs::move_within(root, &from_rel, &to_rel)?;
            }
        }
        anyhow::Ok(())
    };
    match writer.dispose(root, changes, apply).await? {
        Landed::Proposed(proposal) => Ok(WriteOutcome::Proposed {
            path: to_rel,
            proposal: proposal.id.to_string(),
        }),
        Landed::Unproposable(reason) => refused(&from_rel, reason),
        Landed::Applied(()) => {
            for payload in payloads {
                writer.changed(payload);
            }
            Ok(WriteOutcome::Applied {
                ancestor_hash: file_hash(&to).await?,
                path: to_rel,
            })
        }
    }
}
