//! Who is asking, on the plugin routes.
//!
//! # This is not a security boundary
//!
//! Read this before you change anything here, and before you cite it as a
//! defence anywhere else.
//!
//! A block is same-origin script. It can set any header it likes, so it can
//! call itself `app` and reach everything below. Nothing here stops a hostile
//! block, and nothing here is meant to. What this *is*:
//!
//! - the **seam** a real boundary attaches to, once blocks run in a sandboxed
//!   opaque origin and speak through a bridge that stamps the identity rather
//!   than accepting one. Until then the identity is asserted, not proved.
//! - an **honest error for the accidental case** — a block that reaches for
//!   another plugin's command, or for the install route, gets told no instead
//!   of succeeding by inheriting the app's session.
//!
//! So: no `auth`, no `verify`, no `authorize` in any name in this file. Those
//! words would promise a check that is not happening, and the next reader
//! would build on the promise.
//!
//! # Why absent is refused
//!
//! The identity is three-valued — [`PluginCaller::App`], a named plugin, or
//! absent — and absent is a refusal rather than a pass. A gate whose default
//! is open is bypassed by *omitting* the header, which is easier than forging
//! one, and a test that only checks "plugin X cannot call plugin Y" passes
//! while that bypass works. The first draft of this step had exactly that
//! shape. See `docs/Meta/Analysis/Plugin API Plan.md`, step 1.
//!
//! # The identity a block mounts under is the note author's
//!
//! `BlockProps.plugin` comes from the first line of the ```plugin fence
//! (`components/blocks/registry.ts`, `components/blocks/mount.ts`), so whoever
//! wrote the note picked the string the block mounts under, and the block
//! sends that string here. The caller-supplied identity is therefore already
//! caller-supplied one layer up, before any script forges anything. A header
//! does not fix that; only isolating blocks does.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::WebError;

/// The header a caller declares itself in.
pub const PLUGIN_CALLER_HEADER: &str = "x-crucible-plugin";

/// The value the first-party frontends send: the web app itself.
pub const APP_CALLER: &str = "app";

/// Who is calling a plugin route.
///
/// Deliberately two variants and no `Default`: the third value — nobody said —
/// is the extractor's rejection, not a variant anything can construct and
/// forget to handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCaller {
    /// The app's own UI. Reaches every plugin route.
    App,
    /// Script drawing for one plugin. Reaches that plugin's own things.
    Plugin(String),
}

impl PluginCaller {
    /// True when this caller may act on `plugin`'s behalf.
    pub fn speaks_for(&self, plugin: &str) -> bool {
        match self {
            PluginCaller::App => true,
            PluginCaller::Plugin(name) => name == plugin,
        }
    }

    /// Refuse anyone but the app. `what` names the thing being refused, so the
    /// message says which route said no.
    pub fn require_app(&self, what: &str) -> Result<(), WebError> {
        match self {
            PluginCaller::App => Ok(()),
            PluginCaller::Plugin(name) => Err(WebError::Forbidden(format!(
                "plugin `{name}` may not {what}: that is the app's to do"
            ))),
        }
    }

    /// Refuse a caller acting on another plugin's behalf.
    pub fn require_speaks_for(&self, plugin: &str, what: &str) -> Result<(), WebError> {
        if self.speaks_for(plugin) {
            return Ok(());
        }
        let PluginCaller::Plugin(name) = self else {
            unreachable!("App speaks for every plugin");
        };
        Err(WebError::Forbidden(format!(
            "plugin `{name}` may not {what} of plugin `{plugin}`"
        )))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for PluginCaller {
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let declared = parts
            .headers
            .get(PLUGIN_CALLER_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        match declared {
            Some(APP_CALLER) => Ok(PluginCaller::App),
            Some(name) => Ok(PluginCaller::Plugin(name.to_string())),
            // The case the whole seam exists for. Answering "allowed" here
            // would make omission the bypass — see the module comment.
            None => Err(WebError::Forbidden(format!(
                "this route needs a caller identity: send `{PLUGIN_CALLER_HEADER}: {APP_CALLER}`, \
                 or the name of the plugin you are drawing for"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_speaks_for_every_plugin_and_a_plugin_only_for_itself() {
        assert!(PluginCaller::App.speaks_for("kanban"));
        assert!(PluginCaller::Plugin("kanban".into()).speaks_for("kanban"));
        assert!(!PluginCaller::Plugin("kanban".into()).speaks_for("oci"));
    }

    #[test]
    fn a_refusal_names_both_plugins_so_the_message_is_actionable() {
        let refusal = PluginCaller::Plugin("kanban".into())
            .require_speaks_for("oci", "read the settings")
            .unwrap_err()
            .to_string();

        assert!(refusal.contains("kanban"), "{refusal}");
        assert!(refusal.contains("oci"), "{refusal}");
    }
}
