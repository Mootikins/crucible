//! Lua command handler for slash commands
//!
//! Provides `LuaCommandHandler` that implements `CommandHandler` and executes
//! Lua functions when slash commands are invoked.

use crate::annotations::DiscoveredCommand;
use crate::error::LuaError;
use async_trait::async_trait;
use crucible_core::traits::chat::{
    ArgumentSpec, ChatContext, ChatError, ChatResult, CommandDescriptor, CommandHandler,
    CommandKind,
};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

}
