//! Which config failure means "this file does not parse".
//!
//! `init.lua` is the only config language a human writes. A file that does
//! not parse states no intent at all, so the daemon refuses to start on it
//! and names the line. A file that parses and then raises states an intent
//! that ran part way, which the boot rolls back whole and warns about.
//!
//! The two must be told apart AT THE LOAD, not at the top. `cru.include` and
//! `require` both load a file inside a Rust callback, and Luau reports a
//! callback's failure to its caller as a runtime error. Without a mark that
//! survives the callback, the identical mistake would refuse the boot in
//! `init.lua` and pass silently one `include` down.

/// A file the config root owns does not parse.
///
/// The message is Luau's own, which already names the file and the line.
#[derive(Debug, Clone)]
pub struct ConfigSyntaxError {
    /// Luau's message, in the form `<file>:<line>: <reason>`.
    pub message: String,
}

impl std::fmt::Display for ConfigSyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigSyntaxError {}

/// Mark a load failure that is a syntax error; pass every other failure
/// through unchanged.
///
/// Call it where a config file is loaded from Rust. The mark is an external
/// error, which mlua carries under the `CallbackError` it wraps the callback
/// in, so [`config_syntax_error`] still finds it at the top.
pub fn mark_config_syntax(error: mlua::Error) -> mlua::Error {
    match error {
        mlua::Error::SyntaxError { message, .. } => {
            mlua::Error::external(ConfigSyntaxError { message })
        }
        other => other,
    }
}

/// The syntax error inside a failed config evaluation, when there is one.
///
/// Two shapes, and only two. The chunk the caller loaded did not parse, and
/// Luau reports that directly. Or a file that chunk loaded THROUGH THE HOST
/// did not parse and carried [`mark_config_syntax`] out with it.
///
/// A syntax error in a PLUGIN file is neither: a plugin is not the user's
/// config, so it stays fail-open and this returns `None` for it.
pub fn config_syntax_error(error: &mlua::Error) -> Option<ConfigSyntaxError> {
    match error {
        mlua::Error::SyntaxError { message, .. } => Some(ConfigSyntaxError {
            message: message.clone(),
        }),
        other => other.downcast_ref::<ConfigSyntaxError>().cloned(),
    }
}
