//! `GET /api/config` and `POST /api/config`: the daemon's app config, read
//! and written through the two config verbs.
//!
//! Both directions forward. The daemon owns the store, the layering and the
//! refusal rule; this route names the RPC and hands the answer on unchanged,
//! because a second copy of any of those rules here would be a second answer
//! to "what is configured".

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ConfigResponse {
    kiln_path: String,
    /// Non-loopback terminal/shell access is active (opt-in + API key) —
    /// the terminal panel connects from LAN clients only when this is true.
    remote_shell: bool,
    /// The daemon's effective config, whole and unrewritten.
    config: serde_json::Value,
    /// Where `init.lua` and `settings.json` live, so a lock can offer a jump
    /// to the file that holds a key. Absent when the daemon booted from a
    /// config value rather than a file.
    config_root: Option<String>,
    /// One row per recorded leaf: `{key, value, source, file?, line?}`, as
    /// `config.origin` gives them. The flat shape is what a settings control
    /// renders a lock from; the effective config's own `provenance` map is
    /// the same fact in the store's enum shape, and serving both would be two
    /// spellings of one answer.
    origins: serde_json::Value,
    /// The declared control tree the settings UI renders, as
    /// `config.controls` gives it: `{options, read_only}`. Served here rather
    /// than from a second endpoint because a control and the value it shows
    /// are one screen, and two fetches could disagree about which keys exist.
    controls: serde_json::Value,
}

/// The values one save carries, in the shape `config.save` takes.
#[derive(Deserialize)]
struct SaveRequest {
    values: serde_json::Map<String, serde_json::Value>,
}

pub fn config_routes() -> Router<AppState> {
    Router::new().route("/api/config", get(get_config).post(save_config))
}

/// `GET /api/config` — the effective config, its per-leaf provenance, and the
/// two fields this route has always served.
async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, WebError> {
    let effective = state.daemon.config_effective().await.daemon_err()?;
    let origins = state.daemon.config_origins().await.daemon_err()?;
    let controls = state.daemon.config_controls().await.daemon_err()?;
    Ok(Json(ConfigResponse {
        kiln_path: kiln_path_for_client(&effective, || {
            state.config.kiln_path_str().unwrap_or_default()
        }),
        remote_shell: state.remote_shell,
        config: effective.get("config").cloned().unwrap_or_default(),
        config_root: effective
            .get("config_root")
            .and_then(|root| root.as_str())
            .map(str::to_string),
        origins: origins.get("origins").cloned().unwrap_or_default(),
        controls,
    }))
}

/// `POST /api/config` — save values as the user's durable preference.
///
/// The body and the answer are `config.save`'s own: `{values}` in,
/// `{ok, refused, rejected}` out. A refusal rides in the answer rather than in
/// an HTTP error because refusal is per leaf — the siblings the user changed
/// in the same click did save — and because the caller needs the file and the
/// line the daemon named, which an error status cannot carry.
async fn save_config(
    State(state): State<AppState>,
    Json(request): Json<SaveRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let saved = state
        .daemon
        .config_save(request.values)
        .await
        .daemon_err()?;
    Ok(Json(saved))
}

/// The kiln path to report, given the daemon's `config.effective` answer.
///
/// `kiln_path` DEFAULTS to the current directory of whichever process
/// computes it, so a daemon that was never configured answers with its own
/// cwd — meaningless to a browser. `config.effective` says when its value is
/// that default, and this server then substitutes the one it computed itself,
/// exactly as the CLI does with the same flag.
fn kiln_path_for_client(effective: &serde_json::Value, own: impl FnOnce() -> String) -> String {
    if effective["kiln_path_is_default"].as_bool() == Some(true) {
        return own();
    }
    effective["config"]["kiln_path"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(own)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        build_mock_state_with_config, build_test_app, start_mock_daemon, MOCK_DAEMON_KILN_PATH,
        MOCK_LOCATION_REASON, MOCK_PINNED_KEY, MOCK_PIN_FILE,
    };
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use crucible_core::config::CliAppConfig;
    use serde_json::Value;
    use tower::ServiceExt;

    async fn get_config_json(config: CliAppConfig) -> Value {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state_with_config(client, config));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// POST a body and answer with the status and the parsed body.
    async fn post_config(values: Value) -> (StatusCode, Value, Option<Value>) {
        let (mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state_with_config(
            client,
            CliAppConfig::default(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "values": values }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap(),
            mock.received_params("config.save"),
        )
    }

    #[tokio::test]
    async fn config_serves_the_kiln_path_and_remote_shell_flag() {
        let json = get_config_json(CliAppConfig::default()).await;
        assert!(json.get("kiln_path").is_some());
        assert!(json.get("remote_shell").is_some());
    }

    /// The daemon's copy is the live one: it holds what `init.lua` set and
    /// what a `config.set` merged since. A web-side copy holds neither.
    #[tokio::test]
    async fn config_serves_the_daemon_effective_config_and_its_origins() {
        let json = get_config_json(CliAppConfig::default()).await;

        assert_eq!(
            json["config"]["chat"]["model"],
            serde_json::json!("daemon-model"),
            "the effective config must come from the daemon: {json}"
        );
        assert_eq!(json["kiln_path"], serde_json::json!(MOCK_DAEMON_KILN_PATH));
        assert_eq!(json["config_root"], serde_json::json!("/daemon/config"));

        let origin = json["origins"]
            .as_array()
            .unwrap_or_else(|| panic!("origins is a row per leaf: {json}"))
            .iter()
            .find(|row| row["key"] == serde_json::json!("chat.model"))
            .unwrap_or_else(|| panic!("the leaf the daemon recorded must be listed: {json}"));
        assert_eq!(origin["source"], serde_json::json!("lua"));
        assert_eq!(
            origin["file"],
            serde_json::json!(MOCK_PIN_FILE),
            "a lock renders a jump to its line, so the row carries the file"
        );
        assert_eq!(origin["line"], serde_json::json!(12));
    }

    /// The settings UI renders the daemon's declared controls, so they travel
    /// with the values. A read-only leaf travels with its reason: a key shown
    /// as unchangeable and unexplained is a dead end.
    #[tokio::test]
    async fn config_serves_the_declared_control_tree_and_its_read_only_reasons() {
        let json = get_config_json(CliAppConfig::default()).await;

        let chat = json["controls"]["options"]["args"]
            .as_array()
            .unwrap_or_else(|| panic!("the control tree travels whole: {json}"))
            .iter()
            .find(|group| group["path"] == serde_json::json!("chat"))
            .unwrap_or_else(|| panic!("the daemon's groups reach the browser: {json}"));
        assert_eq!(chat["args"][0]["path"], serde_json::json!("chat.model"));
        assert_eq!(chat["args"][0]["type"], serde_json::json!("input"));

        let read_only = &json["controls"]["read_only"][0];
        assert_eq!(read_only["path"], serde_json::json!("data_home"));
        assert_eq!(read_only["reason"], serde_json::json!(MOCK_LOCATION_REASON));
    }

    /// A daemon that was never told a kiln answers with its own working
    /// directory, which means nothing to a browser. This server substitutes
    /// the path it resolved itself.
    #[test]
    fn a_defaulted_daemon_kiln_path_gives_way_to_this_server_s_own() {
        let defaulted = serde_json::json!({
            "config": { "kiln_path": "/the/daemons/cwd" },
            "kiln_path_is_default": true,
        });
        assert_eq!(
            kiln_path_for_client(&defaulted, || "/this/servers/kiln".to_string()),
            "/this/servers/kiln"
        );

        let configured = serde_json::json!({
            "config": { "kiln_path": "/the/configured/kiln" },
            "kiln_path_is_default": false,
        });
        assert_eq!(
            kiln_path_for_client(&configured, || "/this/servers/kiln".to_string()),
            "/the/configured/kiln",
            "a configured kiln_path is the daemon's to state"
        );
    }

    /// What a plugin offers is the plugin's to state, not this route's to infer.
    ///
    /// This endpoint used to report isolation profiles by reaching into raw
    /// `[plugins.*]` TOML and matching on a `profiles` table — so `crucible-web`
    /// encoded the `oci` plugin's config schema, in the rendering layer, for a
    /// plugin whose whole design goal is that no Rust knows what a container is.
    /// It also missed the documented bare-`image` config, which has no
    /// `profiles` table, and would have ignored a second isolating plugin whose
    /// config was shaped differently. Plugins publish instead; see
    /// `GET /api/plugins/publications`.
    #[tokio::test]
    async fn config_reports_nothing_about_plugin_configuration() {
        let mut config = CliAppConfig::default();
        config.plugins.insert(
            "oci".to_string(),
            serde_json::json!({
                "image": "docker.io/library/alpine:latest",
                "profiles": { "rust": { "image": "docker.io/library/rust:1-bookworm" } },
            }),
        );
        let json = get_config_json(config).await;

        assert!(json.get("profiles").is_none(), "got: {json}");
        assert!(json.get("isolation_available").is_none(), "got: {json}");
        assert!(
            !json.to_string().contains("alpine"),
            "no plugin config may reach the browser through this route; got: {json}"
        );
    }

    /// The daemon decides which leaves a save may write, so the values reach
    /// it as the browser sent them.
    #[tokio::test]
    async fn a_save_forwards_the_values_untouched() {
        let values = serde_json::json!({ "chat": { "model": "claude-opus-4" } });
        let (status, body, forwarded) = post_config(values.clone()).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], serde_json::json!(true), "{body}");
        assert_eq!(
            forwarded.expect("the save reaches the daemon")["values"],
            values
        );
    }

    /// A refusal is the answer, not an error: it names the file and the line
    /// that hold the key, and the browser offers a jump to it.
    #[tokio::test]
    async fn a_refused_save_carries_the_file_and_line_that_pinned_the_key() {
        let (status, body, _) =
            post_config(serde_json::json!({ MOCK_PINNED_KEY: { "leaf": 1 } })).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "a per-leaf refusal is an answer, not a transport failure: {body}"
        );
        assert_eq!(body["ok"], serde_json::json!(false), "{body}");
        let refused = &body["refused"][0];
        assert_eq!(refused["source"], serde_json::json!("lua"), "{body}");
        assert_eq!(refused["file"], serde_json::json!(MOCK_PIN_FILE), "{body}");
        assert_eq!(refused["line"], serde_json::json!(12), "{body}");
    }
}
