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
    assert_eq!(result["ok"], true);
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
    assert_eq!(stale["ok"], false);
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
    let lua = loader.executor().lua();
    let registry = loader.handlers().0;
    for body in [
        "return {cancel=true,reason='WIP limit'}",
        "error('broken policy')",
        "while true do end",
    ] {
        let previous = crucible_lua::enter_plugin(lua, "base-test");
        lua.load(format!(
            "cru.on('base:before_write', function() {body} end)"
        ))
        .exec()
        .unwrap();
        crucible_lua::set_source(lua, previous);
        let result = write::set_property_with(&root, &params, &writer).await;
        assert!(result.is_err(), "policy must refuse: {body}");
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), text);
        let source = registry
            .all()
            .into_iter()
            .find(|h| h.name.as_str() == "base:before_write")
            .unwrap()
            .source;
        assert_eq!(crucible_lua::clear_source(lua, &registry, &source), 1);
    }
    assert_eq!(
        write::set_property_with(&root, &params, &writer)
            .await
            .unwrap()["ok"],
        true
    );
}

#[tokio::test]
async fn bases_lua_scope_and_isolation_apply_to_reads_and_writes() {
    let (dir, ctx, loader, mut session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    std::fs::create_dir_all(root.join(".crucible/sessions/other")).unwrap();
    std::fs::write(root.join(".crucible/sessions/other/private.md"), "secret").unwrap();
    let error = call(&loader, r#"return cru.kiln.query('notes', {session=sid,source={yaml='views: []'},this='.crucible/sessions/other/private.md'})"#).await.unwrap_err();
    assert!(!error.to_string().is_empty());
    assert!(call(&loader, r#"return cru.kiln.ensure_base('notes', {session=sid,path='.crucible/sessions/other/private.base',yaml='views: []'})"#).await.is_err());
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
    assert_eq!(call(&loader, r#"return cru.kiln.ensure_base('notes', {session=sid,path='Board.base',yaml='views: []'})"#).await.unwrap()["status"], "exists");
    assert!(std::fs::read_to_string(root.join("Board.base"))
        .unwrap()
        .contains("# user configuration"));
}

#[tokio::test]
async fn bases_lua_permission_denial_prevents_creation() {
    let (dir, _, loader, _) = rig_with_permission(WriteMode::Apply, PermissionMode::Deny).await;
    assert!(call(
        &loader,
        r#"return cru.kiln.ensure_base('notes', {session=sid,path='Board.base',yaml='views: []'})"#
    )
    .await
    .is_err());
    assert!(!dir.path().join("kiln/Board.base").exists());
}

#[tokio::test]
async fn bases_nested_tool_write_retains_outer_review_attribution() {
    let (dir, ctx, loader, session) = rig(WriteMode::Apply).await;
    let root = dir.path().join("kiln");
    ctx.agents
        .review
        .open_or_restore(
            session.id.as_str(),
            &session.storage_path(ctx.sessions.sessions_root()),
            std::slice::from_ref(&root),
        )
        .await
        .unwrap();
    let outer = ctx
        .agents
        .review
        .open_bracket(session.id.as_str())
        .await
        .unwrap();
    crate::agent_manager::messaging::review_capture::CURRENT_CAPTURE.scope(
        (session.id.to_string(), std::cell::Cell::new(true)),
        call(&loader, r#"return cru.kiln.ensure_base('notes', {session=sid,path='Board.base',yaml='views: []'})"#)
    ).await.unwrap();
    ctx.agents
        .review
        .close(session.id.as_str(), outer, "plugin-tool", 1)
        .await
        .unwrap();
    let hunks = ctx
        .agents
        .review
        .list_hunks(session.id.as_str())
        .await
        .unwrap();
    assert!(!hunks.is_empty());
    assert!(hunks.iter().all(|h| !h.is_external()));
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
        assert!(call(&loader, &body).await.unwrap_err().to_string().contains("WIP"));
        assert!(!root.join("tickets/blocked.md").exists());
    }
    let text = "---\nstatus: doing\n---\nOutside";
    std::fs::write(root.join("outside.md"), text).unwrap();
    assert!(write::move_entry(&root, &json!({"path":"outside.md","value":"tickets","ancestor_hash":crucible_core::note_edit::disk_hash(text)}), &ctx.kiln, &disposition::Writer {ctx: Some(&ctx), session: None}).await.unwrap_err().to_string().contains("WIP"));
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
            assert!(call(&loader, r#"return cru.kiln.create_entry('notes', {session=sid,name='Ticket',source={yaml=definition},file_path='public.md'})"#).await.is_err());
        }
        assert!(!root.join("secrets/Ticket.md").exists());
        assert!(!root.join("Ticket 1.md").exists());
    }
}
