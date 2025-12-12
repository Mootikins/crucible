//! Binary search to find the exact line that triggers markdown-it panic
//!
//! Usage: cargo run --example find_markdown_panic -- <file.md>

use markdown_it::MarkdownIt;
use std::panic::AssertUnwindSafe;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --example find_markdown_panic -- <file.md>");
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&args[1]).expect("Failed to read file");
    let lines: Vec<&str> = content.lines().collect();

    println!("Searching {} lines for panic trigger...", lines.len());

    // Binary search to find the problematic line
    let mut low = 0;
    let mut high = lines.len();

    while low < high {
        let mid = (low + high) / 2;
        let test_content = lines[..mid].join("\n");

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut md = MarkdownIt::new();
            markdown_it::plugins::cmark::add(&mut md);
            md.parse(&test_content);
        }));

        if result.is_err() {
            high = mid;
            eprintln!("  Panic with {} lines", mid);
        } else {
            low = mid + 1;
        }
    }

    if low > 0 && low <= lines.len() {
        println!("\n=== Problematic content ends at line: {} ===", low);
        let start = if low > 15 { low - 15 } else { 0 };
        for i in start..std::cmp::min(low + 3, lines.len()) {
            let marker = if i + 1 == low { ">>>" } else { "   " };
            println!("{} {:4}: {}", marker, i + 1, lines[i]);
        }
    } else {
        println!("No panic detected in file.");
    }
}
