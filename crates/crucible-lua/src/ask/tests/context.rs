use super::super::*;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::InteractionRegistry;
use std::sync::{Arc, Mutex};

#[test]
fn test_lua_ask_context_creation() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let push_fn: EventPushCallback = Arc::new(move |_event| {
        called_clone.store(true, Ordering::SeqCst);
    });

    let context = LuaAskContext::new(registry.clone(), push_fn);

    // Verify context was created with the registry
    assert!(!called.load(Ordering::SeqCst));

    // The registry should be accessible
    let guard = context.registry.lock().unwrap();
    assert_eq!(guard.pending_count(), 0);
}

#[test]
fn test_lua_ask_context_event_push() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let event_received = Arc::new(AtomicBool::new(false));
    let received_request_id = Arc::new(Mutex::new(String::new()));

    let event_received_clone = event_received.clone();
    let received_request_id_clone = received_request_id.clone();

    let push_fn: EventPushCallback = Arc::new(move |event| {
        event_received_clone.store(true, Ordering::SeqCst);
        if let SessionEvent::InteractionRequested { request_id, .. } = event {
            *received_request_id_clone.lock().unwrap() = request_id;
        }
    });

    let context = LuaAskContext::new(registry.clone(), push_fn);

    // Create a batch and manually trigger the push event path
    let batch = LuaAskBatch::new();
    let batch_id = batch.inner.id;

    // Manually push the event (simulating what ask_user does internally)
    (context.push_event)(SessionEvent::InteractionRequested {
        request_id: batch_id.to_string(),
        request: InteractionRequest::AskBatch(batch.inner.clone()),
    });

    // Verify the event was received
    assert!(event_received.load(Ordering::SeqCst));
    assert_eq!(*received_request_id.lock().unwrap(), batch_id.to_string());
}

#[test]
fn test_lua_ask_context_registry_integration() {
    // Test that the context correctly registers with the registry
    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let push_fn: EventPushCallback = Arc::new(|_event| {});
    let _context = LuaAskContext::new(registry.clone(), push_fn);

    // The batch ID should be registerable
    let batch = LuaAskBatch::new();
    let batch_id = batch.inner.id;

    {
        let mut guard = registry.lock().unwrap();
        let _rx = guard.register(batch_id);
        assert!(guard.is_pending(batch_id));
    }
}

#[test]
fn test_lua_ask_context_clone() {
    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let push_fn: EventPushCallback = Arc::new(|_event| {});
    let context = LuaAskContext::new(registry.clone(), push_fn);

    let cloned = context.clone();

    // Both should reference the same registry (they share the Arc)
    // Note: We can't lock both at once as that would deadlock with std::sync::Mutex
    // Instead, verify they point to the same underlying data via Arc::ptr_eq
    assert!(Arc::ptr_eq(&context.registry, &cloned.registry));

    // Also verify functionality works through either handle
    {
        let guard = context.registry.lock().unwrap();
        assert_eq!(guard.pending_count(), 0);
    }
    {
        let guard = cloned.registry.lock().unwrap();
        assert_eq!(guard.pending_count(), 0);
    }
}
