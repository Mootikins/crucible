pub mod lifecycle;
pub mod rpc;
pub mod session_events;

pub use lifecycle::{remove_socket, socket_path};
pub use rpc::{
    Request, RequestId, Response, RpcError, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS,
    INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR,
};
pub use session_events::{
    ContextLimitResolvedPayload, ContextLimitSource, EventDecodeError, Group, JobPayload,
    KilnNotesIndexedPayload, McpServersReadyPayload, NotificationPayload, PluginsDiscoveredPayload,
    ProvidersListedPayload, ReviewPayload, SessionEventPayload, SessionInitializedPayload,
    SettingsPayload, SetupPayload, SystemPayload, ToolResultBody, TurnPayload, WorkflowPayload,
    WorkspaceIndexedPayload, EVENT_NAMES,
};
