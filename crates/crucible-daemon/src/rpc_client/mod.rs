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
pub use client::{
    DaemonCapabilities, DaemonClient, FsListDirRequest, FsMoveRequest, FsPathRequest,
    GrepSearchRequest, KilnOpenRequest, KilnSetClassificationRequest, ListAllModelsRequest,
    ListProvidersRequest, LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse,
    LuaGenerateStubsRequest, LuaGenerateStubsResponse, LuaInitSessionRequest,
    LuaInitSessionResponse, LuaPluginHealthRequest, LuaPluginHealthResponse,
    LuaRegisterCommandsRequest, LuaRunPluginTestsRequest, LuaRunPluginTestsResponse,
    LuaShutdownSessionRequest, LuaShutdownSessionResponse, McpStartRequest, NameRequest,
    NoteRenameRequest, PathRequest, PluginInstallRequest, PluginOptionCallRequest,
    PluginOptionsRequest, PluginPublicationsRequest, PluginRemoveRequest, PluginRunCommandRequest,
    PluginTestFailure, PluginTestLoadFailure, ProcessFileRequest, ReviewCommentRequest,
    ReviewResolveCommentRequest, ReviewSetStateRequest, ScmCloneRequest, SearchVectorsRequest,
    SessionAgentSpec, SessionConfigureAgentRequest, SessionCreateParams, SessionCreateRequest,
    SessionDismissNotificationRequest, SessionEvent, SessionExportToFileRequest,
    SessionForkRequest, SessionIdRequest, SessionInjectContextRequest,
    SessionInteractionRespondRequest, SessionRenderMarkdownRequest, SessionReplayRequest,
    SessionResumeFromStorageRequest, SessionSetTitleRequest, SessionSwitchModelRequest,
    SessionTestInteractionRequest, SkillsGetRequest, SkillsListRequest, SkillsSearchRequest,
    VersionCheck,
};
pub use error_ext::ChatResultExt;
pub use storage::{DaemonNoteStore, DaemonStorageClient};

pub use crucible_core::protocol::socket_path;
