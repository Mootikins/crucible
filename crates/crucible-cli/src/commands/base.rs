//! Thin terminal client for daemon-owned Obsidian Bases.
use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use crucible_daemon::bases::{QueryResult, Row};
use serde_json::json;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Format {
    Table,
    Json,
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
    if matches!(format, Format::Json) {
        return Ok(serde_json::to_string_pretty(result)? + "\n");
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
            Format::Csv => format!("\"{}\"", s.replace('"', "\"\"")),
            Format::Md => s.replace('|', "\\|").replace('\n', "<br>"),
            _ => s.replace(['\t', '\n', '\r'], " "),
        };
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
