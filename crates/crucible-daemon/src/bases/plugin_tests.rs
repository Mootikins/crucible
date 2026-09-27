use super::*;
use crate::{
    agent_manager::{AgentManager, AgentManagerParams},
    background_manager::BackgroundJobManager,
    daemon_plugins::DaemonPluginLoader,
    kiln_manager::KilnManager,
    rpc::RpcContext,
};
use crucible_core::{
    config::components::permissions::{PermissionConfig, PermissionMode},
    session::{Session, SessionAgent, SessionType},
    types::WriteMode,
};
use mlua::LuaSerdeExt;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn rig(mode: WriteMode) -> (TempDir, Arc<RpcContext>, DaemonPluginLoader, Session) {
    rig_with_permission(mode, PermissionMode::Allow).await
}
async fn rig_with_permission(
    mode: WriteMode,
    permission: PermissionMode,
) -> (TempDir, Arc<RpcContext>, DaemonPluginLoader, Session) {
    rig_with_rules(
        mode,
        PermissionConfig {
            default: permission,
            ..Default::default()
        },
    )
    .await
}
async fn rig_with_rules(
    mode: WriteMode,
    permission: PermissionConfig,
) -> (TempDir, Arc<RpcContext>, DaemonPluginLoader, Session) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("kiln");
    std::fs::create_dir(&root).unwrap();
    let sm = crate::test_support::temp_session_manager_with_kilns(&[("notes", &root)]);
    let (event_tx, _) = crate::EventBus::channel(32);
    let modes = crucible_lua::ModeRegistry::new();
    modes.set(crucible_lua::ModeDefinition {
        name: "test".into(),
        label: None,
        description: None,
        tools: crucible_lua::ToolSelector::All,
        permissions: Default::default(),
        writes: mode,
    });
    let am = Arc::new(
        AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: sm.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: Some(permission),
            plugin_loader: None,
            source_roots: Default::default(),
            review_snapshot_root: dir.path().join("snapshots"),
        })
        .with_modes(Some(modes)),
    );
    let ctx = Arc::new(RpcContext::for_test(
        Arc::new(KilnManager::new()),
        sm.clone(),
        am.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            dir.path().join("projects.json"),
        )),
        event_tx,
        dir.path().to_owned(),
    ));
    ctx.kiln_registry
        .register_named(crate::test_support::kiln_name("notes"), &root)
        .unwrap();
    let mut session = Session::new(
        SessionType::Plugin,
        vec![crate::test_support::kiln_name("notes")],
    );
    let mut agent = SessionAgent::internal_defaults(None, None);
    agent.mode = Some("test".into());
    session.agent = Some(agent);
    sm.register_transient(session.clone());
    let loader = DaemonPluginLoader::new(Default::default()).unwrap();
    let (handlers, lua) = loader.handlers();
    am.set_plugin_handlers(handlers, lua);
    crucible_lua::bases_api::register(
        loader.executor().lua(),
        Some(plugin_api::resolver(ctx.clone())),
    )
    .unwrap();
    loader
        .executor()
        .lua()
        .globals()
        .set("sid", session.id.as_str())
        .unwrap();
    (dir, ctx, loader, session)
}
async fn call(loader: &DaemonPluginLoader, body: &str) -> anyhow::Result<serde_json::Value> {
    let lua = loader.executor().lua();
    let value = lua.load(body).eval_async::<mlua::Value>().await?;
    Ok(lua.from_value(value)?)
}
#[tokio::test]
async fn bases_lua_apply_tracks_review_and_refuses_stale_or_unbound_writes() {
    let (dir, ctx, loader, session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    let text = "---\nstatus: todo\n---\nBody\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(text))
        .unwrap();
    let mut events = ctx.event_tx.subscribe();
    let result = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='a.md',key='status',value='done',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(result["status"], "applied");
    assert!(std::fs::read_to_string(root.join("a.md"))
        .unwrap()
        .contains("status: done"));
    assert!(!ctx
        .agents
        .review
        .list_hunks(session.id.as_str())
        .await
        .unwrap()
        .is_empty());
    assert!(session
        .storage_path(ctx.sessions.sessions_root())
        .join("review.jsonl")
        .exists());
    let mut changed = false;
    while let Ok(event) = events.try_recv() {
        changed |= event.event == "base:changed";
    }
    assert!(changed);
    let stale = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='a.md',key='status',value='todo',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(stale["status"], "stale");
    assert!(call(&loader, r#"return cru.kiln.set_property('notes', {path='a.md',key='status',value='todo',ancestor_hash=ancestor})"#).await.unwrap_err().to_string().contains("options.session"));
}
#[tokio::test]
async fn bases_lua_proposals_leave_absent_and_empty_files_unchanged_on_rejection() {
    let (dir, ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln");
    std::fs::write(root.join("empty.md"), "").unwrap();
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(""))
        .unwrap();
    let mut events = ctx.event_tx.subscribe();
    let result = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='empty.md',key='status',value='done',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(result["status"], "proposed");
    let created = call(&loader, r#"return cru.kiln.create_entry('notes', {session=sid,name='new',source={yaml='views: [{type: table, name: All}]'}})"#).await.unwrap();
    assert_eq!(created["status"], "proposed");
    for proposal in ctx.agents.proposals().list(true).unwrap() {
        ctx.agents
            .proposals()
            .reject(&proposal.id, Some("no".into()))
            .unwrap();
    }
    assert_eq!(std::fs::read(root.join("empty.md")).unwrap(), b"");
    assert!(!root.join("new.md").exists());
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.event, "base:changed");
    }
}
#[tokio::test]
async fn bases_policy_refusal_error_timeout_and_source_cleanup_cross_lua() {
    let (dir, ctx, loader, session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    let text = "Body\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    let params = json!({"path":"a.md","key":"status","value":"done","ancestor_hash":crucible_core::note_edit::disk_hash(text)});
    let writer = disposition::Writer {
        ctx: Some(&ctx),
        session: Some(session),
    };
    let root = root.canonicalize().unwrap();
    let set = || operation::execute(BaseOperation::SetProperty, &root, params.clone(), &writer);
    let lua = loader.executor().lua();
    let registry = loader.handlers().0;
    for (body, refusal) in [
        ("return {cancel=true,reason='WIP limit'}", "WIP limit"),
        ("error('broken policy')", "broken policy"),
        ("while true do end", ""),
    ] {
        let previous = crucible_lua::enter_plugin(lua, "base-test");
        lua.load(format!(
            "cru.on('base:before_write', function() {body} end)"
        ))
        .exec()
        .unwrap();
        crucible_lua::set_source(lua, previous);
        let message = match set().await {
            Ok(answer) => {
                assert_eq!(answer["status"], "refused", "{body}");
                answer["reason"].as_str().unwrap().to_owned()
            }
            Err(e) => format!("{e:#}"),
        };
        assert!(message.contains(refusal), "{body}: {message}");
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), text);
        let source = registry
            .all()
            .into_iter()
            .find(|h| h.name.as_str() == "base:before_write")
            .unwrap()
            .source;
        assert_eq!(crucible_lua::clear_source(lua, &registry, &source), 1);
    }
    assert_eq!(set().await.unwrap()["status"], "applied");
}

#[tokio::test]
async fn bases_lua_scope_and_isolation_apply_to_reads_and_writes() {
    let (dir, ctx, loader, mut session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("private.md"), "secret").unwrap();
    // Not hidden, so neither is refused for being a hidden kiln file.
    std::os::unix::fs::symlink(outside.path().join("private.md"), root.join("private.md")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("elsewhere")).unwrap();
    // A read resolves the host through kiln containment before the scope.
    let error = call(&loader, r#"return cru.kiln.query('notes', {session=sid,source={yaml='views: []'},this='private.md'})"#).await.unwrap_err();
    assert!(error.to_string().contains("escapes the kiln"), "{error}");
    // A creation resolves its destination through the session's write scope.
    let error = call(&loader, r#"return cru.kiln.ensure_base('notes', {session=sid,path='elsewhere/private.base',yaml='views: []'})"#).await.unwrap_err();
    assert!(error.to_string().contains("via a symlink"), "{error}");
    assert!(!outside.path().join("private.base").exists());
    session.isolation = Some(json!({"required":true}));
    ctx.sessions.register_transient(session.clone());
    let error = call(
        &loader,
        r#"return cru.kiln.query('notes', {session=sid,source={yaml='views: []'}})"#,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("isolation"));
    session.isolation = None;
    session.kilns.clear();
    ctx.sessions.register_transient(session);
    assert!(call(
        &loader,
        r#"return cru.kiln.query('notes', {session=sid,source={yaml='views: []'}})"#
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("not attached"));
}

#[tokio::test]
async fn bases_lua_repeated_proposals_compose_and_reserve_names() {
    let (dir, ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln");
    std::fs::write(root.join("a.md"), "Body\n").unwrap();
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", crucible_core::note_edit::disk_hash("Body\n"))
        .unwrap();
    call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='a.md',key='status',value='done',ancestor_hash=ancestor})"#).await.unwrap();
    call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='a.md',key='priority',value=2,ancestor_hash=ancestor})"#).await.unwrap();
    let session = ctx
        .sessions
        .get_session(
            loader
                .executor()
                .lua()
                .globals()
                .get::<String>("sid")
                .unwrap()
                .as_str(),
        )
        .unwrap();
    let writer = disposition::Writer {
        ctx: Some(&ctx),
        session: Some(session),
    };
    let proposed = writer
        .proposed_text(&root, &root.join("a.md"))
        .unwrap()
        .unwrap();
    assert!(proposed.contains("status: done") && proposed.contains("priority: 2"));
    assert_eq!(
        std::fs::read_to_string(root.join("a.md")).unwrap(),
        "Body\n"
    );
    let first = call(&loader, r#"return cru.kiln.create_entry('notes', {session=sid,name='new',source={yaml='views: []'}})"#).await.unwrap();
    let second = call(&loader, r#"return cru.kiln.create_entry('notes', {session=sid,name='new',source={yaml='views: []'}})"#).await.unwrap();
    assert_ne!(first["path"], second["path"]);
    std::fs::write(root.join("Board.base"), "views: []\n# user configuration\n").unwrap();
    assert_eq!(call(&loader, r#"return cru.kiln.ensure_base('notes', {session=sid,path='Board.base',yaml='views: []'})"#).await.unwrap()["status"], "unchanged");
    assert!(std::fs::read_to_string(root.join("Board.base"))
        .unwrap()
        .contains("# user configuration"));
}

#[tokio::test]
async fn bases_lua_permission_denial_prevents_creation() {
    let (dir, _, loader, _) = rig_with_permission(WriteMode::Apply, PermissionMode::Deny).await;
    let answer = call(
        &loader,
        r#"return cru.kiln.ensure_base('notes', {session=sid,path='Board.base',yaml='views: []'})"#,
    )
    .await
    .unwrap();
    assert_eq!(answer["status"], "refused", "{answer}");
    assert!(!dir.path().join("kiln/Board.base").exists());
}

#[tokio::test]
async fn bases_shipped_kanban_initializes_moves_and_enforces_policy() {
    let (dir, ctx, loader, _) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    std::fs::create_dir(root.join("tickets")).unwrap();
    std::fs::write(root.join("tickets/a.md"), "---\nstatus: todo\n---\nBody\n").unwrap();
    std::fs::write(root.join("tickets/b.md"), "---\nstatus: doing\n---\nBody\n").unwrap();
    let lua = loader.executor().lua();
    let previous = crucible_lua::enter_plugin(lua, "kanban");
    let module: mlua::Table = lua
        .load(include_str!("../../../../runtime/plugins/kanban/init.luau"))
        .eval()
        .unwrap();
    lua.globals().set("kanban", module).unwrap();
    crucible_lua::set_source(lua, previous);
    call(
        &loader,
        r#"return kanban.commands.kanban.fn({kiln='notes'}, {session_id=sid})"#,
    )
    .await
    .unwrap();
    assert!(root.join("tickets.base").exists());
    call(&loader, r#"return kanban.setup({wip={doing=1}})"#)
        .await
        .unwrap();
    assert!(call(&loader, r#"return kanban.tools.kanban_move.fn({kiln='notes',file='a.md',to='doing'}, {session_id=sid})"#).await.unwrap_err().to_string().contains("WIP"));
    call(&loader, r#"return kanban.tools.kanban_move.fn({kiln='notes',file='a.md',to='done'}, {session_id=sid})"#).await.unwrap();
    std::fs::write(root.join("template.md"), "---\nstatus: doing\n---\nBody").unwrap();
    for options in [
        "source={yaml='newItemFolder: tickets\\nfilters: note.status == \"doing\"\\nviews: []'}",
        "source={yaml='newItemFolder: tickets\\nnewItemTemplate: template.md\\nviews: []'}",
        "source={yaml='newItemFolder: tickets\\nviews: []'},content='---\\nstatus: doing\\n---\\nBody'",
    ] {
        let body = format!("return cru.kiln.create_entry('notes', {{session=sid,name='blocked',{options}}})");
        let answer = call(&loader, &body).await.unwrap();
        assert_eq!(answer["status"], "refused", "{answer}");
        assert!(answer["reason"].as_str().unwrap().contains("WIP"), "{answer}");
        assert!(!root.join("tickets/blocked.md").exists());
    }
    let text = "---\nstatus: doing\n---\nOutside";
    std::fs::write(root.join("outside.md"), text).unwrap();
    ctx.kiln.open(&root.canonicalize().unwrap()).await.unwrap();
    let moved = operation::execute(
        BaseOperation::SetProperty,
        &root.canonicalize().unwrap(),
        json!({"path":"outside.md","key":"file.folder","value":"tickets","ancestor_hash":crucible_core::note_edit::disk_hash(text)}),
        &disposition::Writer { ctx: Some(&ctx), session: None },
    )
    .await
    .unwrap();
    assert_eq!(moved["status"], "refused", "{moved}");
    assert!(moved["reason"].as_str().unwrap().contains("WIP"), "{moved}");
    assert!(root.join("outside.md").exists());
    assert!(!root.join("tickets/outside.md").exists());
    assert!(std::fs::read_to_string(root.join("tickets/a.md"))
        .unwrap()
        .contains("status: done"));
}

#[tokio::test]
async fn bases_lua_checks_the_resolved_creation_destination() {
    for mode in [WriteMode::Apply, WriteMode::Propose] {
        let (dir, _, loader, _) = rig_with_rules(
            mode,
            PermissionConfig {
                default: PermissionMode::Allow,
                deny: vec!["edit:secrets/**".into(), "edit:Ticket 1.md".into()],
                ..Default::default()
            },
        )
        .await;
        let root = dir.path().join("kiln");
        std::fs::create_dir(root.join("secrets")).unwrap();
        std::fs::write(root.join("Ticket.md"), "existing").unwrap();
        for yaml in ["newItemFolder: secrets\nviews: []", "views: []"] {
            loader
                .executor()
                .lua()
                .globals()
                .set("definition", yaml)
                .unwrap();
            let answer = call(&loader, r#"return cru.kiln.create_entry('notes', {session=sid,name='Ticket',source={yaml=definition},file_path='public.md'})"#).await.unwrap();
            assert_eq!(answer["status"], "refused", "{answer}");
            assert!(
                answer["reason"].as_str().unwrap().contains("denied"),
                "{answer}"
            );
        }
        assert!(!root.join("secrets/Ticket.md").exists());
        assert!(!root.join("Ticket 1.md").exists());
    }
}

async fn rpc(
    ctx: &RpcContext,
    op: BaseOperation,
    params: serde_json::Value,
) -> crate::protocol::Response {
    let req =
        serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"base.x","params":params}))
            .unwrap();
    super::handle(op, req, ctx).await
}
fn result(response: crate::protocol::Response) -> serde_json::Value {
    let value = serde_json::to_value(&response).unwrap();
    assert!(value["error"].is_null(), "{value}");
    value["result"].clone()
}

#[tokio::test]
async fn bases_writes_never_follow_a_dangling_symlink_out_of_the_kiln() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.md"), root.join("Untitled.md")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.base"), root.join("Board.base")).unwrap();
    let writer = disposition::Writer::default();
    let created = operation::execute(
        BaseOperation::CreateEntry,
        &root,
        json!({"source":{"yaml":"views: []"}}),
        &writer,
    )
    .await
    .unwrap();
    assert_eq!(created["path"], "Untitled 1.md", "{created}");
    let ensured = operation::execute(
        BaseOperation::EnsureBase,
        &root,
        json!({"path":"Board.base","yaml":"views: []"}),
        &writer,
    )
    .await
    .unwrap();
    assert_eq!(ensured["status"], "unchanged", "{ensured}");
    // The last gate on its own: a write to the link itself is refused.
    let _order = writer.serialize(&root).await.unwrap();
    let direct = writer
        .put(
            &root,
            &root.join("Untitled.md"),
            "x".into(),
            crucible_core::file_write::ExpectedBase::Absent,
        )
        .await;
    assert!(direct.unwrap_err().to_string().contains("escapes"));
    assert!(!outside.path().join("x.md").exists());
    assert!(!outside.path().join("x.base").exists());
}

#[tokio::test]
async fn bases_rpc_writes_work_in_a_kiln_registered_through_a_symlink() {
    let (dir, ctx, _loader, _) = rig(WriteMode::Apply).await;
    let real = dir.path().join("kiln");
    std::os::unix::fs::symlink(&real, dir.path().join("linked")).unwrap();
    ctx.kiln_registry
        .register_named(
            crate::test_support::kiln_name("linked"),
            &dir.path().join("linked"),
        )
        .unwrap();
    std::fs::create_dir(real.join("archive")).unwrap();
    let text = "---\nstatus: todo\n---\nBody\n";
    std::fs::write(real.join("a.md"), text).unwrap();
    std::fs::write(
        real.join("Tasks.base"),
        "views: [{type: table, name: Tasks}]",
    )
    .unwrap();
    let hash = crucible_core::note_edit::disk_hash(text);
    assert_eq!(
        result(rpc(&ctx, BaseOperation::List, json!({"kiln":"linked"})).await),
        json!(["Tasks.base"])
    );
    let set = result(rpc(&ctx, BaseOperation::SetProperty, json!({"kiln":"linked","path":"a.md","key":"status","value":"done","ancestor_hash":hash})).await);
    assert_eq!(set["status"], "applied", "{set}");
    let moved = result(rpc(&ctx, BaseOperation::SetProperty, json!({"kiln":"linked","path":"a.md","key":"file.folder","value":"archive","ancestor_hash":set["ancestor_hash"]})).await);
    assert_eq!(moved["status"], "applied", "{moved}");
    assert!(real.join("archive/a.md").exists());
    let created = result(
        rpc(
            &ctx,
            BaseOperation::CreateEntry,
            json!({"kiln":"linked","source":{"path":"Tasks.base"},"name":"New"}),
        )
        .await,
    );
    assert_eq!(created["path"], "New.md", "{created}");
}

#[tokio::test]
async fn bases_property_write_needs_a_value_or_delete() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let text = "---\nstatus: todo\n---\nBody\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    let hash = crucible_core::note_edit::disk_hash(text);
    let writer = disposition::Writer::default();
    let set = |params| operation::execute(BaseOperation::SetProperty, &root, params, &writer);
    let error = set(json!({"path":"a.md","key":"status","ancestor_hash":hash}))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("needs a value or delete"),
        "{error}"
    );
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), text);
    let error =
        set(json!({"path":"a.md","key":"status","value":"x","delete":true,"ancestor_hash":hash}))
            .await
            .unwrap_err();
    assert!(error.to_string().contains("not both"), "{error}");
    // JSON null is an empty property, as Obsidian writes it.
    let answer = set(json!({"path":"a.md","key":"status","value":null,"ancestor_hash":hash}))
        .await
        .unwrap();
    assert_eq!(answer["status"], "applied", "{answer}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.md")).unwrap(),
        "---\nstatus:\n---\nBody\n"
    );
}

#[tokio::test]
async fn bases_property_writes_keep_every_other_byte_and_skip_no_ops() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let text =
        "---\n# kept comment\nversion: 1.10\nid: 007\ntags: [a, b]\nstatus: todo\n---\nBody\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    let writer = disposition::Writer::default();
    let hash = crucible_core::note_edit::disk_hash(text);
    let answer = operation::execute(
        BaseOperation::SetProperty,
        &root,
        json!({"path":"a.md","key":"status","value":"done","ancestor_hash":hash}),
        &writer,
    )
    .await
    .unwrap();
    assert_eq!(answer["status"], "applied", "{answer}");
    let after = std::fs::read_to_string(root.join("a.md")).unwrap();
    assert_eq!(after, text.replace("status: todo", "status: done"));
    let again = operation::execute(BaseOperation::SetProperty, &root, json!({"path":"a.md","key":"status","value":"done","ancestor_hash":answer["ancestor_hash"]}), &writer).await.unwrap();
    assert_eq!(again["status"], "unchanged", "{again}");
}

#[tokio::test]
async fn bases_group_order_changes_only_that_key() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let block = "# board\nfilters: 'file.ext == \"md\"'\nviews:\n  - type: kanban\n    name: Board # main\n    groupBy:\n      property: note.status\n      direction: ASC\n    groupOrder:\n      - todo\n      - done\n    custom: {keep: yes}\n  - type: table\n    name: Other\n";
    let flow = "# flow board\ncustom: 1\nviews: [{type: kanban, name: Board, groupBy: {property: note.status, direction: ASC}}]\n";
    let writer = disposition::Writer::default();
    for (file, text, expected) in [
        (
            "Block.base",
            block,
            block.replace(
                "    groupOrder:\n      - todo\n      - done\n",
                "    groupOrder: [\"done\",\"todo\"]\n",
            ),
        ),
        ("Flow.base", flow, String::new()),
    ] {
        std::fs::write(root.join(file), text).unwrap();
        let answer = operation::execute(BaseOperation::ReorderGroups, &root, json!({"source":{"path":file},"view":"Board","group_order":["done","todo"],"ancestor_hash":crucible_core::note_edit::disk_hash(text)}), &writer).await.unwrap();
        assert_eq!(answer["status"], "applied", "{answer}");
        let after = std::fs::read_to_string(root.join(file)).unwrap();
        if expected.is_empty() {
            assert!(after.starts_with("# flow board\ncustom: 1\n"), "{after}");
            assert_eq!(
                BaseFile::parse(&after).unwrap().views[0].group_order,
                Some(vec![json!("done"), json!("todo")])
            );
        } else {
            assert_eq!(after, expected);
        }
    }
    // A base with no views has only the implied table, which has no groups.
    std::fs::write(root.join("Empty.base"), "filters: 'true'\n").unwrap();
    let error = operation::execute(BaseOperation::ReorderGroups, &root, json!({"source":{"path":"Empty.base"},"group_order":["x"],"ancestor_hash":crucible_core::note_edit::disk_hash("filters: 'true'\n")}), &writer).await.unwrap_err();
    assert!(error.to_string().contains("groupBy"), "{error}");
    assert_eq!(
        std::fs::read_to_string(root.join("Empty.base")).unwrap(),
        "filters: 'true'\n"
    );
}

#[tokio::test]
async fn bases_policy_that_writes_through_bases_is_refused_not_deadlocked() {
    let (dir, _ctx, loader, _) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    let text = "---\nstatus: todo\n---\nBody\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    std::fs::write(root.join("b.md"), text).unwrap();
    let lua = loader.executor().lua();
    lua.globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(text))
        .unwrap();
    let previous = crucible_lua::enter_plugin(lua, "reentrant");
    lua.load(r#"cru.on('base:before_write', function(_ctx, event)
        if event.path == 'a.md' then
            cru.kiln.set_property('notes', {session=sid,path='b.md',key='status',value='x',ancestor_hash=ancestor})
        end
    end)"#)
    .exec()
    .unwrap();
    crucible_lua::set_source(lua, previous);
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='a.md',key='status',value='done',ancestor_hash=ancestor})"#),
    )
    .await
    .expect("a reentrant Bases write must not deadlock");
    let error = answer.unwrap_err().to_string();
    assert!(error.contains("cannot write through Bases"), "{error}");
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), text);
    assert_eq!(std::fs::read_to_string(root.join("b.md")).unwrap(), text);
}

#[tokio::test]
async fn bases_ask_mode_writes_inherit_the_enclosing_tool_call_grant() {
    let (dir, ctx, loader, session) = rig_with_rules(
        WriteMode::Apply,
        PermissionConfig {
            default: PermissionMode::Ask,
            deny: vec!["edit:secret.md".into()],
            ..Default::default()
        },
    )
    .await;
    let root = dir.path().join("kiln");
    let text = "---\nstatus: todo\n---\nBody\n";
    for name in ["a.md", "secret.md"] {
        std::fs::write(root.join(name), text).unwrap();
    }
    let lua = loader.executor().lua();
    lua.globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(text))
        .unwrap();
    let loader = &loader;
    let write = |path: &'static str| {
        let body = format!("return cru.kiln.set_property('notes', {{session=sid,path='{path}',key='status',value='done',ancestor_hash=ancestor}})");
        async move { call(loader, &body).await.unwrap() }
    };
    // Unattended: nobody can answer the prompt.
    let answer = write("a.md").await;
    assert_eq!(answer["status"], "refused", "{answer}");
    use crate::agent_manager::messaging::review_capture::within_tool_call as messaging;
    // Inside a call that the gate allowed, the call's grant covers the write.
    let answer = messaging(session.id.as_str(), async {
        crate::agent_manager::messaging::review_capture::mark_call_allowed();
        write("a.md").await
    })
    .await;
    assert_eq!(answer["status"], "applied", "{answer}");
    // Before the gate decided, there is no grant to inherit.
    let answer = messaging(session.id.as_str(), write("secret.md")).await;
    assert_eq!(answer["status"], "refused", "{answer}");
    // An operator deny still refuses inside an allowed call.
    let answer = messaging(session.id.as_str(), async {
        crate::agent_manager::messaging::review_capture::mark_call_allowed();
        write("secret.md").await
    })
    .await;
    assert_eq!(answer["status"], "refused", "{answer}");
    assert_eq!(
        std::fs::read_to_string(root.join("secret.md")).unwrap(),
        text
    );
    let _ = ctx;
}

#[tokio::test]
async fn bases_lua_inside_a_session_cannot_act_for_another() {
    let (dir, ctx, loader, session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    let text = "---\nstatus: todo\n---\nBody\n";
    std::fs::write(root.join("a.md"), text).unwrap();
    let mut other = Session::new(
        SessionType::Plugin,
        vec![crate::test_support::kiln_name("notes")],
    );
    other.agent = session.agent.clone();
    ctx.sessions.register_transient(other.clone());
    let lua = loader.executor().lua();
    lua.globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(text))
        .unwrap();
    lua.globals().set("other", other.id.as_str()).unwrap();
    let _inside = crucible_lua::enter_session(lua, Some(session.id.as_str()));
    let error = call(&loader, r#"return cru.kiln.set_property('notes', {session=other,path='a.md',key='status',value='done',ancestor_hash=ancestor})"#).await.unwrap_err();
    assert!(
        error.to_string().contains("cannot act for session"),
        "{error}"
    );
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), text);
    // With no session named, the call acts for the session it runs in.
    let answer = call(&loader, r#"return cru.kiln.set_property('notes', {path='a.md',key='status',value='done',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(answer["status"], "applied", "{answer}");
}

#[tokio::test]
async fn bases_lua_folder_moves_match_rpc_and_policy_sees_final_bytes() {
    let (dir, ctx, loader, _) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln").canonicalize().unwrap();
    std::fs::create_dir(root.join("archive")).unwrap();
    let text = "---\nstatus: todo\n---\nSee [[notes/a]] and [[b]].\n";
    std::fs::create_dir(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/a.md"), text).unwrap();
    std::fs::write(root.join("b.md"), "Link to [[notes/a]].\n").unwrap();
    ctx.kiln.open_and_process(&root, false).await.unwrap();
    let lua = loader.executor().lua();
    lua.globals()
        .set("ancestor", crucible_core::note_edit::disk_hash(text))
        .unwrap();
    let previous = crucible_lua::enter_plugin(lua, "seen");
    lua.load(
        "seen = {}\ncru.on('base:before_write', function(_ctx, event) seen[event.path] = event.content end)",
    )
    .exec()
    .unwrap();
    crucible_lua::set_source(lua, previous);
    let answer = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='notes/a.md',key='file.folder',value='archive',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(answer["status"], "applied", "{answer}");
    assert_eq!(answer["path"], "archive/a.md");
    let moved = std::fs::read_to_string(root.join("archive/a.md")).unwrap();
    assert!(moved.contains("[[archive/a]]"), "{moved}");
    let seen: mlua::Table = lua.globals().get("seen").unwrap();
    assert_eq!(seen.get::<String>("archive/a.md").unwrap(), moved);
    assert_eq!(
        seen.get::<String>("b.md").unwrap(),
        "Link to [[archive/a]].\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.md")).unwrap(),
        "Link to [[archive/a]].\n"
    );
}

#[tokio::test]
async fn bases_kanban_wip_limit_counts_pending_proposals() {
    let (dir, _ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln");
    std::fs::create_dir(root.join("tickets")).unwrap();
    for name in ["a", "b"] {
        std::fs::write(
            root.join(format!("tickets/{name}.md")),
            "---\nstatus: todo\n---\nBody\n",
        )
        .unwrap();
    }
    std::fs::write(
        root.join("tickets.base"),
        "filters: 'file.inFolder(\"tickets\")'\nviews: [{type: kanban, name: Board, groupBy: {property: note.status, direction: ASC}}]\n",
    )
    .unwrap();
    let lua = loader.executor().lua();
    let previous = crucible_lua::enter_plugin(lua, "kanban");
    let module: mlua::Table = lua
        .load(include_str!("../../../../runtime/plugins/kanban/init.luau"))
        .eval()
        .unwrap();
    lua.globals().set("kanban", module).unwrap();
    crucible_lua::set_source(lua, previous);
    call(&loader, r#"return kanban.setup({wip={doing=1}})"#)
        .await
        .unwrap();
    let first = call(&loader, r#"return kanban.tools.kanban_move.fn({kiln='notes',file='a.md',to='doing'}, {session_id=sid})"#).await.unwrap();
    assert_eq!(first["status"], "proposed", "{first}");
    let second = call(&loader, r#"return kanban.tools.kanban_move.fn({kiln='notes',file='b.md',to='doing'}, {session_id=sid})"#).await.unwrap_err();
    assert!(second.to_string().contains("WIP"), "{second}");
}

/// A kiln with a note that links to itself and one inbound link, indexed.
async fn move_fixture(ctx: &RpcContext, root: &Path) -> String {
    std::fs::create_dir_all(root.join("archive")).unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    let text = "---\nstatus: todo\n---\nSee [[notes/a]].\n";
    std::fs::write(root.join("notes/a.md"), text).unwrap();
    std::fs::write(root.join("b.md"), "Link to [[notes/a]].\n").unwrap();
    ctx.kiln.open_and_process(root, false).await.unwrap();
    crucible_core::note_edit::disk_hash(text)
}

#[tokio::test]
async fn bases_folder_move_in_propose_mode_is_one_proposal_that_accept_applies() {
    let (dir, ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln").canonicalize().unwrap();
    let hash = move_fixture(&ctx, &root).await;
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", hash)
        .unwrap();
    let answer = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='notes/a.md',key='file.folder',value='archive',ancestor_hash=ancestor})"#).await.unwrap();
    assert_eq!(answer["status"], "proposed", "{answer}");
    assert!(root.join("notes/a.md").exists() && !root.join("archive/a.md").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("b.md")).unwrap(),
        "Link to [[notes/a]].\n"
    );
    let id: crucible_core::proposal::ProposalId =
        serde_json::from_value(answer["proposal"].clone()).unwrap();
    let proposal = ctx.agents.proposals().get(&id).unwrap();
    let mut entries = proposal
        .writes
        .iter()
        .map(|w| (w.path.as_str(), w.remove))
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        [
            ("archive/a.md", false),
            ("b.md", false),
            ("notes/a.md", true)
        ]
    );
    let accepted = ctx
        .agents
        .proposals()
        .accept(&id, std::slice::from_ref(&root))
        .await
        .unwrap();
    assert_eq!(
        accepted.state,
        crucible_core::proposal::ProposalState::Accepted
    );
    assert!(!root.join("notes/a.md").exists());
    assert!(std::fs::read_to_string(root.join("archive/a.md"))
        .unwrap()
        .contains("[[archive/a]]"));
    assert_eq!(
        std::fs::read_to_string(root.join("b.md")).unwrap(),
        "Link to [[archive/a]].\n"
    );
}

#[tokio::test]
async fn bases_proposed_move_conflicts_when_the_old_note_changes_before_accept() {
    let (dir, ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln").canonicalize().unwrap();
    let hash = move_fixture(&ctx, &root).await;
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", hash)
        .unwrap();
    let answer = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='notes/a.md',key='file.folder',value='archive',ancestor_hash=ancestor})"#).await.unwrap();
    let id: crucible_core::proposal::ProposalId =
        serde_json::from_value(answer["proposal"].clone()).unwrap();
    std::fs::write(root.join("notes/a.md"), "edited by a person\n").unwrap();
    let decided = ctx
        .agents
        .proposals()
        .accept(&id, std::slice::from_ref(&root))
        .await
        .unwrap();
    let crucible_core::proposal::ProposalState::Conflicted { files } = &decided.state else {
        panic!("{:?}", decided.state);
    };
    assert!(files
        .iter()
        .any(|f| f.path == "notes/a.md" && f.merged_text == "edited by a person\n"));
    // Nothing of the set landed: the edit and the old path stay.
    assert_eq!(
        std::fs::read_to_string(root.join("notes/a.md")).unwrap(),
        "edited by a person\n"
    );
    assert!(!root.join("archive/a.md").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("b.md")).unwrap(),
        "Link to [[notes/a]].\n"
    );
}

#[tokio::test]
async fn bases_folder_move_runs_the_policy_on_each_rewritten_source() {
    for mode in [WriteMode::Apply, WriteMode::Propose] {
        let (dir, ctx, loader, _) = rig(mode).await;
        let root = dir.path().join("kiln").canonicalize().unwrap();
        let hash = move_fixture(&ctx, &root).await;
        let lua = loader.executor().lua();
        lua.globals().set("ancestor", hash).unwrap();
        let previous = crucible_lua::enter_plugin(lua, "guard");
        lua.load(
            r#"seen = {}
cru.on('base:before_write', function(_ctx, event)
    seen[event.path] = event.content
    if event.path == 'b.md' then return {cancel=true, reason='b is frozen'} end
end)"#,
        )
        .exec()
        .unwrap();
        crucible_lua::set_source(lua, previous);
        let answer = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='notes/a.md',key='file.folder',value='archive',ancestor_hash=ancestor})"#).await.unwrap();
        assert_eq!(answer["status"], "refused", "{answer}");
        assert_eq!(answer["path"], "b.md", "{answer}");
        assert!(answer["reason"].as_str().unwrap().contains("b is frozen"));
        let seen: mlua::Table = lua.globals().get("seen").unwrap();
        assert_eq!(
            seen.get::<String>("b.md").unwrap(),
            "Link to [[archive/a]].\n"
        );
        assert!(root.join("notes/a.md").exists() && !root.join("archive/a.md").exists());
        assert!(ctx.agents.proposals().list(false).unwrap().is_empty());
    }
}

#[tokio::test]
async fn bases_proposed_move_reviews_as_a_rename_and_accept_updates_the_index() {
    let (dir, ctx, loader, _) = rig(WriteMode::Propose).await;
    let root = dir.path().join("kiln").canonicalize().unwrap();
    let hash = move_fixture(&ctx, &root).await;
    loader
        .executor()
        .lua()
        .globals()
        .set("ancestor", hash)
        .unwrap();
    let answer = call(&loader, r#"return cru.kiln.set_property('notes', {session=sid,path='notes/a.md',key='file.folder',value='archive',ancestor_hash=ancestor})"#).await.unwrap();
    let id: crucible_core::proposal::ProposalId =
        serde_json::from_value(answer["proposal"].clone()).unwrap();
    let store = ctx.agents.proposals();
    // The review shows one rename against the old text, and the linking note.
    let diff = store.diff_files(&id).unwrap();
    let mut shown = diff
        .iter()
        .map(|f| (f.path.as_str(), f.status.clone()))
        .collect::<Vec<_>>();
    shown.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(
        shown,
        [
            (
                "archive/a.md",
                crucible_core::diff::FileStatus::Renamed {
                    from: "notes/a.md".into()
                }
            ),
            ("b.md", crucible_core::diff::FileStatus::Modified),
        ]
    );
    let text = store
        .diff_text(
            &id,
            &crucible_core::session::PhysicalRoot::from_top_level(root.clone()),
            "archive/a.md",
        )
        .unwrap();
    assert!(text.base_text.unwrap().contains("[[notes/a]]"));
    assert_eq!(store.get(&id).unwrap().title, "Change 2 notes");
    // Accepting one half of the move takes the other half with it.
    let (tx, mut rx) = crate::EventBus::channel(256);
    let km = crate::kiln_manager::KilnManager::with_event_tx(
        tx,
        None,
        crucible_core::config::default_max_precognition_chars(),
    );
    km.open_and_process(&root, false).await.unwrap();
    while rx.try_recv().is_ok() {}
    let req = serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"proposal.accept",
        "params":{"id":id,"paths":["archive/a.md","b.md"]}}))
    .unwrap();
    let response =
        serde_json::to_value(crate::proposals::handle_proposal_accept(req, store, &km).await)
            .unwrap();
    assert_eq!(
        response["result"]["state"]["kind"], "accepted",
        "{response}"
    );
    assert!(!root.join("notes/a.md").exists());
    // The index follows at once: the backlink names the new path, and the
    // bus says the delete and the insert were one move.
    let notes = km.get(&root).await.unwrap().as_note_store();
    let inbound = notes.inbound_links("archive/a.md").await.unwrap();
    assert!(
        inbound.iter().any(|l| l.source_path == "b.md"),
        "{inbound:?}"
    );
    let mut renamed = false;
    while let Ok(event) = rx.try_recv() {
        renamed |= event.event == "note:renamed";
    }
    assert!(renamed);
}
