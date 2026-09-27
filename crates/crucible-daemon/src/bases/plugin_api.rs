//! `cru.kiln` Bases calls: Lua names a kiln and acts for one session.
use super::operation::{self, BaseOperation};
use super::*;
use crate::{rpc::RpcContext, tools::fs_scope::FsScope};
use crucible_core::{config::KilnName, session::Session};
use crucible_lua::bases_api::BasesResolver;
use serde_json::{json, Value as Json};
use std::sync::Arc;

pub(super) fn scope(ctx: &RpcContext, session: &Session, root: &Path) -> FsScope {
    FsScope::kiln(
        root,
        crate::agent_manager::scope::session_containment(
            session,
            ctx.sessions.sessions_root(),
            &ctx.kiln_registry,
        ),
    )
}
pub(crate) fn resolver(ctx: Arc<RpcContext>) -> BasesResolver {
    Arc::new(move |op, session, kiln, params| {
        let ctx = ctx.clone();
        Box::pin(async move {
            execute(&ctx, op, session.as_deref(), &kiln, params)
                .await
                .map_err(|e| format!("{e:#}"))
        })
    })
}
/// Admit `session` to the kiln `name` at `root`: the kiln is attached,
/// trusted, and any isolation the session requires is active.
fn admit(
    ctx: &RpcContext,
    op: BaseOperation,
    s: &Session,
    name: &KilnName,
    root: &Path,
) -> Result<()> {
    ensure!(
        s.kilns.contains(name),
        "Kiln is not attached to this session"
    );
    ctx.agents.refuse_untrusted(
        s.agent.as_ref(),
        std::slice::from_ref(&root.to_path_buf()),
        s.workspace.as_deref(),
    )?;
    let isolation = ctx.agents.isolation();
    if crate::session_lifecycle::required_isolation(s).is_some() {
        ensure!(
            isolation
                .as_ref()
                .and_then(|i| i.get(s.id.as_str()))
                .is_some(),
            "Session requires isolation that is not active"
        );
    }
    if let Some(isolation) = isolation {
        if let Some(reason) = crate::tools_bridge::isolated_session_refusal(
            &isolation,
            op.name(),
            s.id.as_str(),
            crucible_core::traits::tools::ToolSurface::Host,
            "cru.kiln",
        ) {
            anyhow::bail!(reason);
        }
    }
    Ok(())
}
async fn execute(
    ctx: &RpcContext,
    op: BaseOperation,
    session: Option<&str>,
    kiln: &str,
    mut params: Json,
) -> Result<Json> {
    let name = KilnName::parse(kiln)?;
    let root = kiln_root(ctx, &name).await?;
    let session = session
        .map(|id| {
            ctx.sessions
                .get_session(id)
                .ok_or_else(|| operation::not_found(format!("Session not found: {id}")))
        })
        .transpose()?;
    if let Some(s) = &session {
        admit(ctx, op, s, &name, &root)?;
    }
    ensure!(
        !op.writes() || session.is_some(),
        "Bases writes require options.session"
    );
    if !params.is_object() {
        // An empty Lua table arrives as an empty list.
        params = json!({});
    }
    params["kiln"] = json!(name);
    let writer = super::disposition::Writer {
        ctx: Some(ctx),
        session,
    };
    operation::execute(op, &root, params, &writer).await
}
