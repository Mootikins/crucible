//! Integration tests for Lua/Fennel tool discovery and execution

#[path = "integration/cru_inspect.rs"]
mod cru_inspect;
#[path = "integration/cru_tbl.rs"]
mod cru_tbl;
#[path = "integration/fennel.rs"]
mod fennel;
#[path = "integration/health.rs"]
mod health;
#[path = "integration/mocks.rs"]
mod mocks;
#[path = "integration/plugin_template.rs"]
mod plugin_template;
#[path = "integration/reload.rs"]
mod reload;
#[path = "integration/shared.rs"]
mod shared;
#[path = "integration/shell.rs"]
mod shell;
#[path = "integration/tools.rs"]
mod tools;
