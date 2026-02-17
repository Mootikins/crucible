//! Command parsing for vim-style TUI commands.

mod set;

pub use set::{
    validate_set_for_cli, CliValue, ParseError, SetCommand, SetEffect, SetError, SetRpcAction,
};
