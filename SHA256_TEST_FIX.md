# SHA256 FileHasher Test Fix

## Issue Summary

The `test_hash_file_sha256` test in `/home/moot/crucible/crates/crucible-core/src/hashing/file_hasher.rs` was failing with incorrect hash expectations.

## Root Cause Analysis

The test expectation was **INCORRECT**, not the implementation. The issue stemmed from how the expected hash was generated.

### The Problem

The expected hash `ae97eca8f8ae1672bcc5c79e3fbafd8ee86f65f775e2250a291d3788b7a8af95` was likely generated using:

```bash
bash -c 'echo -n "Hello, World!" | sha256sum'
```

Due to **bash history expansion**, the exclamation mark `!` in `"Hello, World!"` gets escaped to `\!`, resulting in the command actually hashing the string **"Hello, World\!"** (14 bytes with backslash) instead of **"Hello, World!"** (13 bytes).

### Verification

```bash
# This hashes "Hello, World\!" (14 bytes) - WRONG
bash -c 'echo -n "Hello, World!" | sha256sum'
# Output: ae97eca8f8ae1672bcc5c79e3fbafd8ee86f65f775e2250a291d3788b7a8af95

# This hashes "Hello, World!" (13 bytes) - CORRECT
printf 'Hello, World!' | sha256sum
# Output: dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f
```

Python verification:
```python
import hashlib

# With backslash (what bash produced)
hashlib.sha256(b'Hello, World\!').hexdigest()
# 'ae97eca8f8ae1672bcc5c79e3fbafd8ee86f65f775e2250a291d3788b7a8af95'

# Without backslash (what the test actually writes)
hashlib.sha256(b'Hello, World!').hexdigest()
# 'dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f'
```

## The Fix

Changed the expected hash in the test from the incorrect value to the correct SHA256 hash of "Hello, World!" (13 bytes):

```rust
// Before (INCORRECT)
let expected_hex = "ae97eca8f8ae1672bcc5c79e3fbafd8ee86f65f775e2250a291d3788b7a8af95";

// After (CORRECT)
let expected_hex = "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f";
```

## Implementation Status

The `FileHasher::hash_file_streaming` implementation is **working correctly**. The current approach:

1. Reads file in chunks using a buffered reader
2. Collects all data into a Vec
3. Hashes the complete data using the generic `HashingAlgorithm` trait

This is correct and produces the expected results. While not using true streaming hashing (which would require changing the `HashingAlgorithm` trait to support incremental updates), it works correctly for the current design.

## Test Results

After the fix, all tests pass:

```
test hashing::file_hasher::tests::test_hash_file_sha256 ... ok
test hashing::file_hasher::tests::test_hash_file_blake3 ... ok
test hashing::file_hasher::tests::test_hash_block ... ok
test hashing::file_hasher::tests::test_hash_files_batch ... ok
test hashing::file_hasher::tests::test_hash_file_info ... ok
test hashing::file_hasher::tests::test_verify_file_hash ... ok
test hashing::file_hasher::tests::test_verify_block_hash ... ok
test hashing::file_hasher::tests::test_constants ... ok
test hashing::file_hasher::tests::test_error_handling ... ok

test result: ok. 9 passed; 0 failed; 0 ignored
```

## Lessons Learned

1. **Shell quoting matters**: When generating test expectations using shell commands, be aware of special character handling (especially `!` in bash with history expansion enabled)
2. **Verify expectations independently**: Always verify cryptographic hash test vectors using multiple independent methods
3. **Document test vectors**: Include comments explaining how test vectors were generated to prevent similar issues

## Recommendations

For future hash test vectors, use one of these safer methods:

```bash
# Method 1: printf (recommended)
printf 'Hello, World!' | sha256sum

# Method 2: Python
python3 -c "import hashlib; print(hashlib.sha256(b'Hello, World!').hexdigest())"

# Method 3: Hex bytes
printf '\x48\x65\x6c\x6c\x6f\x2c\x20\x57\x6f\x72\x6c\x64\x21' | sha256sum
```

Avoid using `echo` in bash with strings containing special characters like `!`, `$`, `\`, etc.
