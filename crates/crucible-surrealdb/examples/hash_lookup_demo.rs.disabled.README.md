# Hash Lookup Demo - Disabled

This example has been disabled because it uses the old embedding architecture with `EmbeddingThreadPool`,
`KilnScannerConfig`, and `ChangeDetectionMethod`, which have been removed or refactored.

## Status

**NEEDS UPDATE** - This example requires updating to use the new architecture.

## What needs to change

1. Replace `EmbeddingThreadPool` with the new embedding provider system
2. Replace `KilnScannerConfig` with current configuration types
3. Replace `ChangeDetectionMethod` with current change detection API
4. Replace `create_kiln_scanner_with_embeddings` with current scanner initialization

## To re-enable

1. Update the code to use current APIs
2. Rename file back to `hash_lookup_demo.rs`
3. Test that it compiles and runs correctly
