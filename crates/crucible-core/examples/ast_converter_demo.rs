//! Demo of the ASTBlockConverter following SRP
//!
//! This example demonstrates how the new ASTBlockConverter separates
//! conversion concerns from hashing concerns, following the Single
//! Responsibility Principle.

use crucible_core::hashing::{ASTBlockConverter, Blake3Algorithm};
use crucible_parser::types::{ASTBlock, ASTBlockMetadata, ASTBlockType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== ASTBlockConverter Demo ===\n");

    // Create converter with BLAKE3 algorithm
    let converter = ASTBlockConverter::new(Blake3Algorithm);
    println!(
        "Created converter with algorithm: {}\n",
        converter.algorithm_name()
    );

    // Create some example AST blocks
    let blocks = vec![
        ASTBlock::new(
            ASTBlockType::Heading,
            "Introduction".to_string(),
            0,
            12,
            ASTBlockMetadata::heading(1, Some("intro".to_string())),
        ),
        ASTBlock::new(
            ASTBlockType::Paragraph,
            "This is the first paragraph with some content.".to_string(),
            15,
            62,
            ASTBlockMetadata::generic(),
        ),
        ASTBlock::new(
            ASTBlockType::Code,
            "fn main() {\n    println!(\"Hello, world!\");\n}".to_string(),
            65,
            115,
            ASTBlockMetadata::code(Some("rust".to_string()), 3),
        ),
    ];

    println!("Input: {} AST blocks\n", blocks.len());

    // Analyze the batch before conversion
    let stats = converter.analyze_batch(&blocks);
    println!("Batch statistics:");
    println!("  {}\n", stats.summary());

    // Convert AST blocks to HashedBlock format
    println!("Converting blocks...");
    let hashed_blocks = converter.convert_batch(&blocks).await?;

    println!("\nConverted {} blocks:\n", hashed_blocks.len());

    for (i, (ast_block, hashed_block)) in blocks.iter().zip(hashed_blocks.iter()).enumerate() {
        println!("Block {}:", i + 1);
        println!("  Type: {}", ast_block.type_name());
        println!(
            "  Hash: {}",
            &hashed_block.hash[0..16.min(hashed_block.hash.len())]
        ); // Show first 16 chars
        println!("  Index: {}", hashed_block.index);
        println!("  Offset: {}", hashed_block.offset);
        println!("  Content length: {} bytes", hashed_block.data.len());
        println!("  Is last: {}", hashed_block.is_last);
        println!();
    }

    println!("=== Demo Complete ===");

    Ok(())
}
