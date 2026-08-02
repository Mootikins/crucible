use crate::services::daemon::AppState;
use crate::WebError;
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct ConfigResponse {
    kiln_path: String,
    /// Non-loopback terminal/shell access is active (opt-in + API key) —
    /// the terminal panel connects from LAN clients only when this is true.
    remote_shell: bool,
}

pub fn config_routes() -> Router<AppState> {
    Router::new().route("/api/config", get(get_config))
}

async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, WebError> {
    let kiln_path = state.config.kiln_path_str().unwrap_or_default();
    Ok(Json(ConfigResponse {
        kiln_path,
        remote_shell: state.remote_shell,
    }))
}

#[cfg(test)]
mod tests {
    use crate::test_support::{build_mock_state_with_config, build_test_app, start_mock_daemon};
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

    #[tokio::test]
    async fn config_serves_the_kiln_path_and_remote_shell_flag() {
        let json = get_config_json(CliAppConfig::default()).await;
        assert!(json.get("kiln_path").is_some());
        assert!(json.get("remote_shell").is_some());
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
}
