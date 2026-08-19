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

pub use agent::DaemonAgentHandle;
pub use client::workflow::{
    WorkflowApproveGateRequest, WorkflowSessionRequest, WorkflowStartRequest,
};
pub use client::{
    DaemonCapabilities, DaemonClient, LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse,
    LuaGenerateStubsRequest, LuaGenerateStubsResponse, LuaInitSessionRequest,
    LuaInitSessionResponse, LuaPluginHealthRequest, LuaPluginHealthResponse,
    LuaRegisterCommandsRequest, LuaRegisterCommandsResponse, LuaRunPluginTestsRequest,
    LuaRunPluginTestsResponse, LuaShutdownSessionRequest, LuaShutdownSessionResponse,
    FsListDirRequest, FsMoveRequest, FsPathRequest, GrepSearchRequest, KilnOpenRequest,
    KilnSetClassificationRequest, ListAllModelsRequest, ListProvidersRequest, McpStartRequest,
    NameRequest, NoteRenameRequest, PathRequest,
    PluginInstallRequest, PluginOptionCallRequest, PluginOptionsRequest,
    PluginPublicationsRequest, PluginRemoveRequest, PluginRunCommandRequest, PluginTestFailure,
    PluginTestLoadFailure, ProcessFileRequest, ScmCloneRequest, SearchVectorsRequest,
    SessionAgentSpec, SessionCreateParams, SessionCreateRequest, SessionEvent,
    SessionReplayRequest, SkillsGetRequest, SkillsListRequest, SkillsSearchRequest, VersionCheck,
};
pub use error_ext::ChatResultExt;
pub use storage::{DaemonNoteStore, DaemonStorageClient};

pub use crucible_core::protocol::socket_path;
