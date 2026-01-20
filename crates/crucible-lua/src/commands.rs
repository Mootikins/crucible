//! Lua command handler for slash commands
//!
//! Provides `LuaCommandHandler` that implements `CommandHandler` and executes
//! Lua functions when slash commands are invoked.
//!
//! ## Usage in Lua
//!
//! ```lua
//! --- Create a daily note
//! -- @command name="daily" hint="title"
//! -- @param title string? Optional title for the note
//! function daily(args, ctx)
//!     local title = args.title or os.date("%Y-%m-%d")
//!     ctx.insert_text("# " .. title)
//!     ctx.display_info("Created daily note: " .. title)
//! end
//! ```
//!
//! ## Command Discovery
//!
//! Use `discover_commands_from` to scan directories for Lua/Fennel files with
//! `@command` annotations:
//!
//! ```rust,ignore
//! use crucible_lua::commands::discover_commands_from;
//! use crucible_core::discovery::DiscoveryPaths;
//!
//! let paths = DiscoveryPaths::new("plugins", Some(kiln_path));
//! let commands = discover_commands_from(&paths.existing_paths()).await?;
//!
//! for cmd in commands {
//!     let handler = LuaCommandHandler::from_discovered(&cmd);
//!     // Register with SlashCommandRegistry...
//! }
//! ```

use crate::annotations::{AnnotationParser, DiscoveredCommand};
use crate::error::LuaError;
use async_trait::async_trait;
use crucible_core::traits::chat::{
    ArgumentSpec, ChatContext, ChatError, ChatResult, CommandDescriptor, CommandHandler,
    CommandKind,
};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Discover commands from multiple directories.
///
/// Scans each directory for `.lua` and `.fnl` files containing `@command` annotations.
pub async fn discover_commands_from(dirs: &[PathBuf]) -> Result<Vec<DiscoveredCommand>, LuaError> {
    let mut all_commands = Vec::new();
    let parser = AnnotationParser::new();

    for dir in dirs {
        match discover_commands_in_dir(&parser, dir).await {
            Ok(commands) => {
                if !commands.is_empty() {
                    info!(
                        "Discovered {} Lua commands from {}",
                        commands.len(),
                        dir.display()
                    );
                }
                all_commands.extend(commands);
            }
            Err(e) => {
                warn!("Failed to discover commands from {}: {}", dir.display(), e);
            }
        }
    }

    Ok(all_commands)
}

async fn discover_commands_in_dir(
    parser: &AnnotationParser,
    dir: &Path,
) -> Result<Vec<DiscoveredCommand>, LuaError> {
    if !dir.exists() {
        debug!("Command directory does not exist: {}", dir.display());
        return Ok(Vec::new());
    }

    let mut commands = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        let is_lua_or_fennel = path.extension().is_some_and(|e| e == "lua" || e == "fnl");

        if !is_lua_or_fennel {
            continue;
        }

        match discover_commands_in_file(parser, &path).await {
            Ok(file_commands) => {
                for cmd in file_commands {
                    debug!("Discovered command: /{} from {}", cmd.name, path.display());
                    commands.push(cmd);
                }
            }
            Err(e) => {
                warn!("Failed to parse commands in {}: {}", path.display(), e);
            }
        }
    }

    Ok(commands)
}

async fn discover_commands_in_file(
    parser: &AnnotationParser,
    path: &Path,
) -> Result<Vec<DiscoveredCommand>, LuaError> {
    let source = tokio::fs::read_to_string(path).await?;
    parser.parse_commands(&source, path)
}

#[derive(Debug, Clone)]
pub enum ContextAction {
    DisplayInfo(String),
    DisplayError(String),
}

#[derive(Debug, Default, Clone)]
struct ActionCollector {
    actions: Arc<Mutex<Vec<ContextAction>>>,
}

impl ActionCollector {
    fn new() -> Self {
        Self {
            actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn push(&self, action: ContextAction) {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(action);
    }

    fn take_actions(&self) -> Vec<ContextAction> {
        std::mem::take(
            &mut *self
                .actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }
}

struct LuaCommandCtx {
    collector: ActionCollector,
}

impl UserData for LuaCommandCtx {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("display_info", |_, this, msg: String| {
            this.collector.push(ContextAction::DisplayInfo(msg));
            Ok(())
        });

        methods.add_method("display_error", |_, this, msg: String| {
            this.collector.push(ContextAction::DisplayError(msg));
            Ok(())
        });
    }
}

pub struct LuaCommandHandler {
    source_path: PathBuf,
    handler_fn: String,
    is_fennel: bool,
}

impl LuaCommandHandler {
    pub fn new(source_path: PathBuf, handler_fn: String, is_fennel: bool) -> Self {
        Self {
            source_path,
            handler_fn,
            is_fennel,
        }
    }

    pub fn from_discovered(cmd: &DiscoveredCommand) -> Self {
        Self::new(
            PathBuf::from(&cmd.source_path),
            cmd.handler_fn.clone(),
            cmd.is_fennel,
        )
    }

    fn load_and_call(
        &self,
        args_str: &str,
    ) -> Result<(Option<String>, Vec<ContextAction>), LuaError> {
        let lua = Lua::new();

        let source = std::fs::read_to_string(&self.source_path)?;

        #[cfg(feature = "fennel")]
        let lua_source = if self.is_fennel {
            use crate::fennel::FennelCompiler;
            let fennel = FennelCompiler::new(&lua)?;
            fennel.compile_with_lua(&lua, &source)?
        } else {
            source
        };

        #[cfg(not(feature = "fennel"))]
        let lua_source = if self.is_fennel {
            return Err(LuaError::FennelCompile("Fennel support not enabled".into()));
        } else {
            source
        };

        lua.load(&lua_source).exec()?;

        let globals = lua.globals();
        let handler: Function = globals.get(self.handler_fn.as_str()).map_err(|_| {
            LuaError::InvalidTool(format!("Handler function '{}' not found", self.handler_fn))
        })?;

        let args_table = self.parse_args_to_table(&lua, args_str)?;

        let collector = ActionCollector::new();
        let ctx = LuaCommandCtx {
            collector: collector.clone(),
        };
        let ctx_userdata = lua.create_userdata(ctx)?;

        let result: Value = handler.call((args_table, ctx_userdata))?;

        let actions = collector.take_actions();

        let return_msg = match result {
            Value::Nil => None,
            Value::String(s) => Some(s.to_str()?.to_string()),
            Value::Table(t) => t.get::<String>("message").ok(),
            _ => None,
        };

        Ok((return_msg, actions))
    }

    fn parse_args_to_table(&self, lua: &Lua, args_str: &str) -> Result<Table, LuaError> {
        let table = lua.create_table()?;
        let args_str = args_str.trim();

        if args_str.is_empty() {
            return Ok(table);
        }

        table.set("_raw", args_str)?;

        let parts: Vec<&str> = args_str.split_whitespace().collect();
        if !parts.is_empty() {
            table.set("_positional", parts[0])?;
        }

        let args_array = lua.create_table()?;
        for (i, part) in parts.iter().enumerate() {
            args_array.set(i + 1, *part)?;
        }
        table.set("_args", args_array)?;

        for part in &parts {
            if let Some((key, value)) = part.split_once('=') {
                table.set(key, value)?;
            }
        }

        Ok(table)
    }
}

#[async_trait]
impl CommandHandler for LuaCommandHandler {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        match self.load_and_call(args) {
            Ok((return_msg, actions)) => {
                for action in actions {
                    match action {
                        ContextAction::DisplayInfo(msg) => ctx.display_info(&msg),
                        ContextAction::DisplayError(msg) => ctx.display_error(&msg),
                    }
                }
                if let Some(msg) = return_msg {
                    ctx.display_info(&msg);
                }
                Ok(())
            }
            Err(e) => {
                ctx.display_error(&format!("Lua command error: {}", e));
                Err(ChatError::CommandFailed(e.to_string()))
            }
        }
    }
}

pub fn command_to_descriptor(cmd: &DiscoveredCommand) -> CommandDescriptor {
    CommandDescriptor {
        name: cmd.name.clone(),
        description: cmd.description.clone(),
        input_hint: cmd.input_hint.clone(),
        secondary_options: Vec::new(),
        kind: CommandKind::Slash,
        module: Some("lua".to_string()),
        args: cmd
            .params
            .iter()
            .map(|p| {
                let mut spec = ArgumentSpec::new(&p.name).hint(&p.description);
                if !p.optional {
                    spec = spec.required();
                }
                spec
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_script(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_lua_command_handler_simple() {
        let script = write_temp_script(
            r#"
            function greet(args, ctx)
                return { message = "Hello, " .. (args._positional or "world") }
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "greet".to_string(), false);

        let (result, actions) = handler.load_and_call("Alice").unwrap();
        assert_eq!(result, Some("Hello, Alice".to_string()));
        assert!(actions.is_empty());
    }

    #[test]
    fn test_lua_command_handler_empty_args() {
        let script = write_temp_script(
            r#"
            function echo(args, ctx)
                return { message = args._raw or "empty" }
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "echo".to_string(), false);

        let (result, _) = handler.load_and_call("").unwrap();
        assert_eq!(result, Some("empty".to_string()));
    }

    #[test]
    fn test_lua_command_handler_key_value_args() {
        let script = write_temp_script(
            r#"
            function config(args, ctx)
                return { message = "name=" .. (args.name or "none") }
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "config".to_string(), false);

        let (result, _) = handler.load_and_call("name=test").unwrap();
        assert_eq!(result, Some("name=test".to_string()));
    }

    #[test]
    fn test_lua_command_handler_nil_return() {
        let script = write_temp_script(
            r#"
            function noop(args, ctx)
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "noop".to_string(), false);

        let (result, _) = handler.load_and_call("").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_lua_command_handler_string_return() {
        let script = write_temp_script(
            r#"
            function direct(args, ctx)
                return "Direct string"
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "direct".to_string(), false);

        let (result, _) = handler.load_and_call("").unwrap();
        assert_eq!(result, Some("Direct string".to_string()));
    }

    #[test]
    fn test_lua_command_handler_ctx_display_info() {
        let script = write_temp_script(
            r#"
            function notify(args, ctx)
                ctx:display_info("Info message")
                ctx:display_info("Second info")
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "notify".to_string(), false);

        let (result, actions) = handler.load_and_call("").unwrap();
        assert_eq!(result, None);
        assert_eq!(actions.len(), 2);
        assert!(matches!(&actions[0], ContextAction::DisplayInfo(s) if s == "Info message"));
        assert!(matches!(&actions[1], ContextAction::DisplayInfo(s) if s == "Second info"));
    }

    #[test]
    fn test_lua_command_handler_ctx_display_error() {
        let script = write_temp_script(
            r#"
            function fail(args, ctx)
                ctx:display_error("Something went wrong")
                return { message = "Done anyway" }
            end
            "#,
        );

        let handler =
            LuaCommandHandler::new(script.path().to_path_buf(), "fail".to_string(), false);

        let (result, actions) = handler.load_and_call("").unwrap();
        assert_eq!(result, Some("Done anyway".to_string()));
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], ContextAction::DisplayError(s) if s == "Something went wrong")
        );
    }

    #[test]
    fn test_command_to_descriptor() {
        use crate::annotations::DiscoveredParam;

        let cmd = DiscoveredCommand {
            name: "test".to_string(),
            description: "Test command".to_string(),
            params: vec![DiscoveredParam {
                name: "query".to_string(),
                param_type: "string".to_string(),
                description: "Search query".to_string(),
                optional: false,
            }],
            input_hint: Some("query".to_string()),
            source_path: "test.lua".to_string(),
            handler_fn: "test_handler".to_string(),
            is_fennel: false,
        };

        let desc = command_to_descriptor(&cmd);
        assert_eq!(desc.name, "test");
        assert_eq!(desc.description, "Test command");
        assert_eq!(desc.input_hint, Some("query".to_string()));
        assert_eq!(desc.kind, CommandKind::Slash);
        assert_eq!(desc.module, Some("lua".to_string()));
        assert_eq!(desc.args.len(), 1);
        assert_eq!(desc.args[0].name, "query");
        assert!(desc.args[0].required);
    }

    #[test]
    fn test_from_discovered() {
        let cmd = DiscoveredCommand {
            name: "test".to_string(),
            description: "Test".to_string(),
            params: vec![],
            input_hint: None,
            source_path: "/tmp/test.lua".to_string(),
            handler_fn: "test_fn".to_string(),
            is_fennel: false,
        };

        let handler = LuaCommandHandler::from_discovered(&cmd);
        assert_eq!(handler.handler_fn, "test_fn");
        assert_eq!(handler.source_path, PathBuf::from("/tmp/test.lua"));
        assert!(!handler.is_fennel);
    }

    #[tokio::test]
    async fn test_discover_commands_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        let script_content = r#"
--- Create a daily note
-- @command name="daily" hint="title"
-- @param title string? Optional title
function create_daily(args)
    return { message = "Created daily" }
end
"#;
        std::fs::write(dir.path().join("daily.lua"), script_content).unwrap();

        let commands = discover_commands_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "daily");
        assert_eq!(commands[0].input_hint, Some("title".to_string()));
        assert_eq!(commands[0].handler_fn, "create_daily");
    }

    #[tokio::test]
    async fn test_discover_commands_empty_directory() {
        let dir = tempfile::tempdir().unwrap();

        let commands = discover_commands_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_discover_commands_nonexistent_directory() {
        let commands = discover_commands_from(&[PathBuf::from("/nonexistent/path")])
            .await
            .unwrap();

        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_discover_multiple_commands_same_file() {
        let dir = tempfile::tempdir().unwrap();

        let script_content = r#"
--- First command
-- @command name="cmd1"
function cmd1_handler(args)
    return nil
end

--- Second command
-- @command name="cmd2"
function cmd2_handler(args)
    return nil
end
"#;
        std::fs::write(dir.path().join("multi.lua"), script_content).unwrap();

        let commands = discover_commands_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(commands.len(), 2);
        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"cmd1"));
        assert!(names.contains(&"cmd2"));
    }
}
