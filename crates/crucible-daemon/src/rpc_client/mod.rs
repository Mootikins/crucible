//! Client library for connecting to Crucible daemon (cru daemon serve)
//!
//! Connection patterns:
//! - `DaemonClient::connect()` - connect to running daemon
//! - `DaemonClient::connect_or_start()` - connect or spawn daemon if not running
//!
//! Daemon detection is socket-based:
//! - Socket exists and connectable -> daemon running
//! - Socket exists but not connectable -> stale socket, safe to replace
//! - Socket doesn't exist -> daemon not running

mod agent;
mod client;
mod error_ext;
pub mod lifecycle;
mod storage;

pub use agent::{strip_chat_error_prefix, DaemonAgentHandle};
pub use client::workflow::{WorkflowApproveGateRequest, WorkflowStartRequest};
pub use client::NoteListRow;
pub use client::{
    first_per_note, AgentsListCardsRequest, DaemonCapabilities, DaemonClient, EmbeddingCatalog,
    EmbeddingModelRow, EmbeddingModelsRequest, FsListDirRequest, FsMoveRequest, FsPathRequest,
    GrepSearchRequest, KilnOpenRequest, KilnRegisterRequest, ListAllModelsRequest,
    ListProvidersRequest, LlmRegisterProviderRequest, LuaDiscoverPluginsRequest,
    LuaDiscoverPluginsResponse, LuaGenerateStubsRequest, LuaGenerateStubsResponse,
    LuaInitSessionRequest, LuaInitSessionResponse, LuaPluginHealthRequest, LuaPluginHealthResponse,
    LuaRegisterCommandsRequest, LuaRunPluginTestsRequest, LuaRunPluginTestsResponse,
    LuaShutdownSessionRequest, LuaShutdownSessionResponse, McpStartRequest, NameRequest,
    NoteRenameRequest, NotificationDismissRequest, NotificationListRequest, PathRequest,
    PluginInstallRequest, PluginOptionCallRequest, PluginOptionsRequest, PluginPublicationsRequest,
    PluginRemoveRequest, PluginRunCommandRequest, PluginSpecRow, PluginTestFailure,
    PluginTestLoadFailure, ProcessFileRequest, ReviewCommentRequest, ReviewListHunksRequest,
    ReviewResolveCommentRequest, ReviewSetStateRequest, ReviewSetStatesRequest, ScmCloneRequest,
    SearchVectorsRequest, SessionAgentSpec, SessionConfigureAgentRequest, SessionCreateParams,
    SessionCreateRequest, SessionDismissNotificationRequest, SessionEvent,
    SessionExportToFileRequest, SessionForkRequest, SessionIdRequest, SessionInjectContextRequest,
    SessionInteractionRespondRequest, SessionRenderMarkdownRequest, SessionReplayRequest,
    SessionResumeFromStorageRequest, SessionSetTitleRequest, SessionSwitchModelRequest,
    SessionTestInteractionRequest, SkillsGetRequest, SkillsListRequest, SkillsSearchRequest,
    SurfaceRequest, VectorHit, VersionCheck,
};
pub use error_ext::ChatResultExt;
// `DaemonClient::fts_search` returns this type, so callers of the client
// must name it without a path into the storage module.
pub use crate::storage::sqlite::FtsResult;
#[cfg(test)]
pub(crate) use storage::parse_note_from_record;
pub use storage::{DaemonNoteStore, DaemonStorageClient};

pub use crucible_core::protocol::socket_path;
