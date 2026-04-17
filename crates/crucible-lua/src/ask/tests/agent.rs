use super::super::*;
use crucible_core::interaction::{AskBatch, AskQuestion};

#[test]
fn test_extract_json_from_code_block() {
    let content = r#"Here's my answer:
```json
{"answers": [{"selected": [0], "other": null}]}
```"#;
    let result = LuaAgentAskContext::extract_json(content);
    assert!(result.is_ok());
    let json = result.unwrap();
    assert!(json.contains("answers"));
}

#[test]
fn test_extract_json_from_plain_block() {
    let content = r#"Here's my answer:
```
{"answers": [{"selected": [1], "other": null}]}
```"#;
    let result = LuaAgentAskContext::extract_json(content);
    assert!(result.is_ok());
}

#[test]
fn test_extract_json_raw() {
    let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
    let result = LuaAgentAskContext::extract_json(content);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), content);
}

#[test]
fn test_extract_json_with_surrounding_text() {
    let content =
        r#"The answer is {"answers": [{"selected": [0], "other": null}]} and that's it."#;
    let result = LuaAgentAskContext::extract_json(content);
    assert!(result.is_ok());
}

#[test]
fn test_parse_response_single_choice() {
    let batch =
        AskBatch::new().question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

    let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
    let result = LuaAgentAskContext::parse_response(content, batch);

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.answers.len(), 1);
    assert_eq!(response.answers[0].selected, vec![0]);
    assert!(response.answers[0].other.is_none());
}

#[test]
fn test_parse_response_with_other() {
    let batch =
        AskBatch::new().question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

    let content = r#"{"answers": [{"selected": [], "other": "custom answer"}]}"#;
    let result = LuaAgentAskContext::parse_response(content, batch);

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.answers.len(), 1);
    assert!(response.answers[0].selected.is_empty());
    assert_eq!(response.answers[0].other, Some("custom answer".to_string()));
}

#[test]
fn test_parse_response_multi_select() {
    let batch = AskBatch::new().question(
        AskQuestion::new("Q1", "First?")
            .choice("A")
            .choice("B")
            .choice("C"),
    );

    let content = r#"{"answers": [{"selected": [0, 2], "other": null}]}"#;
    let result = LuaAgentAskContext::parse_response(content, batch);

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.answers[0].selected, vec![0, 2]);
}

#[test]
fn test_parse_response_multiple_questions() {
    let batch = AskBatch::new()
        .question(AskQuestion::new("Q1", "First?").choice("A").choice("B"))
        .question(AskQuestion::new("Q2", "Second?").choice("X").choice("Y"));

    let content = r#"{"answers": [
            {"selected": [0], "other": null},
            {"selected": [1], "other": null}
        ]}"#;
    let result = LuaAgentAskContext::parse_response(content, batch);

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.answers.len(), 2);
    assert_eq!(response.answers[0].selected, vec![0]);
    assert_eq!(response.answers[1].selected, vec![1]);
}

#[test]
fn test_format_batch_prompt() {
    let batch = AskBatch::new().question(
        AskQuestion::new("Auth", "Method?")
            .choice("OAuth")
            .choice("JWT"),
    );

    let prompt = LuaAgentAskContext::format_batch_prompt(&batch);

    assert!(prompt.contains("Question 1: Method? (Auth)"));
    assert!(prompt.contains("0: OAuth"));
    assert!(prompt.contains("1: JWT"));
    assert!(prompt.contains("JSON format"));
}
