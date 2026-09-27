//! Lua names a kiln and an explicit session; neither is inferred from VM globals.
use super::*;
use crate::{rpc::RpcContext, tools::fs_scope::FsScope};
use crucible_core::{config::KilnName, session::Session};
use crucible_lua::bases_api::{BaseOperation, BasesResolver};
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
async fn execute(
    ctx: &RpcContext,
    op: BaseOperation,
    session: Option<&str>,
    kiln: &str,
    mut params: Json,
) -> Result<Json> {
    let name = KilnName::parse(kiln)?;
    let root = ctx
        .kiln_registry
        .resolve(&name)
        .path()
        .ok_or_else(|| anyhow::anyhow!("Kiln is unavailable: {name}"))?
        .canonicalize()?;
    let session = session
        .map(|id| {
            ctx.sessions
                .get_session(id)
                .ok_or_else(|| anyhow::anyhow!("Session not found: {id}"))
        })
        .transpose()?;
    if let Some(s) = &session {
        ensure!(
            s.kilns.contains(&name),
            "Kiln is not attached to this session"
        );
        ctx.agents.refuse_untrusted(
            s.agent.as_ref(),
            std::slice::from_ref(&root),
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
    }
    params["kiln"] = json!(name);
    if matches!(op, BaseOperation::Query) {
        let request: Query = serde_json::from_value(params)?;
        let scope = session.as_ref().map(|s| scope(ctx, s, &root));
        return Ok(serde_json::to_value(
            query_scoped(&root, &request, scope.as_ref()).await?,
        )?);
    }
    let session = session.ok_or_else(|| anyhow::anyhow!("Bases writes require options.session"))?;
    ctx.agents
        .bases_write_permission(&session, &params)
        .await
        .map_err(anyhow::Error::msg)?;
    let writer = super::disposition::Writer {
        ctx: Some(ctx),
        session: Some(session),
    };
    match op {
        BaseOperation::Query => unreachable!(),
        BaseOperation::SetProperty => write::set_property_with(&root, &params, &writer).await,
        BaseOperation::CreateEntry => write::create_entry_with(&root, &params, &writer).await,
        BaseOperation::EnsureBase => {
            let path = params["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("path required"))?;
            ensure!(
                Path::new(path).extension().is_some_and(|x| x == "base"),
                "Expected a .base file"
            );
            let session = writer.session.as_ref().unwrap();
            let path = scope(ctx, session, &root)
                .resolve_for_write(path)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let _guard = crate::file_write::lock(path.as_path()).await;
            if path.as_path().exists() {
                return Ok(json!({"ok":true,"status":"exists"}));
            }
            let yaml = params["yaml"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("yaml required"))?;
            BaseFile::parse(yaml)?;
            writer
                .put(
                    &root,
                    path.as_path(),
                    yaml.into(),
                    crucible_core::file_write::ExpectedBase::Absent,
                    &params,
                )
                .await
        }
    }
}
