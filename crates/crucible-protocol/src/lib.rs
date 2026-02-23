// Thin re-export shim — types have moved to crucible-core::protocol
// This crate will be deleted in Task 12
pub use crucible_core::protocol::{
    remove_socket, socket_path, Request, RequestId, Response, RpcError, SessionEventMessage,
    INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR,
};
pub mod recording;
pub use recording::{RecordedEvent, RecordingFooter, RecordingHeader};
