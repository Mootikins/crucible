use anyhow::Result;
use std::fs;
use crate::config::CliConfig;

pub async fn execute(config: CliConfig) -> Result<()> {
    let kiln_path = &config.kiln.path;

    // Check if kiln path exists
    if !kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", kiln_path.display());
        eprintln!("Please set OBSIDIAN_KILN_PATH to a valid kiln directory.");
        return Err(anyhow::anyhow!("kiln path does not exist"));
    }

    let mut total_files = 0;
    let mut total_size = 0;
    let mut markdown_files = 0;

    // Simple file system scanning
    if let Ok(entries) = fs::read_dir(kiln_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                total_files += 1;
                if let Ok(metadata) = fs::metadata(&path) {
                    total_size += metadata.len();
                }
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        markdown_files += 1;
                    }
                }
            }
        }
    }

    println!("📊 Kiln Statistics\n");
    println!("📁 Total files: {}", total_files);
    println!("📝 Markdown files: {}", markdown_files);
    println!("💾 Total size: {} KB", total_size / 1024);
    println!("🗂️  Kiln path: {}", kiln_path.display());
    println!("\n✅ Kiln scan completed successfully.");

    Ok(())
}
