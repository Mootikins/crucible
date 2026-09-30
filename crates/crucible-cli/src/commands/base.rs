//! Thin terminal client for daemon-owned Obsidian Bases.
use crate::formatting::OutputFormat;
use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use crucible_core::protocol::RpcMethod;
use crucible_daemon::bases::{
    Column, CreateEntryParams, ListParams, QueryParams, QueryResult, Row, SetPropertyParams,
    Source, ViewSummary, ViewsParams, WriteOutcome,
};
use crucible_daemon::DaemonClient;
use serde::Serialize;
use serde_json::{json, Value};

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
        /// Output format. The default is a table on a terminal, else TSV.
        #[arg(long, value_enum)]
        format: Option<Format>,
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
        /// The group of the new entry: JSON, else text. `null` is the group
        /// of entries with no value.
        #[arg(long, value_parser = json_or_text)]
        group: Option<Value>,
    },
    /// Set a frontmatter property using the hash returned by query.
    Set {
        note: String,
        key: String,
        /// The new value: JSON, else text. `null` sets an empty property.
        #[arg(required_unless_present = "delete", value_parser = json_or_text)]
        value: Option<Value>,
        #[arg(long)]
        kiln: Option<String>,
        #[arg(long)]
        ancestor_hash: String,
        #[arg(long, conflicts_with = "value")]
        delete: bool,
    },
}

/// A command-line value as JSON when it parses, else as text, so that
/// `'"done"'`, `done`, `3` and `null` all mean what a person expects.
fn json_or_text(text: &str) -> Result<Value, std::convert::Infallible> {
    Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
}

pub async fn handle(cmd: BaseCommands) -> Result<()> {
    let client = crate::common::daemon_client().await?;
    match cmd {
        BaseCommands::List { kiln } => {
            let files: Vec<String> = client
                .call(RpcMethod::BaseList, ListParams { kiln })
                .await?;
            print_json(&files)
        }
        BaseCommands::Views { file, kiln } => {
            let views: Vec<ViewSummary> = client
                .call(
                    RpcMethod::BaseViews,
                    ViewsParams {
                        kiln,
                        source: Source::Path { path: file },
                    },
                )
                .await?;
            print_json(&views)
        }
        BaseCommands::Query {
            file,
            kiln,
            view,
            host,
            format,
        } => {
            let result: QueryResult = client
                .call(
                    RpcMethod::BaseQuery,
                    QueryParams {
                        kiln,
                        source: Source::Path { path: file },
                        view,
                        host,
                    },
                )
                .await?;
            let format = format.unwrap_or(match OutputFormat::for_stdout(None) {
                OutputFormat::Table => Format::Table,
                OutputFormat::Json | OutputFormat::Plain => Format::Tsv,
            });
            print!("{}", render(&result, format)?);
            Ok(())
        }
        BaseCommands::Create {
            file,
            kiln,
            view,
            name,
            content,
            group,
        } => {
            write(
                &client,
                crucible_core::protocol::RpcMethod::BaseCreateEntry,
                CreateEntryParams {
                    kiln,
                    source: Source::Path { path: file },
                    view,
                    name,
                    content,
                    group,
                },
            )
            .await
        }
        BaseCommands::Set {
            note,
            key,
            value,
            kiln,
            ancestor_hash,
            delete,
        } => {
            write(
                &client,
                crucible_core::protocol::RpcMethod::BaseSetProperty,
                SetPropertyParams {
                    kiln,
                    path: note,
                    key,
                    value,
                    delete,
                    ancestor_hash,
                },
            )
            .await
        }
    }
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Send one Bases write. A stale or refused write prints its outcome to
/// stderr and fails, so that a script sees that the disk did not change.
async fn write(
    client: &DaemonClient,
    method: crucible_core::protocol::RpcMethod,
    params: impl Serialize,
) -> Result<()> {
    let outcome: WriteOutcome = client.call(method, params).await?;
    let text = serde_json::to_string_pretty(&outcome)?;
    match outcome {
        WriteOutcome::Applied { .. }
        | WriteOutcome::Unchanged { .. }
        | WriteOutcome::Proposed { .. } => {
            println!("{text}");
            Ok(())
        }
        WriteOutcome::Stale { path, .. } => {
            eprintln!("{text}");
            anyhow::bail!("{path} changed since it was read; query the base again")
        }
        WriteOutcome::Refused { path, reason } => {
            eprintln!("{text}");
            anyhow::bail!("The write to {path} was refused: {reason}")
        }
    }
}

/// The text of one cell: empty for an absent or null value.
fn cell(row: &Row, column: &Column) -> String {
    row.values
        .get(&column.property)
        .filter(|v| **v != crucible_core::bases::BaseValue::Null)
        .map(crucible_core::bases::BaseValue::text)
        .unwrap_or_default()
}

fn render(result: &QueryResult, format: Format) -> Result<String> {
    let header = result
        .columns
        .iter()
        .map(|c| c.display_name.clone())
        .collect::<Vec<_>>();
    let cells = |rows: &[Row]| {
        rows.iter()
            .map(|row| result.columns.iter().map(|c| cell(row, c)).collect())
            .collect::<Vec<Vec<String>>>()
    };
    Ok(match format {
        Format::Data => serde_json::to_string_pretty(result)? + "\n",
        Format::Json => {
            let rows = result
                .rows
                .iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    map.insert("path".into(), json!(row.path));
                    for column in &result.columns {
                        let text = Some(cell(row, column)).filter(|s| !s.is_empty());
                        map.insert(column.display_name.clone(), json!(text));
                    }
                    map
                })
                .collect::<Vec<_>>();
            serde_json::to_string_pretty(&rows)? + "\n"
        }
        Format::Paths => result.rows.iter().map(|r| r.path.clone() + "\n").collect(),
        Format::Table if !result.groups.is_empty() => result
            .groups
            .iter()
            .map(|g| {
                let label = if g.value.empty() {
                    "No value".into()
                } else {
                    g.value.text()
                };
                format!("{label}\n{}\n", table(&header, &cells(&g.rows), format))
            })
            .collect(),
        _ => table(&header, &cells(&result.rows), format),
    })
}

/// One table of `rows` under `header`, in a tabular `format`.
fn table(header: &[String], rows: &[Vec<String>], format: Format) -> String {
    let escape = |s: &str| match format {
        Format::Csv if s.contains([',', '"', '\n', '\r']) => {
            format!("\"{}\"", s.replace('"', "\"\""))
        }
        Format::Csv => s.to_owned(),
        Format::Md => s.replace('|', "\\|").replace('\n', "<br>"),
        _ => s.replace(['\t', '\n', '\r'], " "),
    };
    let escaped = std::iter::once(header)
        .chain(rows.iter().map(Vec::as_slice))
        .map(|row| row.iter().map(|s| escape(s)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    match format {
        Format::Table => {
            crate::output::records_table(
                &header.iter().map(String::as_str).collect::<Vec<_>>(),
                rows,
            ) + "\n"
        }
        Format::Md => markdown(&escaped),
        _ => {
            let separator = if matches!(format, Format::Csv) {
                ","
            } else {
                "\t"
            };
            escaped
                .iter()
                .map(|row| row.join(separator) + "\n")
                .collect()
        }
    }
}

/// A Markdown table with padded columns and centred body cells, as Obsidian
/// copies it. `data` holds the header row first.
fn markdown(data: &[Vec<String>]) -> String {
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
        let cells = row
            .iter()
            .zip(&widths)
            .map(|(s, w)| {
                let padding = w.saturating_sub(s.chars().count());
                let left = if center { padding / 2 } else { 0 };
                format!("{}{}{}", " ".repeat(left), s, " ".repeat(padding - left))
            })
            .collect::<Vec<_>>();
        format!("| {} |\n", cells.join(" | "))
    };
    let rule = widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>();
    std::iter::once(line(&data[0], false))
        .chain(std::iter::once(format!("| {} |\n", rule.join(" | "))))
        .chain(data[1..].iter().map(|row| line(row, true)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bases_cli_render_matches_obsidian_reference_formats() {
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
