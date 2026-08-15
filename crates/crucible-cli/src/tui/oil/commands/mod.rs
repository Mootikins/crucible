//! Command parsing for vim-style TUI commands.

mod set;

pub(crate) use set::parse_bool;
pub use set::{
    classify_set_value, validate_set_for_cli, CliValue, ParseError, SetCommand, SetEffect,
    SetError, SetRpcAction,
};
