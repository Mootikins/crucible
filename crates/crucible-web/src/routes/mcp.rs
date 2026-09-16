use crate::error::WebResultExt;
use crate::services::daemon::AppState;
use crate::WebError;
use axum::{extract::State, Json};
use crucible_daemon::McpStatus;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn mcp_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(mcp_status))
}

/// Whether the daemon is serving an MCP surface, and where.
///
/// The daemon's own answer, forwarded: the two arms and the keys each one
/// writes belong to the manager that holds the server, not to this route.
#[utoipa::path(
    get,
    path = "/api/mcp/status",
    responses(
        (status = 200, body = McpStatus),
        (status = 502, description = "The daemon could not report the MCP status"),
    )
)]
async fn mcp_status(State(state): State<AppState>) -> Result<Json<McpStatus>, WebError> {
    let result = state.daemon.mcp_status().await.daemon_err()?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;

    /// A stopped server says so and writes NOTHING else: `transport`, `port`
    /// and `kiln_path` as null would claim it has them and they are empty.
    #[tokio::test]
    async fn mcp_status_answers_the_declared_shape() {
        let (status, json) = request_json("GET", "/api/mcp/status", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        assert_eq!(json["running"], serde_json::json!(false), "{json}");
        assert_eq!(
            json.as_object().expect("an object").len(),
            1,
            "a stopped server reports only `running`: {json}"
        );
        let parsed: McpStatus =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert!(matches!(parsed, McpStatus::Stopped(_)));
    }

    /// The untagged union reads BOTH ways: each arm's wire form comes back as
    /// that arm, so the running payload can never be read as a stopped one.
    #[test]
    fn each_status_arm_round_trips_as_itself() {
        let running = McpStatus::Running(crucible_daemon::McpRunning {
            running: true,
            transport: "sse".to_string(),
            port: Some(3847),
            kiln_path: "/kilns/docs".to_string(),
            finished: false,
        });
        let wire = serde_json::to_value(&running).expect("the running arm serialises");
        assert_eq!(wire["running"], serde_json::json!(true));
        assert_eq!(wire["port"], serde_json::json!(3847));
        assert_eq!(
            serde_json::from_value::<McpStatus>(wire).expect("the running arm parses"),
            running
        );

        let stopped = McpStatus::Stopped(crucible_daemon::McpStopped { running: false });
        let wire = serde_json::to_value(&stopped).expect("the stopped arm serialises");
        assert_eq!(wire, serde_json::json!({ "running": false }));
        assert_eq!(
            serde_json::from_value::<McpStatus>(wire).expect("the stopped arm parses"),
            stopped
        );
    }

    /// A stdio server has no port, and says so with a written null rather than
    /// by leaving the key out: "no port" and "this answer does not mention
    /// ports" are different sentences.
    #[test]
    fn a_stdio_server_writes_a_null_port() {
        let wire = serde_json::to_value(McpStatus::Running(crucible_daemon::McpRunning {
            running: true,
            transport: "stdio".to_string(),
            port: None,
            kiln_path: "/kilns/docs".to_string(),
            finished: false,
        }))
        .expect("the running arm serialises");
        assert!(wire.get("port").is_some(), "{wire}");
        assert!(wire["port"].is_null(), "{wire}");
    }
}
