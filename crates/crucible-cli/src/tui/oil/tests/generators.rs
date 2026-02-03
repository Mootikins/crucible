use proptest::prelude::*;

pub fn arb_text_content() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 .,!?]{1,100}")
        .unwrap()
        .prop_filter("non-empty and not markdown syntax", |s| {
            let t = s.trim();
            !t.is_empty()
                && !t.starts_with("- ")
                && !t.starts_with("# ")
                && !t
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() && t.contains(". "))
        })
}

pub fn arb_short_text() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z]{5,30}")
        .unwrap()
        .prop_filter("non-empty", |s| !s.trim().is_empty())
}

pub fn arb_user_query() -> impl Strategy<Value = String> {
    prop::string::string_regex("[A-Za-z ]{5,50}")
        .unwrap()
        .prop_filter("non-empty", |s| !s.trim().is_empty())
}

pub fn arb_tool_name() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-z_]{3,12}").unwrap()
}

pub fn arb_tool_args() -> impl Strategy<Value = String> {
    prop::string::string_regex(r#"\{"[a-z]+": "[a-zA-Z0-9]+"\}"#).unwrap()
}

pub fn arb_tool_result() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 _.-]{5,50}").unwrap()
}

pub fn arb_markdown_content() -> impl Strategy<Value = String> {
    prop_oneof![
        arb_text_content(),
        prop::string::string_regex(r"\*\*[a-zA-Z]{3,10}\*\*").unwrap(),
        prop::string::string_regex(r"\*[a-zA-Z]{3,10}\*").unwrap(),
        prop::string::string_regex(r"`[a-zA-Z_]{3,15}`").unwrap(),
        prop::string::string_regex(r"- [a-zA-Z ]{5,30}").unwrap(),
        prop::string::string_regex(r"> [a-zA-Z ]{5,30}").unwrap(),
    ]
}

pub fn arb_terminal_width() -> impl Strategy<Value = usize> {
    60usize..120
}

#[derive(Debug, Clone)]
pub enum TextStreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
}

pub fn arb_text_stream_event() -> impl Strategy<Value = TextStreamEvent> {
    prop_oneof![
        arb_text_content().prop_map(TextStreamEvent::TextDelta),
        arb_text_content().prop_map(TextStreamEvent::ThinkingDelta),
    ]
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCall { name: String, args: String },
    ToolResultDelta { name: String, delta: String },
    ToolResultComplete { name: String },
}

pub fn arb_stream_event() -> impl Strategy<Value = StreamEvent> {
    prop_oneof![
        3 => arb_text_content().prop_map(StreamEvent::TextDelta),
        2 => arb_text_content().prop_map(StreamEvent::ThinkingDelta),
        1 => (arb_tool_name(), arb_tool_args()).prop_map(|(name, args)| StreamEvent::ToolCall { name, args }),
    ]
}

pub fn arb_valid_stream_sequence() -> impl Strategy<Value = Vec<StreamEvent>> {
    prop::collection::vec(arb_stream_event(), 1..20).prop_map(|events| {
        let mut result = Vec::new();
        let mut pending_tool: Option<String> = None;

        for event in events {
            match event {
                StreamEvent::ToolCall { name, args } => {
                    if let Some(prev_name) = pending_tool.take() {
                        result.push(StreamEvent::ToolResultComplete { name: prev_name });
                    }
                    pending_tool = Some(name.clone());
                    result.push(StreamEvent::ToolCall { name, args });
                }
                other => result.push(other),
            }
        }

        if let Some(name) = pending_tool {
            result.push(StreamEvent::ToolResultComplete { name });
        }

        result
    })
}

#[derive(Debug, Clone)]
pub enum RpcEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCall { name: String, args: String },
    ToolResultDelta { name: String, delta: String },
    ToolResultComplete { name: String },
}

pub fn arb_simple_rpc_sequence() -> impl Strategy<Value = Vec<RpcEvent>> {
    prop::collection::vec(
        prop_oneof![
            4 => arb_text_content().prop_map(RpcEvent::TextDelta),
            1 => arb_text_content().prop_map(RpcEvent::ThinkingDelta),
        ],
        1..15,
    )
}

pub fn arb_rpc_sequence_with_tools() -> impl Strategy<Value = Vec<RpcEvent>> {
    (
        prop::collection::vec(arb_tool_name(), 0..3),
        prop::collection::vec(arb_text_content(), 1..10),
        prop::collection::vec(arb_tool_result(), 1..5),
    )
        .prop_map(|(tools, texts, results)| {
            let mut events = Vec::new();
            let mut tool_idx = 0;
            let mut result_idx = 0;

            for (i, text) in texts.iter().enumerate() {
                if i > 0 && tool_idx < tools.len() && i % 3 == 0 {
                    let tool_name = &tools[tool_idx];
                    events.push(RpcEvent::ToolCall {
                        name: tool_name.clone(),
                        args: r#"{"query": "test"}"#.to_string(),
                    });
                    if result_idx < results.len() {
                        events.push(RpcEvent::ToolResultDelta {
                            name: tool_name.clone(),
                            delta: results[result_idx].clone(),
                        });
                        result_idx += 1;
                    }
                    events.push(RpcEvent::ToolResultComplete {
                        name: tool_name.clone(),
                    });
                    tool_idx += 1;
                }
                events.push(RpcEvent::TextDelta(text.clone()));
            }

            events
        })
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub user_query: String,
    pub events: Vec<RpcEvent>,
    pub cancelled: bool,
}

pub fn arb_conversation_turn() -> impl Strategy<Value = ConversationTurn> {
    (arb_user_query(), arb_simple_rpc_sequence(), prop::bool::ANY).prop_map(
        |(user_query, events, cancelled)| {
            let should_cancel = cancelled && events.len() > 2;
            ConversationTurn {
                user_query,
                events,
                cancelled: should_cancel,
            }
        },
    )
}

pub fn arb_multi_turn_conversation() -> impl Strategy<Value = Vec<ConversationTurn>> {
    prop::collection::vec(arb_conversation_turn(), 1..5)
}

#[derive(Debug, Clone)]
pub enum SubagentOutcome {
    Completed(String),
    Failed(String),
    /// Spawned but never resolved (still pending)
    Pending,
}

#[derive(Debug, Clone)]
pub struct SubagentEvent {
    pub id: String,
    pub prompt: String,
    pub outcome: SubagentOutcome,
}

pub fn arb_subagent_event() -> impl Strategy<Value = SubagentEvent> {
    (
        arb_short_text(),
        arb_user_query(),
        prop_oneof![
            arb_text_content().prop_map(SubagentOutcome::Completed),
            arb_text_content().prop_map(SubagentOutcome::Failed),
            Just(SubagentOutcome::Pending),
        ],
    )
        .prop_map(|(id, prompt, outcome)| SubagentEvent {
            id,
            prompt,
            outcome,
        })
}

pub fn arb_subagent_sequence() -> impl Strategy<Value = Vec<SubagentEvent>> {
    prop::collection::vec(arb_subagent_event(), 1..6)
        .prop_map(|mut events| {
            let mut seen = std::collections::HashSet::new();
            events.retain(|e| seen.insert(e.id.clone()));
            events
        })
        .prop_filter("non-empty after dedup", |v| !v.is_empty())
}
