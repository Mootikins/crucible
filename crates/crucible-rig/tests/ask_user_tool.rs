//! Integration tests for AskUserTool
//!
//! Tests the full ask_user flow: tool emits event, receives response, returns structured result.

use std::sync::Arc;
use tokio::sync::Mutex;

use crucible_core::{
    interaction::{AskResponse, InteractionRequest, InteractionResponse},
    interaction_registry::InteractionRegistry,
    events::SessionEvent,
    InteractionContext, EventPushCallback,
};
use crucible_rig::workspace_tools::AskUserTool;
use rig::tool::Tool;
use uuid::Uuid;

/// Helper to create a test InteractionContext with mock event channel
fn create_test_context() -> (Arc<InteractionContext>, tokio::sync::mpsc::Receiver<SessionEvent>) {
    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(10);
    
    let push_event: EventPushCallback = Arc::new(move |event| {
        let _ = event_tx.try_send(event);
    });
    
    let ctx = Arc::new(InteractionContext::new(registry, push_event));
    (ctx, event_rx)
}

#[tokio::test]
async fn test_ask_user_emits_event_and_returns_response() {
    let (ctx, mut event_rx) = create_test_context();
    let tool = AskUserTool::new((*ctx).clone());
    
    // Spawn tool call in background
    let tool_handle = tokio::spawn(async move {
        let args = serde_json::json!({
            "question": "Which option?",
            "choices": ["Option A", "Option B", "Option C"]
        });
        
        let args_obj = serde_json::from_value(args).expect("Failed to parse args");
        tool.call(args_obj).await
    });
    
    // Receive the InteractionRequested event
    let event = event_rx.recv().await.expect("Should receive event");
    
    // Extract request_id and verify event structure
    let request_id = match event {
        SessionEvent::InteractionRequested { request_id, request } => {
            // Verify it's an Ask request
            match request {
                InteractionRequest::Ask(ask_req) => {
                    assert_eq!(ask_req.question, "Which option?");
                    assert_eq!(ask_req.choices.as_ref().unwrap().len(), 3);
                    Uuid::parse_str(&request_id).expect("Invalid UUID")
                }
                _ => panic!("Expected Ask request"),
            }
        }
        _ => panic!("Expected InteractionRequested event"),
    };
    
    // Complete the interaction by sending response
    let response = InteractionResponse::Ask(AskResponse::selected_many(vec![0, 2]));
    {
        let mut registry = ctx.registry.lock().await;
        registry.complete(request_id, response);
    }
    
    // Verify tool returns the response as JSON
    let result = tool_handle.await.expect("Tool task should complete");
    assert!(result.is_ok(), "Tool should succeed");
    
    let json_str = result.unwrap();
    let parsed: AskResponse = serde_json::from_str(&json_str)
        .expect("Response should be valid JSON");
    
    assert_eq!(parsed.selected, vec![0, 2]);
    assert_eq!(parsed.other, None);
}

#[tokio::test]
async fn test_ask_user_handles_cancellation() {
    let (ctx, mut event_rx) = create_test_context();
    let tool = AskUserTool::new((*ctx).clone());
    
    // Spawn tool call in background
    let tool_handle = tokio::spawn(async move {
        let args = serde_json::json!({
            "question": "Do you want to continue?",
            "choices": ["Yes", "No"]
        });
        
        let args_obj = serde_json::from_value(args).expect("Failed to parse args");
        tool.call(args_obj).await
    });
    
    // Receive the event
    let event = event_rx.recv().await.expect("Should receive event");
    
    let request_id = match event {
        SessionEvent::InteractionRequested { request_id, .. } => {
            Uuid::parse_str(&request_id).expect("Invalid UUID")
        }
        _ => panic!("Expected InteractionRequested event"),
    };
    
    // Send cancellation response
    let response = InteractionResponse::Cancelled;
    {
        let mut registry = ctx.registry.lock().await;
        registry.complete(request_id, response);
    }
    
    // Verify tool returns error
    let result = tool_handle.await.expect("Tool task should complete");
    assert!(result.is_err(), "Tool should fail on cancellation");
    
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("cancelled"), "Error should mention cancellation");
}

#[tokio::test]
async fn test_ask_user_with_multi_select() {
    let (ctx, mut event_rx) = create_test_context();
    let tool = AskUserTool::new((*ctx).clone());
    
    // Spawn tool call with multi_select enabled
    let tool_handle = tokio::spawn(async move {
        let args = serde_json::json!({
            "question": "Select all that apply:",
            "choices": ["Red", "Green", "Blue"],
            "multi_select": true
        });
        
        let args_obj = serde_json::from_value(args).expect("Failed to parse args");
        tool.call(args_obj).await
    });
    
    // Receive the event
    let event = event_rx.recv().await.expect("Should receive event");
    
    let request_id = match event {
        SessionEvent::InteractionRequested { request_id, request } => {
            // Verify multi_select flag is set
            match request {
                InteractionRequest::Ask(ask_req) => {
                    assert!(ask_req.multi_select, "Should have multi_select enabled");
                    Uuid::parse_str(&request_id).expect("Invalid UUID")
                }
                _ => panic!("Expected Ask request"),
            }
        }
        _ => panic!("Expected InteractionRequested event"),
    };
    
    // Complete with multiple selections
    let response = InteractionResponse::Ask(AskResponse::selected_many(vec![0, 2]));
    {
        let mut registry = ctx.registry.lock().await;
        registry.complete(request_id, response);
    }
    
    // Verify result
    let result = tool_handle.await.expect("Tool task should complete");
    assert!(result.is_ok(), "Tool should succeed");
    
    let json_str = result.unwrap();
    let parsed: AskResponse = serde_json::from_str(&json_str)
        .expect("Response should be valid JSON");
    
    assert_eq!(parsed.selected, vec![0, 2]);
}

#[tokio::test]
async fn test_ask_user_with_other_text() {
    let (ctx, mut event_rx) = create_test_context();
    let tool = AskUserTool::new((*ctx).clone());
    
    // Spawn tool call with allow_other enabled
    let tool_handle = tokio::spawn(async move {
        let args = serde_json::json!({
            "question": "What's your preference?",
            "choices": ["Option A", "Option B"],
            "allow_other": true
        });
        
        let args_obj = serde_json::from_value(args).expect("Failed to parse args");
        tool.call(args_obj).await
    });
    
    // Receive the event
    let event = event_rx.recv().await.expect("Should receive event");
    
    let request_id = match event {
        SessionEvent::InteractionRequested { request_id, request } => {
            // Verify allow_other flag is set
            match request {
                InteractionRequest::Ask(ask_req) => {
                    assert!(ask_req.allow_other, "Should have allow_other enabled");
                    Uuid::parse_str(&request_id).expect("Invalid UUID")
                }
                _ => panic!("Expected Ask request"),
            }
        }
        _ => panic!("Expected InteractionRequested event"),
    };
    
    // Complete with "other" text response
    let response = InteractionResponse::Ask(AskResponse::other("Custom option".to_string()));
    {
        let mut registry = ctx.registry.lock().await;
        registry.complete(request_id, response);
    }
    
    // Verify result
    let result = tool_handle.await.expect("Tool task should complete");
    assert!(result.is_ok(), "Tool should succeed");
    
    let json_str = result.unwrap();
    let parsed: AskResponse = serde_json::from_str(&json_str)
        .expect("Response should be valid JSON");
    
    assert_eq!(parsed.other, Some("Custom option".to_string()));
}

#[tokio::test]
async fn test_ask_user_with_timeout() {
    let (ctx, _event_rx) = create_test_context();
    let tool = AskUserTool::new((*ctx).clone());
    
    // Spawn tool call but don't complete the interaction
    let tool_handle = tokio::spawn(async move {
        let args = serde_json::json!({
            "question": "This will timeout",
            "choices": ["A", "B"]
        });
        
        let args_obj = serde_json::from_value(args).expect("Failed to parse args");
        tool.call(args_obj).await
    });
    
    // Wait a bit then cancel the task (simulating timeout)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    tool_handle.abort();
    
    // Verify task was aborted
    let result = tool_handle.await;
    assert!(result.is_err(), "Task should be aborted");
}
