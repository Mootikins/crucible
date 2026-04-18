pub(super) fn prettify_tool_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .take(3)
                .map(|(k, v)| {
                    let v_str = match v {
                        serde_json::Value::String(s) => {
                            if s.len() > 30 {
                                let truncated: String = s.chars().take(27).collect();
                                format!("\"{}...\"", truncated)
                            } else {
                                format!("\"{}\"", s)
                            }
                        }
                        _ => v.to_string(),
                    };
                    format!("{}={}", k, v_str)
                })
                .collect();
            if map.len() > 3 {
                format!("({}, ...)", pairs.join(", "))
            } else {
                format!("({})", pairs.join(", "))
            }
        }
        _ => args.to_string(),
    }
}
