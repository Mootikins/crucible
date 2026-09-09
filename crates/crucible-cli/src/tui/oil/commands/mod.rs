//! Command parsing for vim-style TUI commands.

mod set;

pub(crate) use set::parse_bool;
pub use set::{
    classify_key_without_value, classify_set_value, key_home, validate_set_for_cli, CliValue,
    DropKind, KeyHome, ParseError, SetCommand, SetEffect, SetError, SetRpcAction,
};
