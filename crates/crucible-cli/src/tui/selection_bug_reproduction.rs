//! Reproduction test for selection bug
//!
//! User scenario:
//! 1. User has a conversation with a long assistant response
//! 2. User selects their prompt + following paragraph
//! 3. Copy operation copies text from MUCH LATER in the response instead
//!
//! This suggests the cache is built with wrong content or indices are misaligned.

use crate::tui::conversation_view::RatatuiView;
use crate::tui::selection::{SelectableContentCache, SelectionPoint, SelectionState};

#[test]
fn test_selection_bug_reproduction() {
    // Simulate a long conversation similar to what user had
    let mut view = RatatuiView::new("plan", 80, 24);

    // User's prompt
    view.state_mut().conversation.push_user_message(
        "Please explain how the enchanted yoyo works and its history"
    );

    // Assistant's response with multiple paragraphs
    view.state_mut().conversation.start_assistant_streaming();

    // Paragraph 1 (user wants to select this)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "Word of the enchanted yoyo spread through the village, drawing \
             the attention of travelers and scholars alike."
        ),
    ]);

    // Paragraph 2 (user wants to select this too)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "Some claimed it was a gift from the forest spirits, others whispered \
             it was a relic of an ancient civilization. But Elara knew the truth: \
             the yoyo was a bridge between her imagination and the world around her."
        ),
    ]);

    // Paragraph 3 (middle of response - NOT selected)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "Every morning, she would practice in the meadow, the wooden toy \
             dancing at the end of its string, responding to her slightest movement."
        ),
    ]);

    // Paragraph 4 (middle of response - NOT selected)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "The villagers would watch from afar, some with curiosity, others \
             with skepticism. They didn't understand the deep connection forming \
             between girl and toy."
        ),
    ]);

    // Paragraph 5 (near end - this is what gets copied instead!)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "What the villagers didn't know was that the yoyo had chosen Elara, \
             not the other way around. It had been waiting for centuries for \
             someone with enough imagination to wield it properly."
        ),
    ]);

    // Paragraph 6 (last paragraph)
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "And so, the adventures of Elara and her enchanted yoyo had only \
             just begun."
        ),
    ]);

    view.state_mut().conversation.complete_streaming();

    println!("=== DEBUG: Building selection cache ===");
    let cache_data = view.build_selection_cache();
    println!("Total cache lines: {}", cache_data.len());

    // Print first 20 lines to see what's in cache
    for (i, line_info) in cache_data.iter().enumerate().take(20) {
        let preview: String = line_info.text.chars().take(70).collect();
        println!("Cache[{}]: '{}'", i, preview);
    }

    println!("\n=== DEBUG: Simulating user selection ===");

    // User selects their prompt (should be near line 1) and paragraph 2 (line ~4-5)
    // Let's say they select lines 1 through 5
    let selection_start = SelectionPoint::new(1, 0);  // User prompt line
    let selection_end = SelectionPoint::new(5, 50);   // Middle of paragraph 2

    println!("Selection range: {} to {}", selection_start.line, selection_end.line);

    // Build cache and extract selected text
    let mut cache = SelectableContentCache::new();
    let width = view.state().width;
    cache.update(cache_data, width);

    let extracted_text = cache.extract_text(selection_start, selection_end);

    println!("\n=== DEBUG: Extracted text ===");
    println!("{} characters extracted", extracted_text.chars().count());
    println!("{}", extracted_text);

    // Check if the extracted text is correct
    let has_prompt = extracted_text.contains("enchanted yoyo");
    let has_paragraph1 = extracted_text.contains("Word of the enchanted yoyo");
    let has_paragraph2 = extracted_text.contains("Some claimed it was a gift");
    let has_wrong_paragraph = extracted_text.contains("villagers didn't know");

    println!("\n=== VALIDATION ===");
    println!("Contains user prompt keyword: {}", has_prompt);
    println!("Contains paragraph 1: {}", has_paragraph1);
    println!("Contains paragraph 2: {}", has_paragraph2);
    println!("Contains WRONG paragraph (villagers): {}", has_wrong_paragraph);

    // The bug would be if has_wrong_paragraph is true
    // This means we extracted text from much later in the response
    if has_wrong_paragraph {
        println!("\n❌ BUG REPRODUCED: Selected early text but extracted late text!");
        panic!("Selection bug reproduced - wrong text extracted");
    } else if !has_paragraph2 {
        println!("\n⚠️  WARNING: Expected paragraph 2 not found in extraction");
        println!("This might indicate a different selection bug");
    } else {
        println!("\n✅ Selection worked correctly in this test");
    }
}

#[test]
fn test_selection_cache_alignment() {
    // Test that cache indices match what we expect from rendering
    let mut view = RatatuiView::new("plan", 80, 24);

    view.state_mut().conversation.push_user_message("Test prompt");
    view.state_mut().conversation.start_assistant_streaming();
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose("Line 1 of response"),
    ]);
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose("Line 2 of response"),
    ]);
    view.state_mut().conversation.complete_streaming();

    let cache = view.build_selection_cache();

    println!("\n=== Cache Alignment Test ===");
    println!("State width: {}", view.state().width);
    println!("Cache size: {}", cache.len());
    for (i, line) in cache.iter().enumerate() {
        println!("Cache[{}]: '{}'", i, line.text);
    }

    // Expected cache structure:
    // [0] = "" (blank before user)
    // [1] = " > Test prompt"
    // [2] = "" (blank before assistant)
    // [3] = " ● Line 1 of response"
    // [4] = "   Line 2 of response"

    assert_eq!(cache.len(), 5, "Cache should have 5 lines");
    assert_eq!(cache[1].text, " > Test prompt");
    assert!(cache[3].text.contains("Line 1"));
    assert!(cache[4].text.contains("Line 2"));
}

#[test]
fn test_width_mismatch_bug() {
    // Test the hypothesis: cache is built with state.width but rendering
    // uses area.width (smaller due to status/input areas)
    let mut view = RatatuiView::new("plan", 50, 24);  // Narrow terminal

    // Add a long line that will wrap differently at different widths
    view.state_mut().conversation.push_user_message(
        "This is a very long user prompt that will definitely wrap to multiple lines at narrow widths"
    );

    view.state_mut().conversation.start_assistant_streaming();
    view.state_mut().conversation.append_streaming_blocks(vec![
        crate::tui::StreamBlock::prose(
            "This is also a very long response line that will wrap differently depending on the viewport width available"
        ),
    ]);
    view.state_mut().conversation.complete_streaming();

    println!("\n=== Width Mismatch Test ===");
    println!("Terminal width (state.width): {}", view.state().width);
    println!("Viewport width: {}", view.conversation_viewport_height());

    // Cache is built with state.width - 4
    let cache_from_state = view.build_selection_cache();
    println!("Cache size (using state.width): {}", cache_from_state.len());

    for (i, line) in cache_from_state.iter().enumerate() {
        let preview: String = line.text.chars().take(60).collect();
        println!("Cache[{}]: '{}'", i, preview);
    }

    // What if actual rendering uses a narrower area?
    // The viewport height subtracts status, input, popup areas
    // Let's simulate what would happen if rendered with different width

    // This test documents the potential bug: if cache is built with one width
    // but rendering uses another, the indices will be misaligned
    let has_mismatch = cache_from_state.len() > 8;  // Arbitrary check
    if has_mismatch {
        println!("\n⚠️  Width mismatch detected - cache has {} lines", cache_from_state.len());
        println!("This could cause selection misalignment if rendering uses different width");
    }
}
