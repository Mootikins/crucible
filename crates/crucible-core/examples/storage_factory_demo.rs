//! Storage Factory Demo
//!
//! This example demonstrates how to use the StorageFactory to create different
//! storage backends using configuration-driven selection.

use crucible_core::storage::{
    factory::{BackendConfig, HashAlgorithm, StorageConfig, StorageFactory},
    traits::StorageManagement,
    ContentAddressedStorage,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better visibility
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Storage Factory Demo ===\n");

    // Example 1: Create in-memory storage with default configuration
    println!("1. Creating in-memory storage with defaults...");
    let config1 = StorageConfig::in_memory(Some(100 * 1024 * 1024)); // 100MB
    let storage1 = StorageFactory::create(config1).await?;
    demo_storage_operations(&storage1, "InMemory Default").await?;

    // Example 2: Create in-memory storage with custom configuration
    println!("\n2. Creating in-memory storage with custom config...");
    let config2 = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(50 * 1024 * 1024), // 50MB
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        hash_algorithm: HashAlgorithm::Blake3,
        enable_deduplication: true,
        enable_maintenance: true,
        validate_config: true,
    };
    let storage2 = StorageFactory::create(config2).await?;
    demo_storage_operations(&storage2, "InMemory Custom").await?;

    // Example 3: Create storage from environment variables
    println!("\n3. Creating storage from environment...");
    std::env::set_var("STORAGE_BACKEND", "in_memory");
    std::env::set_var("STORAGE_MEMORY_LIMIT", "75000000");

    match StorageFactory::create_from_env().await {
        Ok(storage3) => {
            demo_storage_operations(&storage3, "InMemory from Env").await?;
        }
        Err(e) => {
            println!("   Failed to create from env: {}", e);
        }
    }

    // Clean up environment
    std::env::remove_var("STORAGE_BACKEND");
    std::env::remove_var("STORAGE_MEMORY_LIMIT");

    // Example 4: Demonstrate configuration validation
    println!("\n4. Demonstrating configuration validation...");
    let invalid_config = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(0), // Invalid!
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        ..Default::default()
    };

    match StorageFactory::create(invalid_config).await {
        Ok(_) => println!("   Unexpectedly succeeded with invalid config"),
        Err(e) => println!("   Correctly rejected invalid config: {}", e),
    }

    // Example 5: Use custom backend via dependency injection
    println!("\n5. Using custom backend via dependency injection...");
    use crucible_core::storage::memory::MemoryStorage;
    let custom_storage = Arc::new(MemoryStorage::new()) as Arc<dyn ContentAddressedStorage>;
    let config5 = StorageConfig::custom(custom_storage);
    let storage5 = StorageFactory::create(config5).await?;
    demo_storage_operations(&storage5, "Custom Injected").await?;

    // Example 6: Configuration serialization/deserialization
    println!("\n6. Configuration serialization...");
    let config = StorageConfig::in_memory(Some(200_000_000));
    let json = serde_json::to_string_pretty(&config)?;
    println!("   Serialized config:\n{}", json);

    let deserialized: StorageConfig = serde_json::from_str(&json)?;
    let storage6 = StorageFactory::create(deserialized).await?;
    demo_storage_operations(&storage6, "Deserialized Config").await?;

    // Example 7: Demonstrate different hash algorithms
    println!("\n7. Using different hash algorithms...");
    let config_blake3 = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(10_000_000),
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        hash_algorithm: HashAlgorithm::Blake3,
        ..Default::default()
    };
    let storage_blake3 = StorageFactory::create(config_blake3).await?;
    demo_storage_operations(&storage_blake3, "BLAKE3 Algorithm").await?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

/// Demonstrate basic storage operations
async fn demo_storage_operations(
    storage: &Arc<dyn ContentAddressedStorage>,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   Testing storage: {}", label);

    // Store some blocks
    let test_data = vec![
        ("hash1", b"Hello, World!".to_vec()),
        ("hash2", b"Content-addressed storage is awesome!".to_vec()),
        ("hash3", b"BLAKE3 is fast and secure.".to_vec()),
    ];

    for (hash, data) in &test_data {
        storage.store_block(hash, &data).await?;
        println!("     Stored block: {} ({} bytes)", hash, data.len());
    }

    // Retrieve blocks
    for (hash, expected_data) in &test_data {
        if let Some(data) = storage.get_block(hash).await? {
            assert_eq!(&data, expected_data);
            println!("     Retrieved block: {} ✓", hash);
        }
    }

    // Check existence
    assert!(storage.block_exists("hash1").await?);
    assert!(!storage.block_exists("nonexistent").await?);
    println!("     Existence checks: ✓");

    // Get storage statistics
    let stats = storage.get_stats().await?;
    println!("     Statistics:");
    println!("       - Block count: {}", stats.block_count);
    println!("       - Total size: {} bytes", stats.block_size_bytes);
    println!(
        "       - Avg block size: {:.2} bytes",
        stats.average_block_size
    );
    if stats.deduplication_savings > 0 {
        println!(
            "       - Dedup savings: {} bytes",
            stats.deduplication_savings
        );
    }

    // Perform maintenance
    storage.maintenance().await?;
    println!("     Maintenance completed: ✓");

    Ok(())
}
