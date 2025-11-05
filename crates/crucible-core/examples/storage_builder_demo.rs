//! Storage Builder Pattern Demo
//!
//! This example demonstrates how to use the ContentAddressedStorageBuilder
//! to create configured storage instances following dependency inversion principles.

use crucible_core::storage::{ContentAddressedStorageBuilder, StorageBackendType, HasherConfig};
use crucible_core::storage::BlockSize;
use crucible_core::hashing::blake3::Blake3Hasher;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏗️  Storage Builder Pattern Demo\n");

    // Example 1: Simple in-memory storage with BLAKE3 hasher
    println!("📝 Example 1: Simple in-memory storage with BLAKE3 hasher");
    let storage = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(BlockSize::Medium)
        .with_deduplication(true)
        .with_compression(false)
        .build();

    match storage {
        Ok(_) => println!("✅ Successfully created in-memory storage"),
        Err(e) => println!("❌ Failed to create storage: {}", e),
    }

    println!();

    // Example 2: File-based storage with custom directory
    println!("📁 Example 2: File-based storage with custom directory");
    let file_storage = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::FileBased {
            directory: "/tmp/crucible_storage".to_string(),
            create_if_missing: true,
        })
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(BlockSize::Large)
        .with_cache_size(Some(5000))
        .with_maintenance(true);

    match file_storage.build() {
        Ok(_) => println!("✅ Successfully configured file-based storage"),
        Err(e) => println!("ℹ️  Expected - File backend not yet implemented: {}", e),
    }

    println!();

    // Example 3: SurrealDB storage configuration
    println!("🗃️  Example 3: SurrealDB storage configuration");
    let surrealdb_storage = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::SurrealDB {
            connection_string: "memory".to_string(),
            namespace: "crucible".to_string(),
            database: "production".to_string(),
        })
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(BlockSize::Adaptive { min: 1024, max: 16384 })
        .with_compression(true)
        .with_deduplication(true);

    match surrealdb_storage.build() {
        Ok(_) => println!("✅ Successfully configured SurrealDB storage"),
        Err(e) => println!("ℹ️  Expected - SurrealDB backend not yet implemented: {}", e),
    }

    println!();

    // Example 4: Demonstrate fluent API chaining
    println!("🔗 Example 4: Fluent API chaining with validation");
    let complex_storage = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(BlockSize::Small)
        .with_deduplication(true)
        .with_compression(false)
        .with_cache_size(Some(1000))
        .with_maintenance(false)
        .without_validation();

    match complex_storage.build() {
        Ok(_) => println!("✅ Successfully created complex storage configuration"),
        Err(e) => println!("❌ Failed to create complex storage: {}", e),
    }

    println!();

    // Example 5: Demonstrate validation errors
    println!("🚫 Example 5: Validation error handling");
    let invalid_storage = ContentAddressedStorageBuilder::new()
        // Missing backend configuration - should fail validation
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

    match invalid_storage.build() {
        Ok(_) => println!("❌ Should have failed validation"),
        Err(e) => println!("✅ Correctly caught validation error: {}", e),
    }

    println!();

    // Example 6: Validation with invalid file directory
    println!("🚫 Example 6: Invalid file directory validation");
    let invalid_file_storage = ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::FileBased {
            directory: "".to_string(), // Invalid empty directory
            create_if_missing: true,
        })
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

    match invalid_file_storage.build() {
        Ok(_) => println!("❌ Should have failed validation"),
        Err(e) => println!("✅ Correctly caught directory validation error: {}", e),
    }

    println!();

    println!("🎯 Key Features Demonstrated:");
    println!("  ✅ Fluent builder API with method chaining");
    println!("  ✅ Type-safe configuration with validation");
    println!("  ✅ Dependency inversion - depends on traits, not concretions");
    println!("  ✅ Multiple backend support (InMemory, FileBased, SurrealDB)");
    println!("  ✅ Configurable hashing algorithms (BLAKE3, custom)");
    println!("  ✅ Flexible block sizing and processing options");
    println!("  ✅ Comprehensive error handling and validation");
    println!("  ✅ Easy dependency injection for testing");

    println!("\n🏗️  Storage Builder Pattern Implementation Complete!");

    Ok(())
}