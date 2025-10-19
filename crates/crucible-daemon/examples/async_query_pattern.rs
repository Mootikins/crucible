// Example: Async Query Execution with Cancellation
//
// This example demonstrates the pattern used in the REPL for non-blocking
// query execution with user cancellation support.

use tokio::sync::oneshot;
use tokio::time::{sleep, Duration, timeout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Simulated database query
async fn execute_long_query(query: &str, cancelled: Arc<AtomicBool>) -> Result<String, String> {
    println!("Starting query: {}", query);

    // Simulate long-running query with periodic cancellation checks
    for i in 0..10 {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Query cancelled by user".to_string());
        }

        println!("  Processing chunk {} of 10...", i + 1);
        sleep(Duration::from_millis(500)).await;
    }

    Ok(format!("Query completed: {}", query))
}

/// REPL-style async query execution with cancellation
async fn repl_execute_query(query: &str) -> Result<String, String> {
    // Create cancellation signal
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    // Create oneshot channel for user cancellation (Ctrl+C)
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    // Spawn query in background
    let query_str = query.to_string();
    let query_task = tokio::spawn(async move {
        // Execute with timeout (5 seconds for demo)
        match timeout(Duration::from_secs(5), execute_long_query(&query_str, cancelled_clone)).await {
            Ok(result) => result,
            Err(_) => Err("Query timeout".to_string()),
        }
    });

    // Spawn cancellation listener (simulates Ctrl+C handler)
    let cancel_listener = tokio::spawn(async move {
        // Wait for cancel signal
        cancel_rx.await.ok();
        cancelled.store(true, Ordering::Relaxed);
    });

    // Wait for either completion or cancellation
    tokio::select! {
        result = query_task => {
            cancel_listener.abort();
            match result {
                Ok(query_result) => query_result,
                Err(e) => Err(format!("Task error: {}", e)),
            }
        }
        // In real REPL, this would be triggered by Ctrl+C signal
        // For demo, we don't trigger it (query runs to completion or timeout)
    }
}

#[tokio::main]
async fn main() {
    println!("=== Async Query Pattern Demo ===\n");

    // Example 1: Normal query (completes successfully)
    println!("Example 1: Normal query");
    match repl_execute_query("SELECT * FROM notes LIMIT 10").await {
        Ok(result) => println!("✓ {}\n", result),
        Err(e) => println!("✗ Error: {}\n", e),
    }

    // Example 2: With cancellation (would be triggered by user in real REPL)
    println!("Example 2: Cancellable query");
    println!("(In real REPL, user would press Ctrl+C to cancel)");

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    let query_task = tokio::spawn(async move {
        execute_long_query("SELECT * FROM huge_table", cancelled_clone).await
    });

    // Simulate user pressing Ctrl+C after 2 seconds
    tokio::spawn({
        let cancelled = cancelled.clone();
        async move {
            sleep(Duration::from_secs(2)).await;
            println!("\n⚠️  [Simulating Ctrl+C after 2 seconds]");
            cancelled.store(true, Ordering::Relaxed);
        }
    });

    match query_task.await {
        Ok(Ok(result)) => println!("✓ {}\n", result),
        Ok(Err(e)) => println!("✗ {}\n", e),
        Err(e) => println!("✗ Task error: {}\n", e),
    }

    // Example 3: Query with progress indicator
    println!("Example 3: Query with progress indicator");

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    let query_task = tokio::spawn(async move {
        execute_long_query("SELECT * FROM notes WHERE complex_condition", cancelled_clone).await
    });

    // Progress indicator task
    let progress_task = tokio::spawn(async move {
        let spinners = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut i = 0;
        loop {
            print!("\r{} Executing query...", spinners[i % spinners.len()]);
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
            sleep(Duration::from_millis(100)).await;
            i += 1;
        }
    });

    // Wait for query to complete
    let result = query_task.await;
    progress_task.abort();
    print!("\r"); // Clear progress indicator

    match result {
        Ok(Ok(result)) => println!("✓ {}\n", result),
        Ok(Err(e)) => println!("✗ {}\n", e),
        Err(e) => println!("✗ Task error: {}\n", e),
    }

    println!("=== Demo Complete ===");
}

// Key Patterns Demonstrated:
//
// 1. **Background Execution**: Query runs in spawned task, doesn't block
//
// 2. **Cancellation Support**: AtomicBool checked periodically in query loop
//
// 3. **Timeout Protection**: Outer timeout prevents infinite queries
//
// 4. **Progress Indication**: Separate task shows spinner during execution
//
// 5. **tokio::select!**: Wait for either completion or cancellation
//
// In the real REPL:
// - Ctrl+C sends signal through oneshot channel
// - Progress indicator updates in TUI log window
// - User can start new queries while one is running (future enhancement)
// - Query results are formatted and displayed when complete
