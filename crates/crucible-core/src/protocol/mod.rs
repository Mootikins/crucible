pub mod lifecycle;
pub mod rpc;
pub mod session_events;

pub use lifecycle::{remove_socket, socket_path};
pub use rpc::{
    Request, RequestId, Response, RpcError, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS,
    INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR,
};
pub use session_events::{
    ContextLimitResolvedPayload, ContextLimitSource, KilnNotesIndexedPayload,
    McpServersReadyPayload, PluginsDiscoveredPayload, ProvidersListedPayload,
    SessionInitializedPayload, WorkspaceIndexedPayload,
};
