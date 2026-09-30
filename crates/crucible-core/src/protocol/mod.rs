pub mod lifecycle;
pub mod requests;
pub mod rpc;
pub mod session_events;

pub use lifecycle::{remove_socket, socket_path};
#[cfg(feature = "openapi")]
pub use rpc::RpcMethodSchemas;
pub use rpc::{
    Request, RequestId, Response, RpcError, RpcMethod, SessionEventMessage, BUSY, INTERNAL_ERROR,
    INVALID_PARAMS, INVALID_REQUEST, METHODS, METHOD_NOT_FOUND, PARSE_ERROR,
};
pub use session_events::{
    ContextLimitResolvedPayload, ContextLimitSource, EventDecodeError, Group, JobPayload,
    KilnNotesIndexedPayload, McpServersReadyPayload, NotificationPayload, PluginsDiscoveredPayload,
    ProvidersListedPayload, ReviewPayload, SessionEventPayload, SessionInitializedPayload,
    SettingsPayload, SetupPayload, SystemPayload, ToolResultBody, TurnPayload, WorkflowPayload,
    WorkspaceIndexedPayload,
};
