//! Plugin surfaces: the panels a plugin declares for every client to draw.
//!
//! `GET /api/surfaces` is gone (Simplification Plan step 19): it only
//! forwarded `surface.list`, which the browser now calls through
//! `POST /api/rpc/{method}` (`routes/rpc.rs`). A surface changing travels on
//! the `system` topic of `GET /api/events`
//! (`routes/events.rs::system_event_frame`), which says only that a surface
//! moved. Nothing here knows what a plugin's rows mean: a row is `{id, text,
//! detail, mark}` and the component draws it from that, so a plugin shipped
//! tomorrow gets a panel with no change on this side.
//!
//! `SurfaceChangedEvent` stays: it is the push frame's own shape, not a
//! forwarder for the deleted route. Its own event shape rather than a variant
//! on [`FsEvent`](crate::fs_events::FsEvent), which is a *filesystem* change
//! by its own definition. A focused type per channel is what keeps either one
//! honest.
use crucible_core::protocol::SystemPayload;
use crucible_daemon::SessionEvent;
use serde::Serialize;

/// A surface changed, delivered to the browser.
///
/// Carries the identity and the new version, never the rows — the same contract
/// the daemon event has, and for the same reason: a surface is unbounded where an
/// event is not, and two clients want it at different moments. The browser
/// refetches through `GET /api/surfaces`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SurfaceChangedEvent {
    pub plugin: String,
    pub name: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The surface is gone, so the browser drops it instead of refetching.
    ///
    /// Passed through rather than re-derived. The daemon knew this when it
    /// dropped the entry, and a browser that had to ask `GET /api/surfaces`
    /// to find out would pay a round trip for a fact the event already held.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
}

impl SurfaceChangedEvent {
    /// The SSE `event:` name the browser listens for.
    ///
    /// Read from the daemon's own payload, never written again here. See
    /// `PublicationChangedEvent::EVENT_NAME` for what a second literal cost.
    pub const EVENT_NAME: &'static str = SystemPayload::SURFACE_CHANGED;

    /// Project a daemon event into this shape, or `None` for anything else.
    ///
    /// `name` is required: an event that cannot say which surface moved would
    /// make every client refetch every surface, which is worse than missing it.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        if ev.event != Self::EVENT_NAME {
            return None;
        }
        let d = &ev.data;
        Some(Self {
            plugin: d["plugin"].as_str().unwrap_or_default().to_string(),
            name: d["name"].as_str()?.to_string(),
            version: d["version"].as_u64().unwrap_or(0),
            session: d["session"].as_str().map(str::to_string),
            // Absent means present: the daemon omits the field for an ordinary
            // change, so only an explicit `true` withdraws a panel. Defaulting
            // the other way would erase every panel on an event this build did
            // not recognise.
            withdrawn: d["withdrawn"].as_bool().unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // The push frame
    // =====================================================================

    fn event(data: serde_json::Value) -> SessionEvent {
        SessionEvent::new("system", SurfaceChangedEvent::EVENT_NAME, data)
    }

    /// The browser reads `withdrawn` off the frame to drop a panel without a
    /// refetch (`web/src/components/SurfacesPanel.tsx`). The contract crosses a
    /// language boundary, so it gets a test on this side of it.
    #[test]
    fn a_withdrawal_reaches_the_browser_frame() {
        let projected = SurfaceChangedEvent::from_daemon_event(&event(serde_json::json!({
            "plugin": "p", "name": "sessions", "version": 3, "withdrawn": true,
        })))
        .expect("projects");

        assert!(projected.withdrawn);
        assert_eq!(
            serde_json::to_value(&projected).unwrap(),
            serde_json::json!({
                "plugin": "p", "name": "sessions", "version": 3, "withdrawn": true,
            }),
            "the browser reads this name for the fact"
        );
    }

    /// **Absent means present.** The daemon omits the field for an ordinary
    /// change, so the frame must stay byte-identical to what it was before the
    /// field existed — and must never tell the browser to erase the panel.
    #[test]
    fn an_ordinary_change_carries_no_withdrawal() {
        for data in [
            serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 }),
            serde_json::json!({
                "plugin": "p", "name": "sessions", "version": 2, "withdrawn": false,
            }),
        ] {
            let projected =
                SurfaceChangedEvent::from_daemon_event(&event(data.clone())).expect("projects");
            assert!(!projected.withdrawn, "{data}");
            assert_eq!(
                serde_json::to_value(&projected).unwrap(),
                serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 }),
                "an ordinary change serialises as it always did: {data}"
            );
        }
    }
}
