//! Thin terminal client for daemon-owned Obsidian Bases.
use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use crucible_daemon::bases::{QueryResult, Row};
use serde_json::json;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Format {
    Table,
    Json,
    /// Typed daemon query result, including groups and ancestor hashes.
    Data,
    Csv,
    Tsv,
    Md,
    Paths,
}
#[derive(Debug, Subcommand)]
pub enum BaseCommands {
    /// List saved .base files.
    List {
        #[arg(long)]
        kiln: Option<String>,
    },
    /// List the named views in a base.
    Views {
        file: String,
        #[arg(long)]
        kiln: Option<String>,
    },
    /// Evaluate a base in the daemon.
    Query {
        file: String,
        #[arg(long)]
        kiln: Option<String>,
        #[arg(long)]
        view: Option<String>,
        #[arg(long = "this")]
        host: Option<String>,
        #[arg(long, value_enum, default_value = "table")]
        format: Format,
    },
    /// Create a note using the base and view filters.
    Create {
        file: String,
        #[arg(long)]
        kiln: Option<String>,
        #[arg(long)]
        view: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        group: Option<String>,
    },
    /// Set a frontmatter property using the hash returned by query.
    Set {
        note: String,
        key: String,
        value: Option<String>,
        #[arg(long)]
        kiln: Option<String>,
        #[arg(long)]
        ancestor_hash: String,
        #[arg(long, conflicts_with = "value")]
        delete: bool,
    },
}
pub async fn handle(cmd: BaseCommands) -> Result<()> {
    let client = crate::common::daemon_client().await?;
    let (method, params, format) = match cmd {
        BaseCommands::List { kiln } => ("base.list", json!({"kiln":kiln}), None),
        BaseCommands::Views { file, kiln } => (
            "base.views",
            json!({"kiln":kiln,"source":{"path":file}}),
            None,
        ),
        BaseCommands::Query {
            file,
            kiln,
            view,
            host,
            format,
        } => (
            "base.query",
            json!({"kiln":kiln,"source":{"path":file},"view":view,"this":host}),
            Some(format),
        ),
        BaseCommands::Create {
            file,
            kiln,
            view,
            name,
            content,
            group,
        } => {
            let mut params = json!({"kiln":kiln,"source":{"path":file},"view":view,"name":name,"content":content});
            if let Some(g) = group {
                params["group"] = serde_json::from_str(&g).unwrap_or(json!(g));
            }
            ("base.create_entry", params, None)
        }
        BaseCommands::Set {
            note,
            key,
            value,
            kiln,
            ancestor_hash,
            delete,
        } => (
            "base.set_property",
            json!({"kiln":kiln,"path":note,"key":key,"value":value.map(|v|serde_json::from_str(&v).unwrap_or(json!(v))),"ancestor_hash":ancestor_hash,"delete":delete}),
            None,
        ),
    };
    let result = client.call(method, params).await?;
    anyhow::ensure!(
        result.get("ok") != Some(&json!(false)),
        "Base write refused: {result}"
    );
    if let Some(format) = format {
        let result: QueryResult = serde_json::from_value(result)?;
        print!("{}", render(&result, format)?)
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?)
    }
    Ok(())
}
fn render(result: &QueryResult, format: Format) -> Result<String> {
    if matches!(format, Format::Data) {
        return Ok(serde_json::to_string_pretty(result)? + "\n");
    }
    if matches!(format, Format::Json) {
        let rows = result
            .rows
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                map.insert("path".into(), json!(row.path));
                for column in &result.columns {
                    let value = row
                        .values
                        .get(&column.property)
                        .filter(|v| **v != crucible_core::bases::Value::Null)
                        .map(|v| v.text())
                        .filter(|s| !s.is_empty());
                    map.insert(column.display_name.clone(), json!(value));
                }
                map
            })
            .collect::<Vec<_>>();
        return Ok(serde_json::to_string_pretty(&rows)? + "\n");
    }
    if matches!(format, Format::Paths) {
        return Ok(result.rows.iter().map(|r| r.path.clone() + "\n").collect());
    }
    let mut out = String::new();
    let data_rows = |rows: &[Row]| {
        let mut data = vec![result
            .columns
            .iter()
            .map(|c| c.display_name.clone())
            .collect::<Vec<_>>()];
        data.extend(rows.iter().map(|r| {
            result
                .columns
                .iter()
                .map(|c| {
                    r.values
                        .get(&c.property)
                        .filter(|v| **v != crucible_core::bases::Value::Null)
                        .map(|v| v.text())
                        .unwrap_or_default()
                })
                .collect()
        }));
        data
    };
    let table = |rows: &[Row]| {
        let data = data_rows(rows);
        if matches!(format, Format::Table) && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            return crate::output::records_table(
                &data[0].iter().map(String::as_str).collect::<Vec<_>>(),
                &data[1..],
            ) + "\n";
        }
        let separator = match format {
            Format::Csv => ",",
            Format::Md => " | ",
            _ => "\t",
        };
        let escape = |s: &str| match format {
            Format::Csv if s.contains([',', '"', '\n', '\r']) => {
                format!("\"{}\"", s.replace('"', "\"\""))
            }
            Format::Csv => s.to_owned(),
            Format::Md => s.replace('|', "\\|").replace('\n', "<br>"),
            _ => s.replace(['\t', '\n', '\r'], " "),
        };
        if matches!(format, Format::Md) {
            let data = data
                .iter()
                .map(|row| row.iter().map(|s| escape(s)).collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let widths = (0..data[0].len())
                .map(|i| {
                    data.iter()
                        .map(|r| r[i].chars().count())
                        .max()
                        .unwrap_or(3)
                        .max(3)
                })
                .collect::<Vec<_>>();
            let line = |row: &[String], center: bool| {
                format!(
                    "| {} |",
                    row.iter()
                        .zip(&widths)
                        .map(|(s, w)| {
                            let padding = w.saturating_sub(s.chars().count());
                            let left = if center { padding / 2 } else { 0 };
                            format!("{}{}{}", " ".repeat(left), s, " ".repeat(padding - left))
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            };
            let mut lines = vec![
                line(&data[0], false),
                format!(
                    "| {} |",
                    widths
                        .iter()
                        .map(|w| "-".repeat(*w))
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
            ];
            lines.extend(data.iter().skip(1).map(|row| line(row, true)));
            return lines.join("\n") + "\n";
        }
        let mut lines = vec![];
        for (i, row) in data.iter().enumerate() {
            lines.push(
                row.iter()
                    .map(|s| escape(s))
                    .collect::<Vec<_>>()
                    .join(separator),
            );
            if i == 0 && matches!(format, Format::Md) {
                lines.push(vec!["---"; row.len()].join(separator))
            }
        }
        lines.join("\n") + "\n"
    };
    if matches!(format, Format::Table) && !result.groups.is_empty() {
        for g in &result.groups {
            out.push_str(&format!(
                "{}\n{}\n",
                if g.value.empty() {
                    "No value".into()
                } else {
                    g.value.text()
                },
                table(&g.rows)
            ))
        }
    } else {
        out = table(&result.rows)
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bases_cli_formats_match_obsidian_reference() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../assets/fixtures/bases/obsidian-1.14.2-queries.json"
        ))
        .unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        for (path, text) in corpus["files"].as_object().unwrap() {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap()).unwrap();
        }
        for case in corpus["cases"].as_array().unwrap() {
            let request = crucible_daemon::bases::Query {
                kiln: "test".into(),
                source: crucible_daemon::bases::Source::Inline {
                    yaml: serde_json::to_string(&case["base"]).unwrap(),
                },
                view: None,
                host: None,
            };
            let result = crucible_daemon::bases::query(dir.path(), &request)
                .await
                .unwrap();
            for (name, format) in [
                ("json", Format::Json),
                ("csv", Format::Csv),
                ("tsv", Format::Tsv),
                ("md", Format::Md),
                ("paths", Format::Paths),
            ] {
                let expected = case["formats"][name]["output"].as_str().unwrap();
                let output = render(&result, format).unwrap();
                if name == "json" {
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&output).unwrap(),
                        serde_json::from_str::<serde_json::Value>(expected).unwrap(),
                        "{} {name}",
                        case["id"]
                    );
                } else {
                    assert_eq!(
                        output.trim_end(),
                        expected.trim_end(),
                        "{} {name}",
                        case["id"]
                    );
                }
            }
        }
    }
}
