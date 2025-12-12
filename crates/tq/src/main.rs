//! tq - TOON Query
//!
//! A jq-like tool for TOON format. Reads TOON or JSON, applies jq filters,
//! outputs TOON or JSON.
//!
//! # Usage
//!
//! ```bash
//! # Convert JSON to TOON
//! echo '{"name": "Ada", "age": 30}' | tq
//!
//! # Convert TOON to JSON
//! echo 'name: Ada\nage: 30' | tq -j
//!
//! # Query TOON data with jq syntax
//! echo 'users[2]{name,age}:\n  Alice,30\n  Bob,25' | tq '.users[0].name'
//!
//! # Format tool response with smart content extraction
//! cat tool_response.json | tq --format read
//! ```

use clap::Parser;
use std::io::{self, Read, Write};
use tq::{
    command_formatter, compile_filter, parse_input, read_note_formatter, run_filter,
    search_formatter, CompiledFilter, ContentFormatter, InputFormat, OutputFormat, TqError,
};

#[derive(Parser, Debug)]
#[command(name = "tq")]
#[command(about = "TOON Query - jq-like tool for TOON format")]
#[command(version)]
struct Cli {
    /// jq filter expression (default: identity ".")
    #[arg(default_value = ".")]
    filter: String,

    /// Input files (default: stdin)
    #[arg(value_name = "FILE")]
    files: Vec<String>,

    /// Input format: auto, json, toon
    #[arg(short = 'i', long, default_value = "auto")]
    input_format: InputFormat,

    /// Output JSON instead of TOON
    #[arg(short = 'j', long)]
    json: bool,

    /// Output raw strings (no quotes)
    #[arg(short = 'r', long)]
    raw: bool,

    /// Compact output (single line)
    #[arg(short = 'c', long)]
    compact: bool,

    /// Read each line as separate input
    #[arg(short = 's', long)]
    slurp: bool,

    /// Null input (don't read any input)
    #[arg(short = 'n', long)]
    null_input: bool,

    /// Use smart formatter for tool responses (read, search, command)
    #[arg(short = 'f', long, value_name = "TYPE")]
    format: Option<FormatType>,

    /// Colorize output
    #[arg(long, default_value = "auto")]
    color: ColorOption,
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum ColorOption {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum FormatType {
    /// Format read_note tool responses
    Read,
    /// Format search tool responses
    Search,
    /// Format command/shell output
    Command,
    /// Auto-detect based on fields
    Auto,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("tq: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), TqError> {
    let cli = Cli::parse();

    // Configure color output
    match cli.color {
        ColorOption::Always => yansi::enable(),
        ColorOption::Never => yansi::disable(),
        ColorOption::Auto => {
            if !atty_check() {
                yansi::disable();
            }
        }
    }

    // Compile the filter
    let filter = compile_filter(&cli.filter)?;

    // Determine output format
    let output_format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Toon
    };

    // Get formatter if specified
    let formatter = cli.format.map(|ft| match ft {
        FormatType::Read => read_note_formatter(),
        FormatType::Search => search_formatter(),
        FormatType::Command => command_formatter(),
        FormatType::Auto => ContentFormatter::new().with_default_threshold(200),
    });

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if cli.null_input {
        // Process with null input
        let results = run_filter(&filter, serde_json::Value::Null)?;
        for value in results {
            output_value(
                &mut out,
                &value,
                output_format,
                &formatter,
                cli.raw,
                cli.compact,
            )?;
        }
    } else if cli.files.is_empty() {
        // Read from stdin
        let input = read_stdin()?;
        process_input(&mut out, &input, &cli, &filter, output_format, &formatter)?;
    } else {
        // Read from files
        for path in &cli.files {
            let input = std::fs::read_to_string(path)?;
            process_input(&mut out, &input, &cli, &filter, output_format, &formatter)?;
        }
    }

    Ok(())
}

fn process_input(
    out: &mut impl Write,
    input: &str,
    cli: &Cli,
    filter: &CompiledFilter,
    output_format: OutputFormat,
    formatter: &Option<ContentFormatter>,
) -> Result<(), TqError> {
    // Detect and parse input format
    let format = cli.input_format.detect(input);
    let value = parse_input(input, format)?;

    // Run the filter
    let results = run_filter(filter, value)?;

    // Output results
    for value in results {
        output_value(out, &value, output_format, formatter, cli.raw, cli.compact)?;
    }

    Ok(())
}

fn output_value(
    out: &mut impl Write,
    value: &serde_json::Value,
    format: OutputFormat,
    formatter: &Option<ContentFormatter>,
    raw: bool,
    compact: bool,
) -> Result<(), TqError> {
    // If formatter is specified, use it
    if let Some(fmt) = formatter {
        let formatted = fmt.format(value)?;
        writeln!(out, "{}", formatted)?;
        return Ok(());
    }

    if raw {
        // Raw string output
        if let serde_json::Value::String(s) = value {
            writeln!(out, "{}", s)?;
        } else {
            let s = match format {
                OutputFormat::Json => {
                    if compact {
                        serde_json::to_string(value)?
                    } else {
                        serde_json::to_string_pretty(value)?
                    }
                }
                OutputFormat::Toon => toon_format::encode_default(value)
                    .map_err(|e| TqError::ToonParse(e.to_string()))?,
            };
            writeln!(out, "{}", s)?;
        }
    } else {
        let s = match format {
            OutputFormat::Json => {
                if compact {
                    serde_json::to_string(value)?
                } else {
                    serde_json::to_string_pretty(value)?
                }
            }
            OutputFormat::Toon => {
                toon_format::encode_default(value).map_err(|e| TqError::ToonParse(e.to_string()))?
            }
        };
        writeln!(out, "{}", s)?;
    }
    Ok(())
}

fn read_stdin() -> Result<String, io::Error> {
    let mut input = String::new();
    io::stdin().lock().read_to_string(&mut input)?;
    Ok(input)
}

fn atty_check() -> bool {
    // Simple check - in a real implementation, use the atty crate
    std::env::var("TERM").is_ok()
}
