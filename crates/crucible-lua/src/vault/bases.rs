//! Named-kiln operations backed by the daemon's Bases and review owners.
use super::*;
use strum::IntoEnumIterator;

#[derive(Clone, Copy, strum::EnumIter)]
pub enum BaseOperation {
    Query,
    SetProperty,
    CreateEntry,
    EnsureBase,
}
impl BaseOperation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::SetProperty => "set_property",
            Self::CreateEntry => "create_entry",
            Self::EnsureBase => "ensure_base",
        }
    }
}
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

pub fn register(lua: &Lua, resolver: Option<BasesResolver>) -> Result<(), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let kiln: Table = cru.get("kiln")?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.kiln", kiln);
    for operation in BaseOperation::iter() {
        let resolver = resolver.clone();
        ns.async_func(
            operation.name(),
            "(kiln: string, options: { [string]: any }) -> { [string]: any }",
            move |lua, (kiln, options): (String, Table)| {
                let resolver = resolver.clone();
                let session = options.get::<Option<String>>("session");
                async move {
                    let resolver = resolver
                        .ok_or_else(|| mlua::Error::runtime("Bases requires a daemon runtime"))?;
                    let options = lua.from_value(Value::Table(options))?;
                    let result = resolver(operation, session?, kiln, options)
                        .await
                        .map_err(mlua::Error::runtime)?;
                    lua.to_value(&result)
                }
            },
        )?;
    }
    Ok(())
}
