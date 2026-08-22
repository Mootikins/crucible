# ContentHasher Trait

This document describes the `ContentHasher` trait in `crucible-core`.

## Overview

The `ContentHasher` trait (`src/storage/traits.rs`) is an abstraction for content hashing. An
implementation supplies one hash algorithm. Callers that hold a `dyn ContentHasher` do not depend
on that algorithm.

The trait has three required methods:

- `hash_block(&self, data: &[u8]) -> String` returns the hash as a hex string.
- `algorithm_name(&self) -> &'static str` returns the algorithm name.
- `hash_length(&self) -> usize` returns the hash length in bytes.

The trait has one default method. `is_valid_hash(&self, hash: &str) -> bool` checks that the
string has the correct length and contains only hex digits.

## Related Types

The hash value types live in `src/types/hashing.rs`:

- `FileHash`: 32-byte hash for a file
- `BlockHash`: 32-byte hash for a content block
- `HashAlgorithm`: the algorithms the types can name
- `FileHashInfo` and `BlockHashInfo`: a hash with its metadata

## Status

The workspace has no implementation of `ContentHasher` outside the trait's own tests. The
`hashing/` module with the BLAKE3 and SHA256 implementations was deleted in Consolidation Plan
batch B17. Block hashing in the parser uses `parser/block_hasher.rs` directly.
