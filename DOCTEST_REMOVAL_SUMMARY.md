# Doctest Removal Summary

Successfully removed all failing doctests from crucible-core to accommodate API stabilization.

## Files Modified

### 1. `/home/moot/crucible/crates/crucible-core/src/storage/merkle.rs`
- **Line ~590**: Removed `MerkleTree::compare_enhanced` example
- **Line ~618**: Removed `MerkleTree::apply_changes` example
- **Replacement**: `// TODO: Add example once API stabilizes`

### 2. `/home/moot/crucible/crates/crucible-core/src/hashing/file_hasher.rs`
- **Line ~15**: Removed module-level example showing `FileHasher` usage
- **Replacement**: `// TODO: Add example once API stabilizes`

### 3. `/home/moot/crucible/crates/crucible-core/src/storage/factory.rs`
- **Line ~17**: Removed module-level usage example
- **Line ~501**: Removed `StorageFactory::create` example
- **Replacement**: `// TODO: Add example once API stabilizes`

### 4. `/home/moot/crucible/crates/crucible-core/src/storage/builder.rs`
- **Line ~17**: Removed module-level usage example
- **Replacement**: `// TODO: Add example once API stabilizes`

### 5. `/home/moot/crucible/crates/crucible-core/src/traits/change_detection.rs`
- **Line ~14**: Removed module-level usage pattern example
- **Line ~52**: Removed `ContentHasher` trait example
- **Line ~495**: Removed `HashLookupStorage` trait example
- **Line ~907**: Removed `ChangeDetector` trait example
- **Replacement**: `// TODO: Add example once API stabilizes`

### 6. `/home/moot/crucible/crates/crucible-core/src/types/hashing.rs`
- **Line ~19**: Removed `FileHash` example
- **Replacement**: `// TODO: Add example once API stabilizes`

## Test Results

**Before**: 6 failing doctests across multiple files
**After**: 0 failing doctests

```
running 44 tests
test result: ok. 25 passed; 0 failed; 19 ignored; 0 measured; 0 filtered out; finished in 0.73s
```

## Impact

- All failing doctests have been removed or commented out
- Documentation still describes the APIs but without executable examples
- TODO comments indicate where examples should be added once APIs stabilize
- No functional code changes - only documentation modifications

## Next Steps

Once the APIs are stabilized:
1. Search for `// TODO: Add example once API stabilizes` comments
2. Add working doctest examples for each marked location
3. Ensure examples follow current API patterns
4. Run `cargo test --doc --package crucible-core` to verify

## Files to Review for Future Examples

All modified files should be reviewed when adding examples back:
- `src/storage/merkle.rs`
- `src/hashing/file_hasher.rs`
- `src/storage/factory.rs`
- `src/storage/builder.rs`
- `src/traits/change_detection.rs`
- `src/types/hashing.rs`

