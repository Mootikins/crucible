use super::*;
use crate::file_write::lock;
use crucible_core::{file_write::ExpectedBase, note_edit::disk_hash};
use serde_json::{json, Value as Json};

/// Replace only the header; the body bytes (including CRLF) are never serialized.
fn property_text(text: &str, key: &str, value: Option<Json>) -> Result<String> {
    ensure!(
        !key.is_empty() && !key.contains(['\n', '\r']),
        "Invalid property name"
    );
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut props = serde_yaml::Mapping::new();
    let mut body = text;
    if text.lines().next() == Some("+++") {
        anyhow::bail!("Bases property writes require YAML frontmatter")
    }
    if text.lines().next() == Some("---") {
        let start = text.find('\n').unwrap_or(text.len()) + 1;
        ensure!(start <= text.len(), "Unclosed frontmatter");
        let mut offset = start;
        let mut found = false;
        for line in text[start..].split_inclusive('\n') {
            if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
                let raw = &text[start..offset];
                props = if raw.trim().is_empty() {
                    serde_yaml::Mapping::new()
                } else {
                    serde_yaml::from_str(raw).context("Invalid YAML frontmatter")?
                };
                body = &text[offset + line.len()..];
                found = true;
                break;
            }
            offset += line.len();
        }
        ensure!(found, "Unclosed YAML frontmatter");
    }
    let yaml_key = serde_yaml::Value::String(key.into());
    if let Some(mut value) = value {
        if props.get(&yaml_key).is_some_and(|v| v.is_sequence()) && !value.is_array() {
            value = json!([value]);
        }
        props.insert(yaml_key, serde_yaml::to_value(value)?);
    } else {
        props.remove(&yaml_key);
    }
    let header = serde_yaml::to_string(&props)?;
    Ok(format!(
        "---{newline}{}{newline}---{newline}{body}",
        header.trim_end().replace('\n', newline)
    ))
}
#[cfg(test)]
pub(super) async fn set_property(root: &Path, params: &Json) -> Result<Json> {
    set_property_with(root, params, &super::disposition::Writer::default()).await
}
pub(super) async fn set_property_with(
    root: &Path,
    params: &Json,
    writer: &super::disposition::Writer<'_>,
) -> Result<Json> {
    let path = params["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let path = contained(root, path)?;
    ensure!(
        crucible_core::kiln::is_note_file(&path),
        "Only note properties can be edited"
    );
    let key = params["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("key required"))?;
    ensure!(
        !key.starts_with("file.") && !key.starts_with("formula."),
        "Computed properties cannot be edited"
    );
    let key = key.strip_prefix("note.").unwrap_or(key);
    let hash = params["ancestor_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("ancestor_hash required"))?;
    let _guard = lock(&path).await;
    writer.read_path(root, &path)?;
    let text = tokio::fs::read_to_string(&path).await?;
    if disk_hash(&text) != hash {
        return Ok(json!({"ok":false,"error":"stale_base","current_hash":disk_hash(&text)}));
    }
    let value = if params["delete"] == true {
        None
    } else {
        Some(
            params
                .get("value")
                .ok_or_else(|| anyhow::anyhow!("value or delete required"))?
                .clone(),
        )
    };
    let text = writer.proposed_text(root, &path)?.unwrap_or(text);
    let types = property_types(root).await?;
    let value = value.map(|v| normalize_property(key, v, &types));
    let content = property_text(&text, key, value)?;
    writer
        .put(
            root,
            &path,
            content,
            ExpectedBase::Hash { hash: hash.into() },
        )
        .await
}
fn plain(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Boolean(b) => json!(b),
        Value::Number(n) => json!(n),
        Value::List(xs) => Json::Array(xs.iter().map(plain).collect()),
        Value::Object(o) => json!(o
            .iter()
            .map(|(k, v)| (k.clone(), plain(v)))
            .collect::<BTreeMap<_, _>>()),
        _ => json!(v.text()),
    }
}
fn property(e: &Expr) -> Option<String> {
    match e {
        Expr::Name(n) if !matches!(n.as_str(), "file" | "formula" | "this") => Some(n.clone()),
        Expr::Get(o, k) if matches!(o.as_ref(),Expr::Name(n) if n=="note") => {
            if let Expr::Literal(Value::String(k)) = k.as_ref() {
                Some(k.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}
fn field(e: &Expr, namespace: &str, key: &str) -> bool {
    matches!(e,Expr::Get(o,k) if matches!(o.as_ref(),Expr::Name(n) if n==namespace)&&matches!(k.as_ref(),Expr::Literal(Value::String(s)) if s==key))
}
fn derive(e: &Expr, props: &mut BTreeMap<String, Json>, folder: &mut String) {
    match e {
        Expr::Binary(op, l, r) if matches!(op.as_str(), "==" | ">=" | "<=") => {
            if let Expr::Literal(v) = r.as_ref() {
                if let Some(k) = property(l) {
                    props.insert(k, plain(v));
                } else if field(l, "file", "folder") {
                    *folder = v.text();
                }
            }
        }
        Expr::Binary(op, l, r) if matches!(op.as_str(), "&&" | "||") => {
            derive(l, props, folder);
            derive(r, props, folder);
        }
        Expr::Call(c, args) => {
            if let Expr::Get(o, k) = c.as_ref() {
                if let Expr::Literal(Value::String(method)) = k.as_ref() {
                    let literal = args
                        .iter()
                        .filter_map(|e| {
                            if let Expr::Literal(v) = e {
                                Some(plain(v))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    if let Some(key) = property(o) {
                        if method == "isEmpty" {
                            props.insert(key, json!(""));
                        } else if matches!(method.as_str(), "containsAll" | "containsAny") {
                            let values = if method == "containsAny" {
                                literal.into_iter().take(1).collect::<Vec<_>>()
                            } else {
                                literal
                            };
                            if !values.is_empty() {
                                props.insert(key, json!(values));
                            }
                        } else if matches!(method.as_str(), "contains" | "startsWith" | "endsWith")
                        {
                            if let Some(v) = literal.first() {
                                props.insert(key, v.clone());
                            }
                        }
                    } else if matches!(o.as_ref(),Expr::Name(n) if n=="file") {
                        match method.as_str() {
                            "inFolder" => {
                                if let Some(Json::String(v)) = literal.first() {
                                    *folder = v.clone()
                                }
                            }
                            "hasProperty" => {
                                if let Some(Json::String(v)) = literal.first() {
                                    props.entry(v.clone()).or_insert(Json::Null);
                                }
                            }
                            "hasTag" if args.len() == 1 => {
                                let tags = props.entry("tags".into()).or_insert(json!([]));
                                if let Some(xs) = tags.as_array_mut() {
                                    xs.extend(literal)
                                }
                            }
                            _ => {}
                        }
                    } else if field(o, "file", "tags")
                        && matches!(method.as_str(), "contains" | "containsAll" | "containsAny")
                    {
                        props.insert(
                            "tags".into(),
                            json!(if method == "containsAny" {
                                literal.into_iter().take(1).collect()
                            } else {
                                literal
                            }),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}
fn derive_filter(
    filter: Option<&Filter>,
    props: &mut BTreeMap<String, Json>,
    folder: &mut String,
) -> Result<()> {
    if let Some(f) = filter {
        match f {
            Filter::Expression(s) => derive(&Expr::parse(s)?, props, folder),
            Filter::Tree(FilterTree::And(xs) | FilterTree::Or(xs)) => {
                for x in xs {
                    derive_filter(Some(x), props, folder)?
                }
            }
            Filter::Tree(FilterTree::Not(_)) => {}
        }
    }
    Ok(())
}
#[cfg(test)]
pub(super) async fn create_entry(root: &Path, params: &Json) -> Result<Json> {
    create_entry_with(root, params, &super::disposition::Writer::default()).await
}
pub(super) async fn create_entry_with(
    root: &Path,
    params: &Json,
    writer: &super::disposition::Writer<'_>,
) -> Result<Json> {
    let source: Source = serde_json::from_value(params["source"].clone())?;
    if let Source::Path { path } = &source {
        writer.read_path(root, &source_path(root, path)?)?;
    }
    let base = load(root, &source).await?;
    let view = base.view(params["view"].as_str())?;
    let mut props = BTreeMap::new();
    let mut folder = String::new();
    derive_filter(base.filters.as_ref(), &mut props, &mut folder)?;
    derive_filter(view.filters.as_ref(), &mut props, &mut folder)?;
    for key in &view.order {
        if let Some(k) = key.strip_prefix("note.") {
            props.entry(k.into()).or_insert(Json::Null);
        }
    }
    if let Some(f) = &base.new_item_folder {
        folder = f.clone()
    }
    if let (Some(g), Some(value)) = (&view.group_by, params.get("group")) {
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
    let name = params["name"].as_str().unwrap_or("Untitled");
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', '\0']) && !name.starts_with('.'),
        "Entry name must be a filename"
    );
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let mut body = if let Some(content) = params["content"].as_str() {
        content.to_owned()
    } else if let Some(template) = &base.new_item_template {
        let path = contained(root, template)?;
        writer.read_path(root, &path)?;
        tokio::fs::read_to_string(path).await?
    } else {
        String::new()
    };
    let types = property_types(root).await?;
    for (key, value) in props {
        body = property_text(&body, &key, Some(normalize_property(&key, value, &types)))?;
    }
    for i in 0..10_000 {
        let filename = if i == 0 {
            format!("{stem}.md")
        } else {
            format!("{stem} {i}.md")
        };
        let path = parent.join(filename);
        let _guard = lock(&path).await;
        if path.exists() || writer.proposed_text(root, &path)?.is_some() {
            continue;
        }
        let result = writer
            .put(root, &path, body.clone(), ExpectedBase::Absent)
            .await?;
        ensure!(result["ok"] == true, "Entry creation refused: {result}");
        let mut result = result;
        result["path"] = json!(path.strip_prefix(root)?.to_string_lossy());
        if result["status"] != "proposed" {
            result["ancestor_hash"] = json!(disk_hash(&body));
        }
        return Ok(result);
    }
    anyhow::bail!("No unused entry name")
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

#[cfg(test)]
pub(super) async fn reorder_groups(root: &Path, params: &Json) -> Result<Json> {
    reorder_groups_with(root, params, &super::disposition::Writer::default()).await
}
pub(super) async fn reorder_groups_with(
    root: &Path,
    params: &Json,
    writer: &super::disposition::Writer<'_>,
) -> Result<Json> {
    let source: Source = serde_json::from_value(params["source"].clone())?;
    let path = match &source {
        Source::Path { path } => source_path(root, path)?,
        Source::Inline { .. } => contained(
            root,
            params["this"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Inline writes require the host note"))?,
        )?,
    };
    let hash = params["ancestor_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("ancestor_hash required"))?;
    let _guard = lock(&path).await;
    let text = tokio::fs::read_to_string(&path).await?;
    if disk_hash(&text) != hash {
        return Ok(json!({"ok":false,"current_hash":disk_hash(&text)}));
    }
    let range = match &source {
        Source::Inline { yaml } => Some(super::inline::range(&path, &text, yaml).await?),
        _ => None,
    };
    let mut base = BaseFile::parse(range.as_ref().map(|r| &text[r.clone()]).unwrap_or(&text))?;
    let index = match params["view"].as_str() {
        Some(name) => base.views.iter().position(|v| v.name == name),
        None => (!base.views.is_empty()).then_some(0),
    }
    .ok_or_else(|| anyhow::anyhow!("View not found"))?;
    base.views[index].group_order = serde_json::from_value(
        params
            .get("group_order")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("group_order required"))?,
    )?;
    let yaml = base.to_yaml()?;
    let content = if let Some(range) = range {
        let yaml = if text.contains("\r\n") {
            yaml.replace('\n', "\r\n")
        } else {
            yaml
        };
        format!("{}{}{}", &text[..range.start], yaml, &text[range.end..])
    } else {
        yaml
    };
    writer
        .put(
            root,
            &path,
            content,
            ExpectedBase::Hash { hash: hash.into() },
        )
        .await
}
pub(super) async fn move_entry(
    root: &Path,
    params: &Json,
    km: &std::sync::Arc<crate::kiln_manager::KilnManager>,
    writer: &super::disposition::Writer<'_>,
) -> Result<Json> {
    let from = params["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    let from = contained(root, from)?;
    let folder = params["value"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Folder must be text"))?;
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
    let hash = params["ancestor_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("ancestor_hash required"))?;
    let mut paths = vec![from.clone(), to.clone()];
    paths.sort();
    paths.dedup();
    let mut guards = vec![];
    for path in paths {
        guards.push(lock(&path).await)
    }
    let current = file_hash(&from).await?;
    if current != hash {
        return Ok(json!({"ok":false,"current_hash":current}));
    }
    if from == to {
        return Ok(json!({"ok":true,"path":from.strip_prefix(root)?.to_string_lossy()}));
    }
    let _order = lock(&root.join(".crucible-bases-writes")).await;
    let content = if crucible_core::kiln::is_note_file(&from) {
        Some(tokio::fs::read_to_string(&from).await?)
    } else {
        None
    };
    let payload = writer.before(root, &to, &from, content.as_deref()).await?;
    let from_rel = from.strip_prefix(root)?.to_string_lossy();
    let to_rel = to.strip_prefix(root)?.to_string_lossy();
    if crucible_core::kiln::is_indexable_file(&from) {
        crate::server::note_refactor::rename_note(km, root, &from_rel, &to_rel).await?;
    } else {
        crate::server::fs::move_within(root, &from_rel, &to_rel)?;
    }
    writer.changed(payload);
    Ok(json!({"ok":true,"path":to_rel,"ancestor_hash":current}))
}
