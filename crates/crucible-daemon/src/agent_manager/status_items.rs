//! The status list of a session, as the clients draw it.
//!
//! Two sources make the list. Plugins publish items into the
//! [`crucible_lua::StatusRegistry`]. The engine makes one item for each
//! plugin that starts turns, from the session's approval knob and the turn
//! that runs now (decision 10 of "Plugin Turns and Status Items"). The
//! engine item is not in the registry, so a plugin's `publish` cannot erase
//! it, and a plugin cannot show a permission state that the knob does not
//! hold.

use super::AgentManager;
use crate::protocol::SessionEventMessage;
use crucible_core::protocol::session_events::SystemPayload;
use crucible_core::session::PluginApproval;
use crucible_core::status_color::StatusColorGroup;
use crucible_core::types::{
    StatusDisplayItem, StatusItemKind, PLUGIN_APPROVAL_ACTION, PLUGIN_TURNS_ID_PREFIX,
};
use std::collections::{BTreeMap, BTreeSet};

/// The engine items of one session. One item for each plugin whose
/// approval is not `inherit` or whose turn runs now. Each item is pinned:
/// `ask`, `stop` and a running plugin turn never overflow (decision 11).
pub(crate) fn plugin_turn_items(
    approvals: &BTreeMap<String, PluginApproval>,
    running: Option<&str>,
) -> Vec<StatusDisplayItem> {
    let mut plugins: BTreeSet<&str> = approvals
        .iter()
        .filter(|(_, approval)| **approval != PluginApproval::Inherit)
        .map(|(plugin, _)| plugin.as_str())
        .collect();
    plugins.extend(running);
    plugins
        .into_iter()
        .map(|plugin| {
            let approval = approvals.get(plugin).copied().unwrap_or_default();
            // Decision 13: a running loop uses `info`, `ask` uses `warn` and
            // `stop` uses `danger`. The approval outranks the running state,
            // because it changes what the turn may do.
            let group = match approval {
                PluginApproval::Inherit => StatusColorGroup::Info,
                PluginApproval::Ask => StatusColorGroup::Warn,
                PluginApproval::Stop => StatusColorGroup::Danger,
            };
            let mut text = String::new();
            if running == Some(plugin) {
                text.push_str("↻ ");
            }
            text.push_str(plugin);
            if approval != PluginApproval::Inherit {
                text.push_str(" · ");
                text.push_str(approval.as_str());
            }
            StatusDisplayItem {
                id: format!("{PLUGIN_TURNS_ID_PREFIX}{plugin}"),
                text,
                priority: 0,
                color_group: group,
                action: Some(PLUGIN_APPROVAL_ACTION.to_owned()),
                pinned: true,
                plugin: plugin.to_owned(),
                kind: StatusItemKind::PluginTurns,
                progress: None,
            }
        })
        .collect()
}

impl AgentManager {
    /// Bind the registry that plugins publish status items into.
    /// Idempotent, like the other registries.
    pub fn set_status_registry(&self, registry: crucible_lua::StatusRegistry) {
        let _ = self.status.set(registry);
    }

    /// The plugin whose turn runs now in `session_id`, if a plugin turn runs.
    ///
    /// Reads the slot without making one: a status read for a session with
    /// no slot is a session with no running turn.
    fn running_plugin(&self, session_id: &str) -> Option<String> {
        let slot = self.slots.get(session_id)?;
        let gate = slot.turn_gate()?;
        gate.origin.plugin().map(str::to_owned)
    }

    /// The engine items of `session_id`. It reads the approvals from
    /// storage when the session is not in memory, so a client that attaches
    /// to a dormant session sees a stored `ask` or `stop`.
    pub(crate) async fn plugin_turn_status_items(
        &self,
        session_id: &str,
    ) -> Vec<StatusDisplayItem> {
        let approvals = match self.session_manager.read_session(session_id).await {
            Ok(Some(session)) => session.plugin_approvals,
            Ok(None) => BTreeMap::new(),
            Err(error) => {
                tracing::warn!(%session_id, %error, "could not read the plugin approvals for the status list");
                BTreeMap::new()
            }
        };
        plugin_turn_items(&approvals, self.running_plugin(session_id).as_deref())
    }

    /// The whole list of `session_id`: the engine items, then the items
    /// that plugins published, in their order.
    pub(crate) async fn status_items(&self, session_id: &str) -> Vec<StatusDisplayItem> {
        let mut items = self.plugin_turn_status_items(session_id).await;
        items.extend(self.published_status_items(session_id));
        items
    }

    /// [`Self::status_items`] for a caller that cannot wait: the change
    /// notifier of the registry runs inside a Lua call. It reads the
    /// approvals of the session in memory only. A session that a plugin
    /// changes a status for is in memory; a client that reads a dormant
    /// session uses the `session.status` read, which reads storage.
    pub(crate) fn resident_status_items(&self, session_id: &str) -> Vec<StatusDisplayItem> {
        let approvals = self
            .session_manager
            .get_session(session_id)
            .map(|session| session.plugin_approvals)
            .unwrap_or_default();
        let mut items = plugin_turn_items(&approvals, self.running_plugin(session_id).as_deref());
        items.extend(self.published_status_items(session_id));
        items
    }

    fn published_status_items(&self, session_id: &str) -> Vec<StatusDisplayItem> {
        self.status
            .get()
            .map(|registry| registry.display_items(session_id))
            .unwrap_or_default()
    }

    /// Send the whole list of `session_id` to its clients.
    pub(crate) async fn emit_status_items(&self, session_id: &str, event_tx: &crate::EventBus) {
        let status = self.status_items(session_id).await;
        emit_status_items_changed(event_tx, session_id, status);
    }
}

/// The notifier that the registry calls after a plugin changes a list. It
/// sends the whole list of the session, engine items included. The handle is
/// weak, because the manager holds the registry that holds the notifier.
pub(crate) fn change_notifier(
    agents: std::sync::Weak<AgentManager>,
    event_tx: crate::EventBus,
) -> crucible_lua::statusline_exprs::ChangeNotifier {
    std::sync::Arc::new(move |session_id: &str| {
        if let Some(agents) = agents.upgrade() {
            emit_status_items_changed(
                &event_tx,
                session_id,
                agents.resident_status_items(session_id),
            );
        }
    })
}

/// Send `status` as the new list of `session_id`.
pub(crate) fn emit_status_items_changed(
    event_tx: &crate::EventBus,
    session_id: &str,
    status: Vec<StatusDisplayItem>,
) {
    let event =
        SessionEventMessage::typed(session_id, SystemPayload::StatusItemsChanged { status });
    if !event_tx.emit(event) {
        tracing::debug!(%session_id, "status item change had no subscribers");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approvals(pairs: &[(&str, PluginApproval)]) -> BTreeMap<String, PluginApproval> {
        pairs.iter().map(|(p, a)| ((*p).to_owned(), *a)).collect()
    }

    #[test]
    fn inherit_and_no_turn_make_no_item() {
        let stored = approvals(&[("alpha", PluginApproval::Inherit)]);
        assert!(plugin_turn_items(&stored, None).is_empty());
    }

    #[test]
    fn each_state_has_its_text_and_group() {
        let stored = approvals(&[("ask", PluginApproval::Ask), ("stop", PluginApproval::Stop)]);
        let items = plugin_turn_items(&stored, Some("stop"));
        let seen: Vec<_> = items
            .iter()
            .map(|item| (item.text.as_str(), item.color_group.name(), item.pinned))
            .collect();
        assert_eq!(
            seen,
            [
                ("ask · ask", "warn", true),
                ("↻ stop · stop", "danger", true)
            ]
        );
        let running = plugin_turn_items(&BTreeMap::new(), Some("goal"));
        assert_eq!(running[0].text, "↻ goal");
        assert_eq!(running[0].color_group, StatusColorGroup::Info);
        assert!(running[0].pinned);
    }
}
