/// SQLite's shared little-endian embedding codec and blob-direct scoring.
/// Serialize an embedding vector to raw bytes (f32 little-endian)
pub(super) fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize raw bytes to an embedding vector
pub(super) fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    // `as_chunks` yields `[u8; 4]` directly, so there is no fallible
    // `try_into` and no `expect` on a length the slicing already guarantees.
    // A trailing partial chunk is not an embedding and is dropped.
    let (chunks, _partial) = bytes.as_chunks::<4>();
    chunks.iter().copied().map(f32::from_le_bytes).collect()
}

// ============================================================================
// Cosine Similarity (blob-direct)
// ============================================================================

/// Cosine similarity between the query and a raw embedding blob (f32 LE),
/// without deserializing the blob into a `Vec<f32>` first — the scan phase of
/// `search` scores every row and only materializes the k winners, so this is
/// the hot loop.
///
/// Accumulation order matches the classic slice-based cosine (dot, then the
/// two norms, each summed element-by-element), so scores are bit-identical to
/// the pre-tuning implementation. Returns 0.0 on dimension mismatch or zero
/// magnitude.
pub(super) fn cosine_similarity_blob(query: &[f32], blob: &[u8]) -> f32 {
    if query.is_empty() || blob.len() != query.len() * 4 {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for (chunk, q) in blob.as_chunks::<4>().0.iter().zip(query) {
        let v = f32::from_le_bytes(*chunk);
        dot += q * v;
        norm_b_sq += v * v;
    }
    let norm_a: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = norm_b_sq.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_serialize_deserialize_embedding() {
        let original = vec![1.0_f32, 2.5, -std::f32::consts::PI, 0.0, f32::MAX, f32::MIN];
        let bytes = serialize_embedding(&original);
        let restored = deserialize_embedding(&bytes);

        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    /// Score a query against a slice by serializing it first — the shape the
    /// production scan sees (raw blobs out of SQLite).
    fn blob_sim(a: &[f32], b: &[f32]) -> f32 {
        cosine_similarity_blob(a, &serialize_embedding(b))
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let sim = blob_sim(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let sim = blob_sim(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let sim = blob_sim(&[1.0, 0.0, 0.0], &[-1.0, 0.0, 0.0]);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = blob_sim(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let sim = blob_sim(&[1.0, 2.0], &[1.0, 2.0, 3.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_truncated_blob_scores_zero() {
        // A blob whose byte length is not a whole number of f32s (torn write)
        // must score 0, not panic or read garbage.
        let query = [1.0f32, 0.0, 0.0];
        let mut blob = serialize_embedding(&[1.0, 0.0, 0.0]);
        blob.pop();
        assert_eq!(cosine_similarity_blob(&query, &blob), 0.0);
    }

    #[test]
    fn zero_magnitude_scores_zero() {
        assert_eq!(blob_sim(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(blob_sim(&[1.0, 0.0], &[0.0, 0.0]), 0.0);
    }

    #[test]
    fn decoding_ignores_only_a_trailing_partial_float() {
        let mut bytes = serialize_embedding(&[1.0, -2.0]);
        bytes.extend([1, 2, 3]);
        assert_eq!(deserialize_embedding(&bytes), vec![1.0, -2.0]);
    }
}
