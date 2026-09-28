//! Named-kiln operations backed by the daemon's Bases and review owners.
use super::*;
use crucible_core::bases::BaseOperation;
use strum::IntoEnumIterator;

pub type BasesResolver = Arc<
    dyn Fn(
            BaseOperation,
            Option<String>,
            String,
            serde_json::Value,
        ) -> BoxFuture<'static, Result<serde_json::Value, String>>
        + Send
        + Sync,
>;

/// The session a Bases call acts for.
///
/// Inside a plugin tool, command or session hook, the host already entered
/// the session of that call; a plugin may repeat it but not name another,
/// because that would borrow the other session's permissions and ledger.
/// Outside any session the plugin names the session explicitly.
fn acting_session(
    ambient: Option<String>,
    requested: Option<String>,
    operation: BaseOperation,
) -> Result<Option<String>, String> {
    match (ambient, requested) {
        (Some(ambient), Some(requested)) if ambient != requested => Err(format!(
            "cru.kiln.{} runs inside session {ambient} and cannot act for session {requested}",
            operation.name()
        )),
        (Some(ambient), _) => Ok(Some(ambient)),
        (None, requested) => Ok(requested),
    }
}

pub fn register(lua: &Lua, resolver: Option<BasesResolver>) -> Result<(), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let kiln: Table = cru.get("kiln")?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.kiln", kiln);
    for operation in BaseOperation::iter() {
        let resolver = resolver.clone();
        ns.async_func(
            operation.name(),
            "(kiln: string, options: { [string]: any }?) -> any",
            move |lua, (kiln, options): (String, Option<Table>)| {
                let resolver = resolver.clone();
                async move {
                    let resolver = resolver
                        .ok_or_else(|| mlua::Error::runtime("Bases requires a daemon runtime"))?;
                    let options = match options {
                        Some(options) => options,
                        None => lua.create_table()?,
                    };
                    let session = acting_session(
                        crate::plugin_context::current_session(&lua),
                        options.get("session")?,
                        operation,
                    )
                    .map_err(mlua::Error::runtime)?;
                    let options = lua.from_value(Value::Table(options))?;
                    let result = resolver(operation, session, kiln, options)
                        .await
                        .map_err(mlua::Error::runtime)?;
                    lua.to_value(&result)
                }
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each `cru.kiln` Bases function gives its own operation to the
    /// resolver, with the kiln name and the options of the Lua call.
    #[tokio::test]
    async fn each_kiln_function_sends_its_operation_to_the_resolver() {
        let lua = Lua::new();
        super::super::register_vault_module(&lua).unwrap();
        let resolver: BasesResolver = Arc::new(|operation, session, kiln, options| {
            Box::pin(async move {
                Ok(serde_json::json!({
                    "operation": operation.name(),
                    "session": session,
                    "kiln": kiln,
                    "options": options,
                }))
            })
        });
        register(&lua, Some(resolver)).unwrap();
        for operation in BaseOperation::iter() {
            let name = operation.name();
            let script =
                format!("return cru.kiln.{name}('notes', {{ session = 's1', key = 'k' }})");
            let answer: serde_json::Value = lua
                .from_value(lua.load(&script).eval_async::<Value>().await.unwrap())
                .unwrap();
            assert_eq!(
                answer["operation"], name,
                "cru.kiln.{name} sent another operation"
            );
            assert_eq!(answer["session"], "s1");
            assert_eq!(answer["kiln"], "notes");
            assert_eq!(answer["options"]["key"], "k");
        }
    }

    #[test]
    fn a_call_inside_a_session_cannot_name_another_session() {
        let op = BaseOperation::SetProperty;
        assert_eq!(
            acting_session(Some("a".into()), None, op),
            Ok(Some("a".into()))
        );
        assert_eq!(
            acting_session(Some("a".into()), Some("a".into()), op),
            Ok(Some("a".into()))
        );
        assert!(acting_session(Some("a".into()), Some("b".into()), op)
            .unwrap_err()
            .contains("cannot act for session b"));
        assert_eq!(
            acting_session(None, Some("b".into()), op),
            Ok(Some("b".into()))
        );
        assert_eq!(acting_session(None, None, op), Ok(None));
    }
}
