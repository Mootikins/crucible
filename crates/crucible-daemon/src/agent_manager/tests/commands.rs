//! The session's command catalog, and the routing of a `/name` message
//! through the real `session.send_message` handler.

use super::*;
use crate::protocol::{Request, RequestId, Response};
use crucible_core::types::{CommandKind, SendOutcome, SessionCommand};
use crucible_lua::{CommandEffect, DiscoveredCommand};

fn request(method: &str, params: serde_json::Value) -> Request {
    Request {
        jsonrpc: "2.0".to_string(),
        id: Some(RequestId::Number(1)),
        method: method.to_string(),
        params,
    }
}

/// A plugin `alpha` with two commands: `reflect`, which echoes its input, and
/// `help`, which a built-in command keeps.
fn install_plugin(am: &AgentManager) {
    let lua = mlua::Lua::new();
    let reflect = lua
        .load("return function(args) return 'reflected: ' .. tostring(args and args.input) end")
        .eval::<mlua::Function>()
        .unwrap();
    let command = |name: &str| DiscoveredCommand {
        name: name.to_string(),
        description: format!("{name} from alpha"),
        params: Vec::new(),
        input_hint: None,
        effect: CommandEffect::Write,
        source_path: "test".to_string(),
        handler_fn: name.to_string(),
    };
    let registry = crate::plugin_tools::PluginRegistry::new();
    registry.register_plugin(
        "alpha",
        &lua,
        &[],
        &[command("reflect"), command("help")],
        HashMap::new(),
        HashMap::from([
            ("reflect".to_string(), reflect.clone()),
            ("help".to_string(), reflect),
        ]),
    );
    am.set_plugin_tool_registry(Arc::new(registry));
}

fn agent_command(name: &str) -> SessionCommand {
    SessionCommand {
        name: name.to_string(),
        description: format!("{name} from the agent"),
        input_hint: None,
        kind: CommandKind::Agent,
    }
}

struct Fixture {
    am: Arc<AgentManager>,
    session: crucible_core::session::Session,
    tx: crate::EventBus,
    rx: broadcast::Receiver<SessionEventMessage>,
    messages: CapturedMessages,
    prompt: CapturedPrompt,
    _workspace: TempDir,
    _agent_commands: tokio::sync::watch::Sender<Vec<SessionCommand>>,
}

/// A session in a workspace with the skill `tidy`, the plugin `alpha`, and
/// an agent that advertises `compact` and `reflect`.
async fn fixture() -> Fixture {
    let workspace = TempDir::new().unwrap();
    let skill = workspace.path().join(".crucible/skills/tidy");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: tidy\ndescription: Tidy the imports\n---\nSort every import block.\n",
    )
    .unwrap();

    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();
    install_plugin(&am);

    let messages = Arc::new(StdMutex::new(None));
    let prompt = Arc::new(StdMutex::new(None));
    am.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: prompt.clone(),
            received_messages: messages.clone(),
            events: vec![script::text("reply"), script::done()],
        }))),
    );
    let (agent_commands, rx) =
        tokio::sync::watch::channel(vec![agent_command("compact"), agent_command("reflect")]);
    am.slot(&session.id).seed_agent_commands_for_test(rx);

    let (tx, rx) = crate::EventBus::channel(256);
    Fixture {
        am,
        session,
        tx,
        rx,
        messages,
        prompt,
        _workspace: workspace,
        _agent_commands: agent_commands,
    }
}

impl Fixture {
    async fn send(&self, content: &str) -> Response {
        let ctx = crate::rpc::RpcContext::for_test(
            self.am.kiln_manager.clone(),
            self.am.session_manager().clone(),
            self.am.clone(),
            Arc::new(crate::project_manager::ProjectManager::new(
                self._workspace.path().join("projects.json"),
            )),
            self.tx.clone(),
            self._workspace.path().into(),
        );
        crate::server::session::handle_session_send_message(
            request(
                "session.send_message",
                serde_json::json!({ "session_id": self.session.id, "content": content }),
            ),
            &self.am,
            &self.tx,
            ctx.diff_admission(),
        )
        .await
    }

    /// Wait until the turn that the last send started frees its slot.
    async fn finish_turn(&mut self) {
        loop {
            let event = timeout(Duration::from_secs(10), self.rx.recv())
                .await
                .unwrap()
                .unwrap();
            if event.event == "message_complete" {
                break;
            }
        }
        while self.am.request_state.contains_key(self.session.id.as_str()) {
            tokio::task::yield_now().await;
        }
    }

    /// The events already on the bus, by name.
    fn drained(&mut self) -> Vec<String> {
        std::iter::from_fn(|| self.rx.try_recv().ok())
            .map(|event| event.event)
            .collect()
    }
}

fn outcome(response: Response) -> SendOutcome {
    let result = response.result.unwrap_or_else(|| {
        panic!("the send succeeds: {:?}", response.error);
    });
    serde_json::from_value(result).unwrap()
}

#[tokio::test]
async fn the_catalog_lists_each_source_in_order_and_an_earlier_source_keeps_a_name() {
    let f = fixture().await;
    let catalog = f.am.session_commands(&f.session.id, None).await.unwrap();

    let find = |name: &str| {
        catalog
            .iter()
            .position(|c| c.name == name)
            .unwrap_or_else(|| panic!("/{name} is in the catalog: {catalog:#?}"))
    };
    assert!(matches!(
        catalog[find("help")].kind,
        CommandKind::Builtin { .. }
    ));
    assert!(matches!(
        catalog[find("plan")].kind,
        CommandKind::Mode { .. }
    ));
    assert_eq!(
        catalog[find("reflect")].kind,
        CommandKind::Plugin {
            plugin: "alpha".into()
        }
    );
    assert_eq!(catalog[find("tidy")].kind, CommandKind::Skill);
    assert_eq!(catalog[find("tidy")].description, "Tidy the imports");
    assert_eq!(catalog[find("compact")].kind, CommandKind::Agent);

    // One entry per name, and the sources in order.
    for name in ["help", "reflect"] {
        assert_eq!(catalog.iter().filter(|c| c.name == name).count(), 1);
    }
    assert!(find("help") < find("plan"));
    assert!(find("plan") < find("reflect"));
    assert!(find("reflect") < find("tidy"));
    assert!(find("tidy") < find("compact"));
}

#[tokio::test]
async fn a_plugin_command_runs_with_its_input_and_starts_no_turn() {
    let mut f = fixture().await;
    assert_eq!(
        outcome(f.send("/reflect last three turns").await),
        SendOutcome::Command {
            command: "reflect".into(),
            result: "reflected: last three turns".into(),
        }
    );
    assert!(f.prompt.lock().unwrap().is_none(), "no turn ran");
    assert!(!f.drained().contains(&"user_message".to_string()));
}

#[tokio::test]
async fn a_bare_mode_command_switches_the_mode_and_starts_no_turn() {
    let mut f = fixture().await;
    let SendOutcome::Command { command, .. } = outcome(f.send("/plan").await) else {
        panic!("a bare mode command starts no turn");
    };
    assert_eq!(command, "plan");
    assert_eq!(
        f.am.get_mode(&f.session.id).unwrap().as_deref(),
        Some("plan")
    );
    let events = f.drained();
    assert!(events.contains(&"mode_changed".to_string()), "{events:?}");
    assert!(!events.contains(&"user_message".to_string()), "{events:?}");
}

/// A command name in another case still names the command.
#[tokio::test]
async fn a_command_name_in_another_case_names_the_command() {
    let f = fixture().await;
    assert!(matches!(
        outcome(f.send("/PLAN").await),
        SendOutcome::Command { command, .. } if command == "plan"
    ));
}

#[tokio::test]
async fn a_mode_command_with_text_switches_the_mode_and_sends_the_text() {
    let mut f = fixture().await;
    assert!(matches!(
        outcome(f.send("/plan outline the change").await),
        SendOutcome::Turn { .. }
    ));
    f.finish_turn().await;
    assert_eq!(
        f.prompt.lock().unwrap().as_deref(),
        Some("outline the change")
    );
    assert_eq!(
        f.am.get_mode(&f.session.id).unwrap().as_deref(),
        Some("plan")
    );
}

/// The turn keeps the text the user typed, and the skill's instructions go
/// with it as tagged context, as review comments do.
#[tokio::test]
async fn a_skill_command_gives_the_turn_the_skill_instructions() {
    let mut f = fixture().await;
    assert!(matches!(
        outcome(f.send("/tidy src/lib.rs").await),
        SendOutcome::Turn { .. }
    ));
    f.finish_turn().await;
    assert_eq!(
        f.prompt.lock().unwrap().as_deref(),
        Some("/tidy src/lib.rs")
    );
    let skill_blocks: Vec<String> = f
        .messages
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .into_iter()
        .filter(|m| m.metadata.tags.iter().any(|tag| tag == "skill"))
        .map(|m| m.content)
        .collect();
    assert_eq!(skill_blocks.len(), 1, "{skill_blocks:?}");
    assert!(skill_blocks[0].contains("Sort every import block."));
}

/// The agent reads its own commands from the prompt, and a name the catalog
/// does not hold is text.
#[tokio::test]
async fn an_agent_command_and_an_unknown_name_go_to_the_agent_as_text() {
    let mut f = fixture().await;
    for text in ["/compact", "/nothing here"] {
        assert!(matches!(
            outcome(f.send(text).await),
            SendOutcome::Turn { .. }
        ));
        f.finish_turn().await;
        assert_eq!(f.prompt.lock().unwrap().as_deref(), Some(text));
    }
}

/// An agent can advertise its commands before the announcer starts, and at
/// any time after. The clients hear of both.
#[tokio::test]
async fn each_new_agent_command_list_is_announced() {
    let (commands, rx) = tokio::sync::watch::channel(vec![agent_command("compact")]);
    let (tx, mut events) = crate::EventBus::channel(8);
    crate::agent_manager::messaging::send::announce_agent_commands("s1", rx, tx);

    let first = next_event(&mut events).await;
    assert_eq!(first.event, "commands_changed");
    assert_eq!(first.session_id, "s1");

    commands.send_replace(vec![agent_command("review")]);
    assert_eq!(next_event(&mut events).await.event, "commands_changed");
}

async fn next_event(events: &mut broadcast::Receiver<SessionEventMessage>) -> SessionEventMessage {
    timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap()
}
