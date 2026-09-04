//! `cru.embed(kiln, text)`: the vector a named kiln's embedder gives a text.
//!
//! A retrieval strategy that re-embeds a block with a prefix, or embeds a
//! query it composed, needs the SAME model the kiln was indexed with. The
//! host owns that model, so Lua names the kiln and the host maps the name to
//! its provider. A dimension mismatch is the caller's problem: the function
//! answers whatever the provider answers.
//!
//! ```lua
//! local vector = cru.embed("notes", "Title > Section\n\nThe block text")
//! ```
//!
//! [`register_embed_module`] installs a stub that raises; the daemon binds
//! the real function through [`register_embed_resolver`]. The stub raises
//! rather than answering an empty table so a strategy never stores a vector
//! of length zero by mistake.

use crate::error::LuaError;
use crucible_core::enrichment::EmbeddingProvider;
use futures_util::future::BoxFuture;
use mlua::{Lua, Table};
use std::sync::Arc;

/// Maps a kiln NAME to the embedding provider its index was built with.
///
/// The same seam as `KilnRepositoryResolver`: the host owns the map, and the
/// error string reaches Lua as-is, so it must name the kiln and never a
/// directory.
pub type EmbedResolver = Arc<
    dyn Fn(&str) -> BoxFuture<'static, Result<Arc<dyn EmbeddingProvider>, String>> + Send + Sync,
>;

/// The declared Luau type of `cru.embed`.
const DECL: &str = "(kiln: string, text: string) -> { number }";

/// The message the stub raises.
const NO_EMBEDDER: &str = "cru.embed: no embedder bound";

/// Install the raising stub on `cru`.
pub fn register_embed_module(lua: &Lua) -> Result<(), LuaError> {
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru);
    root.async_func(
        "embed",
        DECL,
        |_lua, (_kiln, _text): (String, String)| async move {
            Err::<Vec<f32>, _>(mlua::Error::runtime(NO_EMBEDDER))
        },
    )?;
    root.doc(
        "embed",
        "Raises when no embedder is bound, and when the kiln name does not resolve.",
    );
    Ok(())
}

/// Bind `cru.embed` to a resolver, replacing the stub.
pub fn register_embed_resolver(lua: &Lua, resolver: EmbedResolver) -> Result<(), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru);
    root.async_func(
        "embed",
        DECL,
        move |_lua, (kiln, text): (String, String)| {
            let resolver = Arc::clone(&resolver);
            async move {
                let provider = resolver(&kiln).await.map_err(mlua::Error::runtime)?;
                provider
                    .embed(&text)
                    .await
                    .map_err(|e| mlua::Error::runtime(format!("cru.embed: {e}")))
            }
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    /// A provider that answers with a fixed vector, whatever the text.
    struct Fixed(Vec<f32>);

    #[async_trait::async_trait]
    impl EmbeddingProvider for Fixed {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(self.0.clone())
        }
        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| self.0.clone()).collect())
        }
        fn model_name(&self) -> &str {
            "fixed"
        }

        fn provider_kind(&self) -> &'static str {
            "mock"
        }
        fn dimensions(&self) -> usize {
            self.0.len()
        }
        fn provider_name(&self) -> &str {
            "test"
        }
        async fn list_models(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn the_stub_raises_and_names_itself() {
        let lua = TestLuaBuilder::new().with_vault().build();
        let err = lua
            .load(r#"return cru.embed("notes", "text")"#)
            .eval::<mlua::Value>()
            .expect_err("unbound");
        assert!(err.to_string().contains(NO_EMBEDDER), "{err}");
    }

    #[tokio::test]
    async fn a_bound_resolver_answers_the_named_kiln_and_refuses_the_rest() {
        let lua = TestLuaBuilder::new().with_vault().build();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(Fixed(vec![0.5, -1.0, 2.0]));
        let resolver: EmbedResolver = Arc::new(move |name: &str| {
            let answer = if name == "notes" {
                Ok(Arc::clone(&provider))
            } else {
                Err(format!("kiln '{name}' is not registered"))
            };
            Box::pin(async move { answer })
        });
        register_embed_resolver(&lua, resolver).unwrap();

        let vector: Vec<f64> = lua
            .load(r#"return cru.embed("notes", "any text")"#)
            .eval_async()
            .await
            .unwrap();
        assert_eq!(vector, vec![0.5, -1.0, 2.0]);

        let err = lua
            .load(r#"return cru.embed("elsewhere", "any text")"#)
            .eval_async::<mlua::Value>()
            .await
            .expect_err("unregistered kiln");
        assert!(err.to_string().contains("elsewhere"), "{err}");
    }
}
