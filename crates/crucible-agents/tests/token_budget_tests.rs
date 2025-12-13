//! Token Budget Edge Case Tests
//!
//! Tests for edge cases in token budget and context management that complement
//! the property-based tests in token_budget_property_tests.rs.
//!
//! These tests focus on specific scenarios:
//! - Zero budget handling
//! - Empty context handling
//! - Large message truncation
//! - System prompt interaction with budget
//! - Budget exact fit scenarios
//! - FIFO ordering guarantees

use crucible_agents::context::SlidingWindowContext;
use crucible_agents::token::TokenBudget;
use crucible_core::traits::context::ContextManager;
use crucible_core::traits::llm::LlmMessage;

// ============================================================================
// Token Budget Edge Cases
// ============================================================================

#[test]
fn test_zero_budget_clears_context() {
    let mut context = SlidingWindowContext::new(1000);

    // Add several messages
    context.add_message(LlmMessage::user("Hello world"));
    context.add_message(LlmMessage::assistant("Hi there!"));
    context.add_message(LlmMessage::user("How are you?"));

    assert_eq!(context.message_count(), 3);

    // Trim to zero budget - should remove all messages
    context.trim_to_budget(0);

    // All messages should be cleared
    assert_eq!(context.message_count(), 0);
    assert!(context.get_messages().is_empty());
}

#[test]
fn test_zero_budget_preserves_system_prompt() {
    let mut context = SlidingWindowContext::new(1000);

    // Set system prompt
    context.set_system_prompt("You are a helpful assistant.".to_string());

    // Add messages
    context.add_message(LlmMessage::user("Hello"));
    context.add_message(LlmMessage::assistant("Hi"));

    // Trim to zero budget
    context.trim_to_budget(0);

    // System prompt should be preserved
    assert_eq!(
        context.get_system_prompt(),
        Some("You are a helpful assistant.")
    );

    // But messages should be cleared
    assert_eq!(context.message_count(), 0);

    // get_messages should only return system prompt
    let messages = context.get_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "You are a helpful assistant.");
}

#[test]
fn test_budget_smaller_than_system_prompt() {
    let mut context = SlidingWindowContext::new(1000);

    // Create a large system prompt: 1000 chars = ~250 tokens
    let large_system_prompt = "x".repeat(1000);
    context.set_system_prompt(large_system_prompt.clone());

    // Add a small message: 4 chars = 1 token
    context.add_message(LlmMessage::user("test"));

    assert_eq!(context.message_count(), 1);

    // Trim to budget smaller than system prompt alone (10 tokens)
    context.trim_to_budget(10);

    // System prompt should be preserved even though it exceeds budget
    assert_eq!(context.get_system_prompt(), Some(large_system_prompt.as_str()));

    // Message should be removed to try to meet budget
    assert_eq!(context.message_count(), 0);
}

#[test]
fn test_empty_context_handling() {
    let mut context = SlidingWindowContext::new(1000);

    // Should not panic on empty context
    context.trim_to_budget(100);

    assert_eq!(context.message_count(), 0);
    assert_eq!(context.token_estimate(), 0);
    assert!(context.get_messages().is_empty());

    // Try trimming to zero on empty context
    context.trim_to_budget(0);

    assert_eq!(context.message_count(), 0);
    assert!(context.get_messages().is_empty());
}

#[test]
fn test_empty_context_with_system_prompt() {
    let mut context = SlidingWindowContext::new(1000);

    context.set_system_prompt("System prompt".to_string());

    // No messages, only system prompt
    assert_eq!(context.message_count(), 0);

    // Trim should not panic
    context.trim_to_budget(100);

    // System prompt preserved
    assert_eq!(context.get_system_prompt(), Some("System prompt"));
    assert_eq!(context.message_count(), 0);

    // get_messages should return just system prompt
    let messages = context.get_messages();
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_large_message_truncation() {
    let mut context = SlidingWindowContext::new(1000);

    // Create a very large single message: 4000 chars = ~1000 tokens
    let huge_message = "x".repeat(4000);
    context.add_message(LlmMessage::user(huge_message));

    assert_eq!(context.message_count(), 1);

    // Trim to very small budget (10 tokens)
    context.trim_to_budget(10);

    // Large message should be removed
    assert_eq!(context.message_count(), 0);
    assert!(context.get_messages().is_empty());
}

#[test]
fn test_large_message_with_small_messages() {
    let mut context = SlidingWindowContext::new(1000);

    // Add large message first: 400 chars = 100 tokens
    let large_message = "x".repeat(400);
    context.add_message(LlmMessage::user(large_message));

    // Add smaller messages after: 4 chars = 1 token each
    context.add_message(LlmMessage::assistant("MSG2"));
    context.add_message(LlmMessage::user("MSG3"));
    context.add_message(LlmMessage::assistant("MSG4"));

    assert_eq!(context.message_count(), 4);

    // Trim to budget that fits only small messages (3 tokens)
    context.trim_to_budget(3);

    // Large message should be removed, small ones kept
    assert_eq!(context.message_count(), 3);

    let messages = context.get_messages();
    assert_eq!(messages[0].content, "MSG2");
    assert_eq!(messages[1].content, "MSG3");
    assert_eq!(messages[2].content, "MSG4");
}

#[test]
fn test_sliding_window_fifo_ordering() {
    let mut context = SlidingWindowContext::new(1000);

    // Add messages in order: 8 chars = 2 tokens each
    context.add_message(LlmMessage::user("MSG_001_"));
    context.add_message(LlmMessage::assistant("MSG_002_"));
    context.add_message(LlmMessage::user("MSG_003_"));
    context.add_message(LlmMessage::assistant("MSG_004_"));
    context.add_message(LlmMessage::user("MSG_005_"));

    assert_eq!(context.message_count(), 5);

    // Trim to fit only 3 messages (6 tokens)
    context.trim_to_budget(6);

    assert_eq!(context.message_count(), 3);

    let messages = context.get_messages();

    // Should keep the most recent messages (003, 004, 005)
    assert_eq!(messages[0].content, "MSG_003_");
    assert_eq!(messages[1].content, "MSG_004_");
    assert_eq!(messages[2].content, "MSG_005_");
}

#[test]
fn test_fifo_with_system_prompt() {
    let mut context = SlidingWindowContext::new(1000);

    // Set system prompt: 8 chars = 2 tokens
    context.set_system_prompt("SYSTMPRT".to_string());

    // Add messages: 4 chars = 1 token each
    context.add_message(LlmMessage::user("MSG1"));
    context.add_message(LlmMessage::assistant("MSG2"));
    context.add_message(LlmMessage::user("MSG3"));
    context.add_message(LlmMessage::assistant("MSG4"));

    // Trim to fit system + 2 messages (4 tokens = 2 for system + 2 for messages)
    context.trim_to_budget(4);

    // Should keep system prompt + last 2 messages
    assert_eq!(context.get_system_prompt(), Some("SYSTMPRT"));
    assert_eq!(context.message_count(), 2);

    let messages = context.get_messages();
    assert_eq!(messages.len(), 3); // system + 2 messages
    assert_eq!(messages[0].content, "SYSTMPRT");
    assert_eq!(messages[1].content, "MSG3");
    assert_eq!(messages[2].content, "MSG4");
}

#[test]
fn test_budget_exact_fit() {
    let mut context = SlidingWindowContext::new(1000);

    // Add messages that exactly fit budget: 4 chars = 1 token each
    context.add_message(LlmMessage::user("MSG1"));
    context.add_message(LlmMessage::assistant("MSG2"));
    context.add_message(LlmMessage::user("MSG3"));

    assert_eq!(context.message_count(), 3);
    assert_eq!(context.token_estimate(), 3);

    // Trim to exact token count
    context.trim_to_budget(3);

    // All messages should be preserved
    assert_eq!(context.message_count(), 3);

    let messages = context.get_messages();
    assert_eq!(messages.len(), 3);
}

#[test]
fn test_budget_exact_fit_with_system() {
    let mut context = SlidingWindowContext::new(1000);

    // System: 8 chars = 2 tokens
    context.set_system_prompt("SYSTMPRT".to_string());

    // Messages: 4 chars = 1 token each
    context.add_message(LlmMessage::user("MSG1"));
    context.add_message(LlmMessage::assistant("MSG2"));

    // Total: 2 + 1 + 1 = 4 tokens
    assert_eq!(context.token_estimate(), 4);

    // Trim to exact fit
    context.trim_to_budget(4);

    // Everything should be preserved
    assert_eq!(context.get_system_prompt(), Some("SYSTMPRT"));
    assert_eq!(context.message_count(), 2);
}

#[test]
fn test_budget_off_by_one() {
    let mut context = SlidingWindowContext::new(1000);

    // Add 3 messages: 4 chars = 1 token each
    context.add_message(LlmMessage::user("MSG1"));
    context.add_message(LlmMessage::assistant("MSG2"));
    context.add_message(LlmMessage::user("MSG3"));

    assert_eq!(context.token_estimate(), 3);

    // Trim to one less than current
    context.trim_to_budget(2);

    // Should keep last 2 messages
    assert_eq!(context.message_count(), 2);

    let messages = context.get_messages();
    assert_eq!(messages[0].content, "MSG2");
    assert_eq!(messages[1].content, "MSG3");
}

// ============================================================================
// TokenBudget Specific Tests
// ============================================================================

#[test]
fn test_token_budget_tracking() {
    let mut budget = TokenBudget::new(1000);

    assert_eq!(budget.current(), 0);
    assert_eq!(budget.remaining(), 1000);
    assert!(budget.has_room(1000));
    assert!(!budget.has_room(1001));

    budget.add(100);
    assert_eq!(budget.current(), 100);
    assert_eq!(budget.remaining(), 900);

    budget.add(400);
    assert_eq!(budget.current(), 500);
    assert_eq!(budget.remaining(), 500);

    budget.remove(200);
    assert_eq!(budget.current(), 300);
    assert_eq!(budget.remaining(), 700);
}

#[test]
fn test_token_budget_reset() {
    let mut budget = TokenBudget::new(1000);

    budget.add(500);
    assert_eq!(budget.current(), 500);
    assert_eq!(budget.remaining(), 500);

    budget.reset();
    assert_eq!(budget.current(), 0);
    assert_eq!(budget.remaining(), 1000);
}

#[test]
fn test_token_budget_set_max() {
    let mut budget = TokenBudget::new(1000);

    budget.add(300);
    assert_eq!(budget.remaining(), 700);

    // Increase max
    budget.set_max(2000);
    assert_eq!(budget.current(), 300);
    assert_eq!(budget.remaining(), 1700);

    // Decrease max
    budget.set_max(500);
    assert_eq!(budget.current(), 300);
    assert_eq!(budget.remaining(), 200);
}

#[test]
fn test_token_budget_set_max_below_current() {
    let mut budget = TokenBudget::new(1000);

    budget.add(800);
    assert_eq!(budget.current(), 800);

    // Set max below current usage
    budget.set_max(500);
    assert_eq!(budget.current(), 800); // Current doesn't change
    assert_eq!(budget.remaining(), 0); // Saturating subtraction
}

#[test]
fn test_token_budget_zero_max() {
    let budget = TokenBudget::new(0);

    assert_eq!(budget.remaining(), 0);
    assert!(!budget.has_room(1));
    assert!(budget.has_room(0)); // Zero tokens should fit
}

#[test]
fn test_token_budget_estimate_empty_string() {
    let budget = TokenBudget::new(1000);

    let estimate = budget.estimate_tokens("");
    assert_eq!(estimate, 0);
}

#[test]
fn test_token_budget_estimate_scales_with_length() {
    let budget = TokenBudget::new(1000);

    let short_text = "hello";
    let long_text = "hello".repeat(10);

    let short_estimate = budget.estimate_tokens(short_text);
    let long_estimate = budget.estimate_tokens(&long_text);

    // Longer text should have more tokens
    assert!(long_estimate > short_estimate);
}

#[test]
fn test_token_budget_correction_increases_estimate() {
    let mut budget = TokenBudget::new(1000);

    let text = "a".repeat(400); // ~100 tokens estimated

    let initial_estimate = budget.estimate_tokens(&text);

    // Add estimate and correct with higher actual
    budget.add(initial_estimate);
    budget.correct(150); // 50% higher than estimated

    // Next estimate should be higher due to correction
    let corrected_estimate = budget.estimate_tokens(&text);
    assert!(corrected_estimate > initial_estimate);
}

#[test]
fn test_token_budget_correction_decreases_estimate() {
    let mut budget = TokenBudget::new(1000);

    let text = "a".repeat(400);

    let initial_estimate = budget.estimate_tokens(&text);

    // Add estimate and correct with lower actual
    budget.add(initial_estimate);
    budget.correct(50); // 50% lower than estimated

    // Next estimate should be lower due to correction
    let corrected_estimate = budget.estimate_tokens(&text);
    assert!(corrected_estimate < initial_estimate);
}

#[test]
fn test_token_budget_multiple_corrections() {
    let mut budget = TokenBudget::new(10000);

    // Perform multiple corrections with consistent ratio
    // Key: reset between corrections to avoid cumulative drift
    for _ in 0..10 {
        budget.reset();
        budget.add(100);
        budget.correct(150); // Always 50% higher
    }

    // Correction factor should have adapted upward
    // With EMA (alpha=0.3), it should converge toward 1.5
    // After 10 iterations, it should be noticeably > 1.0
    assert_eq!(budget.corrections(), 10);
    assert!(
        budget.correction_factor() > 1.1,
        "Correction factor should increase when actual > estimated, got {}",
        budget.correction_factor()
    );
}

#[test]
fn test_token_budget_has_room_edge_cases() {
    let mut budget = TokenBudget::new(100);

    budget.add(50);

    // Exact fit
    assert!(budget.has_room(50));

    // One over
    assert!(!budget.has_room(51));

    // Zero always fits
    assert!(budget.has_room(0));

    // Large number doesn't fit
    assert!(!budget.has_room(1000));
}

// ============================================================================
// Combined Budget and Context Tests
// ============================================================================

#[test]
fn test_context_trim_after_many_additions() {
    let mut context = SlidingWindowContext::new(1000);

    // Add many messages
    for i in 0..100 {
        context.add_message(LlmMessage::user(format!("Message {}", i)));
    }

    assert_eq!(context.message_count(), 100);

    // Trim to small budget (20 tokens)
    context.trim_to_budget(20);

    // Should keep only the most recent messages
    assert!(context.message_count() <= 20);

    let messages = context.get_messages();

    // Check that we kept the latest messages
    if !messages.is_empty() {
        let last_msg = &messages[messages.len() - 1].content;
        assert!(last_msg.contains("Message 99"));
    }
}

#[test]
fn test_context_alternating_roles() {
    let mut context = SlidingWindowContext::new(1000);

    // Add alternating user/assistant messages with controlled size
    // Each message is exactly 8 chars = 2 tokens
    for i in 0..10 {
        if i % 2 == 0 {
            context.add_message(LlmMessage::user(format!("USER_{:03}", i)));
        } else {
            context.add_message(LlmMessage::assistant(format!("ASST_{:03}", i)));
        }
    }

    // Total: 10 messages * 2 tokens = 20 tokens
    // Trim to fit only 4 messages (8 tokens)
    context.trim_to_budget(8);

    assert_eq!(context.message_count(), 4);

    let messages = context.get_messages();

    // Should preserve order and alternation of the last 4 messages
    assert!(messages[0].content.starts_with("USER"));
    assert!(messages[1].content.starts_with("ASST"));
    assert!(messages[2].content.starts_with("USER"));
    assert!(messages[3].content.starts_with("ASST"));
}

#[test]
fn test_clear_and_repopulate() {
    let mut context = SlidingWindowContext::new(1000);

    context.set_system_prompt("System".to_string());

    // Add, clear, add again
    context.add_message(LlmMessage::user("First"));
    context.clear();
    context.add_message(LlmMessage::user("Second"));

    // Should only have the second message
    assert_eq!(context.message_count(), 1);

    let messages = context.get_messages();
    assert_eq!(messages.len(), 2); // system + 1 message
    assert_eq!(messages[1].content, "Second");
}
