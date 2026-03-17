use std::collections::HashMap;

pub struct ToolCallTracker {
    calls: HashMap<(String, String), usize>,
}

impl ToolCallTracker {
    pub fn new() -> Self {
        Self {
            calls: HashMap::new(),
        }
    }

    pub fn record_call(&mut self, name: &str, args: &serde_json::Value) -> usize {
        let key = (name.to_string(), canonical_args(args));
        let count = self.calls.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    pub fn is_repeat_failure(
        &self,
        name: &str,
        args: &serde_json::Value,
        threshold: usize,
    ) -> bool {
        let key = (name.to_string(), canonical_args(args));
        self.calls
            .get(&key)
            .is_some_and(|attempt_count| *attempt_count >= threshold)
    }

    pub fn reset(&mut self) {
        self.calls.clear();
    }
}

fn canonical_args(args: &serde_json::Value) -> String {
    serde_json::to_string(args).unwrap_or_else(|_| "null".to_string())
}

impl Default for ToolCallTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tool_call_tracker {
    use super::ToolCallTracker;
    use serde_json::json;

    #[test]
    fn test_record_call_returns_attempt_number() {
        let mut tracker = ToolCallTracker::new();
        let attempt = tracker.record_call("read_file", &json!({ "path": "/tmp/a" }));

        assert_eq!(attempt, 1);
    }

    #[test]
    fn test_record_call_increments_same_args() {
        let mut tracker = ToolCallTracker::new();
        let args = json!({ "path": "/tmp/a" });

        assert_eq!(tracker.record_call("read_file", &args), 1);
        assert_eq!(tracker.record_call("read_file", &args), 2);
        assert_eq!(tracker.record_call("read_file", &args), 3);
    }

    #[test]
    fn test_record_call_distinct_args_tracked_separately() {
        let mut tracker = ToolCallTracker::new();

        let first = tracker.record_call("read_file", &json!({ "path": "/tmp/a" }));
        let second = tracker.record_call("read_file", &json!({ "path": "/tmp/b" }));

        assert_eq!(first, 1);
        assert_eq!(second, 1);
    }

    #[test]
    fn test_is_repeat_failure_at_threshold() {
        let mut tracker = ToolCallTracker::new();
        let args = json!({ "path": "/tmp/a" });

        tracker.record_call("read_file", &args);
        tracker.record_call("read_file", &args);
        assert!(!tracker.is_repeat_failure("read_file", &args, 3));

        tracker.record_call("read_file", &args);
        assert!(tracker.is_repeat_failure("read_file", &args, 3));
    }

    #[test]
    fn test_reset_clears_state() {
        let mut tracker = ToolCallTracker::new();
        let args = json!({ "path": "/tmp/a" });

        tracker.record_call("read_file", &args);
        tracker.record_call("read_file", &args);
        tracker.reset();

        assert_eq!(tracker.record_call("read_file", &args), 1);
        assert!(!tracker.is_repeat_failure("read_file", &args, 2));
    }
}
