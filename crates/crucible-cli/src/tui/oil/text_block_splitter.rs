//! Streaming text block splitting and merging.
//!
//! Splits incoming LLM text deltas into paragraph blocks on `\n\n` boundaries,
//! while merging ordered list continuations and preserving code blocks.
//!
//! Used by `ContainerList::append_text` to accumulate streaming content into
//! discrete text blocks for incremental rendering and graduation.

/// Check if a text part starts with an ordered list item `N. ` where N > 1.
/// Used to merge counting-up list items that span text block boundaries.
fn starts_with_ordered_list_item(s: &str) -> bool {
    ordered_list_start_number(s).is_some_and(|n| n > 1)
}

/// Extract the number from an ordered list item at the start of a string.
/// Returns `Some(n)` for strings like "3. foo", `None` for non-list text.
fn ordered_list_start_number(s: &str) -> Option<u32> {
    let trimmed = s.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return None;
    }
    if let Some(dot_pos) = trimmed.find(". ") {
        let prefix = &trimmed[..dot_pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            return prefix.parse::<u32>().ok();
        }
    }
    None
}

/// Extract the ordered list number from the last line of a block.
/// Returns `Some(n)` if the block ends with "N. ...", `None` otherwise.
fn last_ordered_list_number(s: &str) -> Option<u32> {
    let last_line = s.lines().last()?;
    ordered_list_start_number(last_line)
}

/// Check if a block ends with an ordered list item.
fn ends_with_ordered_list_item(s: &str) -> bool {
    last_ordered_list_number(s).is_some()
}

/// Check if `part` is a lazy list continuation of `prev`.
/// Lazy numbering: all items use `1.`, so `1.` after `1.` should merge.
fn is_lazy_list_continuation(part: &str, prev: &str) -> bool {
    ordered_list_start_number(part) == Some(1) && last_ordered_list_number(prev) == Some(1)
}

/// Check whether `current` should merge into `previous` (list continuation or unclosed fence).
///
/// Covers three cases:
/// 1. Counting-up ordered list: current starts with N>1 and previous ends with a list item
/// 2. Lazy ordered list: both start/end with `1.`
/// 3. Unclosed code fence in previous block
fn should_merge_blocks(current: &str, previous: &str) -> bool {
    (starts_with_ordered_list_item(current) && ends_with_ordered_list_item(previous))
        || is_lazy_list_continuation(current, previous)
        || has_unclosed_fence(previous)
}

/// Check if a text block has an unclosed code fence.
///
/// Scans lines for fence markers (``` or ~~~). An odd count means the last fence
/// was an opening marker with no matching close — the block is mid-code-block.
fn has_unclosed_fence(s: &str) -> bool {
    let mut inside_fence = false;
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            inside_fence = !inside_fence;
        }
    }
    inside_fence
}

/// Split an incoming text delta into paragraph blocks, merging with existing blocks.
///
/// Handles:
/// - `\n\n` paragraph splitting
/// - Ordered list continuation merging (keeps "1. ...\n\n2. ..." in one block)
/// - Full-text resend detection (some LLM backends re-emit the full response)
/// - Empty placeholder management (trailing `\n\n` pushes a placeholder for next delta)
pub(super) fn split_and_merge_text_delta(existing: &[String], delta: &str) -> Vec<String> {
    let mut blocks: Vec<String> = existing.to_vec();
    let parts: Vec<&str> = delta.split("\n\n").collect();

    if blocks.is_empty() {
        // First content — build blocks from parts
        if let Some((first, rest)) = parts.split_first() {
            if !first.is_empty() {
                blocks.push(first.to_string());
            }
            push_parts_merging_lists(&mut blocks, rest);
            // Trailing \n\n means next delta starts fresh — but not inside a code fence
            if delta.ends_with("\n\n")
                && !blocks.is_empty()
                && !blocks.last().map_or(true, |b| has_unclosed_fence(b) || b.is_empty())
            {
                blocks.push(String::new());
            }
        }
    } else if parts.len() == 1 {
        // No separator — append to current block
        if try_merge_list_across_placeholder(&mut blocks, delta) {
            // merged
        } else if let Some(last) = blocks.last_mut() {
            if last.is_empty() {
                *last = delta.to_string();
            } else if is_full_text_resend(last, delta) {
                tracing::debug!(
                    incoming_len = delta.len(),
                    existing_len = last.len(),
                    "Skipping duplicate full-text delta"
                );
            } else {
                last.push_str(delta);
            }
        }
        // Retroactive merge: if tokens arrived one at a time ("2", ".", " X"),
        // the current block may now form a list item. Merge it back if so.
        try_retroactive_list_merge(&mut blocks);
    } else {
        // Has separator(s) — append first part to current, create new blocks for rest
        if let Some((first, rest)) = parts.split_first() {
            append_first_part(&mut blocks, first);
            push_parts_merging_lists(&mut blocks, rest);
            // Trailing \n\n means next delta starts fresh — but not inside a code fence
            if delta.ends_with("\n\n")
                && !blocks.is_empty()
                && !blocks.last().map_or(true, |b| has_unclosed_fence(b) || b.is_empty())
            {
                blocks.push(String::new());
            }
        }
    }

    blocks
}

/// Append parts to blocks, merging ordered list continuations and code fence
/// interiors with the previous block.
fn push_parts_merging_lists(blocks: &mut Vec<String>, parts: &[&str]) {
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let should_merge = blocks.last().is_some_and(|prev| should_merge_blocks(part, prev));
        if should_merge {
            if let Some(last) = blocks.last_mut() {
                last.push_str("\n\n");
                last.push_str(part);
            }
        } else {
            blocks.push(part.to_string());
        }
    }
}

/// Try to merge across an empty placeholder for list continuations and unclosed fences.
/// Returns true if merged.
fn try_merge_list_across_placeholder(blocks: &mut Vec<String>, delta: &str) -> bool {
    if blocks.len() >= 2 && blocks.last().map(|b| b.is_empty()).unwrap_or(false) {
        let prev = &blocks[blocks.len() - 2];
        if should_merge_blocks(delta, prev) {
            blocks.pop();
            if let Some(prev) = blocks.last_mut() {
                prev.push_str("\n\n");
                prev.push_str(delta);
            }
            return true;
        }
    }
    false
}

/// Retroactively merge the last block into the one before it if it forms a list continuation.
///
/// Defensive: handles cases where tokens arrive individually ("2", ".", " Item")
/// and the block only becomes recognizable as a list item after accumulation.
fn try_retroactive_list_merge(blocks: &mut Vec<String>) {
    if blocks.len() < 2 {
        return;
    }
    let current = &blocks[blocks.len() - 1];
    let prev = &blocks[blocks.len() - 2];

    // Note: retroactive merge only applies to list continuations, not unclosed fences.
    // Unclosed fences are handled at delta-arrival time, not retroactively.
    let should_merge = (starts_with_ordered_list_item(current)
        && ends_with_ordered_list_item(prev))
        || is_lazy_list_continuation(current, prev);

    if should_merge {
        let current = blocks.pop().unwrap();
        if let Some(prev) = blocks.last_mut() {
            prev.push_str("\n\n");
            prev.push_str(&current);
        }
    }
}

/// Append the first part of a multi-part delta to the current blocks.
fn append_first_part(blocks: &mut Vec<String>, first: &str) {
    // Check if last block is an empty placeholder and first part should merge
    // (list continuation or unclosed code fence in the block before the placeholder)
    if blocks.len() >= 2
        && blocks.last().map(|b| b.is_empty()).unwrap_or(false)
        && !first.is_empty()
    {
        let prev = &blocks[blocks.len() - 2];
        if should_merge_blocks(first, prev) {
            blocks.pop();
            if let Some(prev) = blocks.last_mut() {
                prev.push_str("\n\n");
                prev.push_str(first);
            }
            return;
        }
    }

    if let Some(last) = blocks.last_mut() {
        // Inside an unclosed fence: always rejoin (even if first is empty, it's fence interior).
        // List continuation: only if first is non-empty (the caller's guard).
        if has_unclosed_fence(last)
            || (!first.is_empty() && should_merge_blocks(first, last))
        {
            last.push_str("\n\n");
            last.push_str(first);
        } else {
            last.push_str(first);
        }
    }
}

/// Detect full-text re-sends from LLM backends.
fn is_full_text_resend(existing: &str, incoming: &str) -> bool {
    let incoming = incoming.trim_start_matches('\n');
    let existing = existing.trim_start_matches('\n');
    !incoming.is_empty() && incoming == existing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_with_ordered_list_item() {
        assert!(starts_with_ordered_list_item("2. Second"));
        assert!(starts_with_ordered_list_item("3. Third"));
        assert!(starts_with_ordered_list_item("10. Tenth"));
        assert!(!starts_with_ordered_list_item("1. First"));
        assert!(!starts_with_ordered_list_item("Not a list"));
        assert!(!starts_with_ordered_list_item(""));
    }

    #[test]
    fn test_ends_with_ordered_list_item() {
        assert!(ends_with_ordered_list_item("1. First item"));
        assert!(ends_with_ordered_list_item("Some text\n2. Second item"));
        assert!(!ends_with_ordered_list_item("Just text"));
        assert!(!ends_with_ordered_list_item(""));
    }
}
