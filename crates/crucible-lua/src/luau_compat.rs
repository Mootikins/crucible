//! The standard-library pieces Luau does not ship, and Crucible's plugins do use.
//!
//! Luau removes `io` entirely and trims `os` to the clock calls, because its
//! home is a game client where a script may not touch the disk. Crucible's
//! plugins are host extensions, not untrusted content: `todo-list` reads and
//! writes its tasks file, `discord` reads its token out of the environment,
//! and every plugin's test suite mints a temporary directory. Under PUC Lua
//! they used the language's own `io` and `os`; the runtime swap must not
//! silently take a capability away.
//!
//! So the host provides them, in Rust, with the same signatures and the same
//! failure convention: `nil, message` rather than a raise, exactly as Lua's
//! `io.open` answers. What is NOT provided is as deliberate: no `io.popen`,
//! no `os.execute`, no `loadlib`. A plugin that wants to run a command asks
//! `cru.shell`, which is policy-gated; a plugin that reaches for a process
//! through `io` finds nothing.

use mlua::{Lua, MultiValue, UserData, UserDataMethods, Value};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// An open file, or a closed one. `Mutex` rather than `RefCell` because the
/// `send` feature puts the VM behind a lock and every userdata with it.
struct LuaFile {
    handle: Mutex<Option<OpenFile>>,
    path: PathBuf,
}

struct OpenFile {
    reader: BufReader<File>,
    writable: bool,
}

impl LuaFile {
    fn with_open<T>(&self, f: impl FnOnce(&mut OpenFile) -> mlua::Result<T>) -> mlua::Result<T> {
        let mut slot = self
            .handle
            .lock()
            .map_err(|_| mlua::Error::runtime("file handle lock poisoned"))?;
        match slot.as_mut() {
            Some(open) => f(open),
            None => Err(mlua::Error::runtime(format!(
                "attempt to use a closed file ({})",
                self.path.display()
            ))),
        }
    }
}

/// One `read` format. Lua spells them with or without the leading `*`.
enum ReadFormat {
    All,
    Line { keep_newline: bool },
    Number,
    Bytes(usize),
}

impl ReadFormat {
    fn parse(value: &Value) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(ReadFormat::Line {
                keep_newline: false,
            }),
            Value::Integer(n) => Ok(ReadFormat::Bytes((*n).max(0) as usize)),
            Value::Number(n) => Ok(ReadFormat::Bytes(n.max(0.0) as usize)),
            Value::String(s) => {
                let spec = s.to_string_lossy();
                let spec = spec.strip_prefix('*').unwrap_or(&spec);
                match spec.chars().next() {
                    Some('a') => Ok(ReadFormat::All),
                    Some('l') => Ok(ReadFormat::Line {
                        keep_newline: false,
                    }),
                    Some('L') => Ok(ReadFormat::Line { keep_newline: true }),
                    Some('n') => Ok(ReadFormat::Number),
                    _ => Err(mlua::Error::runtime(format!(
                        "bad read format '{spec}' (expected 'a', 'l', 'L', 'n' or a count)"
                    ))),
                }
            }
            other => Err(mlua::Error::runtime(format!(
                "bad read format (expected a string or a count, got {})",
                other.type_name()
            ))),
        }
    }

    fn read(&self, lua: &Lua, open: &mut OpenFile) -> mlua::Result<Value> {
        match self {
            ReadFormat::All => {
                let mut buffer = String::new();
                open.reader
                    .read_to_string(&mut buffer)
                    .map_err(mlua::Error::external)?;
                Ok(Value::String(lua.create_string(&buffer)?))
            }
            ReadFormat::Line { keep_newline } => {
                let mut buffer = String::new();
                let read = open
                    .reader
                    .read_line(&mut buffer)
                    .map_err(mlua::Error::external)?;
                if read == 0 {
                    return Ok(Value::Nil);
                }
                if !keep_newline {
                    while buffer.ends_with('\n') || buffer.ends_with('\r') {
                        buffer.pop();
                    }
                }
                Ok(Value::String(lua.create_string(&buffer)?))
            }
            ReadFormat::Number => {
                let mut buffer = String::new();
                let read = open
                    .reader
                    .read_line(&mut buffer)
                    .map_err(mlua::Error::external)?;
                if read == 0 {
                    return Ok(Value::Nil);
                }
                match buffer.trim().parse::<f64>() {
                    Ok(number) => Ok(Value::Number(number)),
                    Err(_) => Ok(Value::Nil),
                }
            }
            ReadFormat::Bytes(count) => {
                let mut buffer = vec![0u8; *count];
                let mut filled = 0;
                while filled < *count {
                    let read = open
                        .reader
                        .read(&mut buffer[filled..])
                        .map_err(mlua::Error::external)?;
                    if read == 0 {
                        break;
                    }
                    filled += read;
                }
                if filled == 0 && *count > 0 {
                    return Ok(Value::Nil);
                }
                buffer.truncate(filled);
                Ok(Value::String(lua.create_string(&buffer)?))
            }
        }
    }
}

impl UserData for LuaFile {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, this, formats: MultiValue| {
            let requested: Vec<Value> = if formats.is_empty() {
                vec![Value::Nil]
            } else {
                formats.into_iter().collect()
            };
            let mut results = Vec::with_capacity(requested.len());
            for format in requested {
                let format = ReadFormat::parse(&format)?;
                results.push(this.with_open(|open| format.read(lua, open))?);
            }
            Ok(MultiValue::from_iter(results))
        });

        methods.add_method("lines", |lua, _this, format: Value| {
            let format = ReadFormat::parse(&format)?;
            let iterator = lua.create_function_mut(move |lua, this: mlua::AnyUserData| {
                let file = this.borrow::<LuaFile>()?;
                file.with_open(|open| format.read(lua, open))
            })?;
            Ok((iterator, ()))
        });

        // `write` answers with the file itself, as Lua's does: shipped
        // plugins read it as `local wrote, err = handle:write(...)` and treat
        // a nil first return as the failure.
        methods.add_function(
            "write",
            |_, (this, values): (mlua::AnyUserData, MultiValue)| {
                {
                    let file = this.borrow::<LuaFile>()?;
                    file.with_open(|open| {
                        if !open.writable {
                            return Err(mlua::Error::runtime("the file is not open for writing"));
                        }
                        for value in &values {
                            let text = match value {
                                Value::String(s) => s.to_string_lossy().to_string(),
                                Value::Integer(n) => n.to_string(),
                                Value::Number(n) => n.to_string(),
                                other => {
                                    return Err(mlua::Error::runtime(format!(
                                        "cannot write a {} to a file",
                                        other.type_name()
                                    )))
                                }
                            };
                            open.reader
                                .get_mut()
                                .write_all(text.as_bytes())
                                .map_err(mlua::Error::external)?;
                        }
                        Ok(())
                    })?;
                }
                Ok(this)
            },
        );

        methods.add_method(
            "seek",
            |_, this, (whence, offset): (Option<String>, Option<i64>)| {
                this.with_open(|open| {
                    let offset = offset.unwrap_or(0);
                    let position = match whence.as_deref().unwrap_or("cur") {
                        "set" => SeekFrom::Start(offset.max(0) as u64),
                        "cur" => SeekFrom::Current(offset),
                        "end" => SeekFrom::End(offset),
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "bad seek base '{other}' (expected 'set', 'cur' or 'end')"
                            )))
                        }
                    };
                    open.reader.seek(position).map_err(mlua::Error::external)
                })
            },
        );

        methods.add_function("flush", |_, this: mlua::AnyUserData| {
            {
                let file = this.borrow::<LuaFile>()?;
                file.with_open(|open| {
                    open.reader.get_mut().flush().map_err(mlua::Error::external)
                })?;
            }
            Ok(this)
        });

        methods.add_method("close", |_, this, ()| {
            let mut slot = this
                .handle
                .lock()
                .map_err(|_| mlua::Error::runtime("file handle lock poisoned"))?;
            if let Some(mut open) = slot.take() {
                let _ = open.reader.get_mut().flush();
            }
            Ok(true)
        });
    }
}

/// Open one file the way `io.open` does, returning `nil, message` on failure.
fn open_file(lua: &Lua, path: String, mode: Option<String>) -> mlua::Result<MultiValue> {
    let mode = mode.unwrap_or_else(|| "r".to_string());
    let cleaned: String = mode.chars().filter(|c| *c != 'b').collect();
    let mut options = OpenOptions::new();
    match cleaned.as_str() {
        "r" => options.read(true),
        "w" => options.write(true).create(true).truncate(true),
        "a" => options.append(true).create(true),
        "r+" => options.read(true).write(true),
        "w+" => options.read(true).write(true).create(true).truncate(true),
        "a+" => options.read(true).append(true).create(true),
        other => {
            return Err(mlua::Error::runtime(format!(
                "bad file mode '{other}' (expected r, w, a, r+, w+ or a+)"
            )))
        }
    };

    match options.open(&path) {
        Ok(file) => {
            let writable = cleaned != "r";
            let handle = lua.create_userdata(LuaFile {
                handle: Mutex::new(Some(OpenFile {
                    reader: BufReader::new(file),
                    writable,
                })),
                path: PathBuf::from(&path),
            })?;
            Ok(MultiValue::from_iter([Value::UserData(handle)]))
        }
        Err(e) => Ok(MultiValue::from_iter([
            Value::Nil,
            Value::String(lua.create_string(format!("{path}: {e}"))?),
        ])),
    }
}

/// Register the `io` table and the `os` functions Luau omits.
///
/// Idempotent, and never replaces what the VM already has: a test that stubs
/// `os.getenv` keeps its stub.
pub fn register_stdlib_compat(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    if matches!(globals.get::<Value>("io")?, Value::Nil) {
        let io = lua.create_table()?;
        io.set(
            "open",
            lua.create_function(|lua, (path, mode): (String, Option<String>)| {
                open_file(lua, path, mode)
            })?,
        )?;
        io.set(
            "lines",
            lua.create_function(|lua, (path, format): (String, Value)| {
                let opened = open_file(lua, path.clone(), Some("r".to_string()))?;
                let handle = match opened.into_iter().next() {
                    Some(Value::UserData(handle)) => handle,
                    _ => {
                        return Err(mlua::Error::runtime(format!(
                            "{path}: cannot open file for reading"
                        )))
                    }
                };
                let format = ReadFormat::parse(&format)?;
                let iterator = lua.create_function_mut(move |lua, this: mlua::AnyUserData| {
                    let file = this.borrow::<LuaFile>()?;
                    file.with_open(|open| format.read(lua, open))
                })?;
                Ok((iterator, handle))
            })?,
        )?;
        io.set(
            "close",
            lua.create_function(|_, handle: mlua::AnyUserData| {
                let file = handle.borrow::<LuaFile>()?;
                let mut slot = file
                    .handle
                    .lock()
                    .map_err(|_| mlua::Error::runtime("file handle lock poisoned"))?;
                if let Some(mut open) = slot.take() {
                    let _ = open.reader.get_mut().flush();
                }
                Ok(true)
            })?,
        )?;
        io.set(
            "type",
            lua.create_function(|lua, value: Value| {
                let Value::UserData(handle) = value else {
                    return Ok(Value::Nil);
                };
                let Ok(file) = handle.borrow::<LuaFile>() else {
                    return Ok(Value::Nil);
                };
                let open = file
                    .handle
                    .lock()
                    .map(|slot| slot.is_some())
                    .unwrap_or(false);
                let kind = if open { "file" } else { "closed file" };
                Ok(Value::String(lua.create_string(kind)?))
            })?,
        )?;
        globals.set("io", io)?;
    }

    let os: mlua::Table = match globals.get::<Value>("os")? {
        Value::Table(os) => os,
        _ => {
            let os = lua.create_table()?;
            globals.set("os", os.clone())?;
            os
        }
    };

    if matches!(os.get::<Value>("getenv")?, Value::Nil) {
        os.set(
            "getenv",
            lua.create_function(|lua, name: String| match std::env::var(&name) {
                Ok(value) => Ok(Value::String(lua.create_string(&value)?)),
                Err(_) => Ok(Value::Nil),
            })?,
        )?;
    }

    if matches!(os.get::<Value>("tmpname")?, Value::Nil) {
        os.set(
            "tmpname",
            lua.create_function(|lua, ()| {
                // A name, not a file: Lua's `os.tmpname` creates nothing, and
                // callers immediately `os.remove` it or make a directory of
                // it. The random suffix is what keeps two VMs apart.
                let unique = format!(
                    "crucible-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or_default()
                );
                let path = std::env::temp_dir().join(unique);
                Ok(Value::String(
                    lua.create_string(path.to_string_lossy().as_ref())?,
                ))
            })?,
        )?;
    }

    if matches!(os.get::<Value>("remove")?, Value::Nil) {
        os.set(
            "remove",
            lua.create_function(|lua, path: String| match std::fs::remove_file(&path) {
                Ok(()) => Ok(MultiValue::from_iter([Value::Boolean(true)])),
                Err(e) => Ok(MultiValue::from_iter([
                    Value::Nil,
                    Value::String(lua.create_string(format!("{path}: {e}"))?),
                ])),
            })?,
        )?;
    }

    if matches!(os.get::<Value>("rename")?, Value::Nil) {
        os.set(
            "rename",
            lua.create_function(|lua, (from, to): (String, String)| {
                match std::fs::rename(&from, &to) {
                    Ok(()) => Ok(MultiValue::from_iter([Value::Boolean(true)])),
                    Err(e) => Ok(MultiValue::from_iter([
                        Value::Nil,
                        Value::String(lua.create_string(format!("{from} -> {to}: {e}"))?),
                    ])),
                }
            })?,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> Lua {
        let lua = Lua::new();
        register_stdlib_compat(&lua).expect("compat installs");
        lua
    }

    #[test]
    fn a_file_round_trips_through_io_open() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.md").to_string_lossy().to_string();
        let content: String = lua
            .load(format!(
                r##"
                local handle = assert(io.open({path:?}, "w"))
                handle:write("# Tasks\n", "- [ ] ship it\n")
                handle:close()
                local reader = assert(io.open({path:?}, "r"))
                local all = reader:read("a")
                reader:close()
                return all
                "##
            ))
            .eval()
            .expect("round trip");
        assert_eq!(content, "# Tasks\n- [ ] ship it\n");
    }

    /// `write` answers with the file, which is how a plugin tells a failed
    /// write from a good one: `local wrote, err = handle:write(...)`.
    #[test]
    fn write_answers_with_the_file() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt").to_string_lossy().to_string();
        let same: bool = lua
            .load(format!(
                r#"
                local handle = assert(io.open({path:?}, "w"))
                local wrote = handle:write("x")
                handle:close()
                return wrote == handle
                "#
            ))
            .eval()
            .unwrap();
        assert!(same, "write returns the file handle");
    }

    /// The failure convention is Lua's: `nil` plus a message, never a raise.
    /// Every shipped plugin writes `local handle = io.open(p); if not handle`.
    #[test]
    fn a_missing_file_returns_nil_and_a_message() {
        let lua = vm();
        let (handle, message): (Value, String) = lua
            .load(r#"return io.open("/nonexistent/crucible/file", "r")"#)
            .eval()
            .expect("io.open must not raise");
        assert!(matches!(handle, Value::Nil));
        assert!(!message.is_empty(), "the message names the failure");
    }

    #[test]
    fn a_line_read_strips_the_newline_and_ends_at_eof() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        let path = path.to_string_lossy().to_string();
        let joined: String = lua
            .load(format!(
                r#"
                local handle = assert(io.open({path:?}, "r"))
                local first = handle:read("l")
                local second = handle:read("*l")
                local third = handle:read("l")
                handle:close()
                return first .. "," .. second .. "," .. tostring(third)
                "#
            ))
            .eval()
            .unwrap();
        assert_eq!(joined, "one,two,nil");
    }

    #[test]
    fn an_appended_file_keeps_what_was_there() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, "first\n").unwrap();
        let path = path.to_string_lossy().to_string();
        let content: String = lua
            .load(format!(
                r#"
                local handle = assert(io.open({path:?}, "a"))
                handle:write("second\n")
                handle:close()
                local reader = assert(io.open({path:?}, "r"))
                local all = reader:read("a")
                reader:close()
                return all
                "#
            ))
            .eval()
            .unwrap();
        assert_eq!(content, "first\nsecond\n");
    }

    #[test]
    fn a_closed_file_refuses_further_reads() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("closed.txt");
        std::fs::write(&path, "x").unwrap();
        let path = path.to_string_lossy().to_string();
        let err = lua
            .load(format!(
                r#"
                local handle = assert(io.open({path:?}, "r"))
                handle:close()
                return handle:read("a")
                "#
            ))
            .eval::<Value>()
            .expect_err("a closed file must refuse");
        assert!(err.to_string().contains("closed file"), "got: {err}");
    }

    /// The three `os` calls every shipped test suite opens with.
    #[test]
    fn os_tmpname_remove_and_getenv_answer() {
        let lua = vm();
        let name: String = lua.load("return os.tmpname()").eval().unwrap();
        assert!(!name.is_empty());
        assert!(
            !std::path::Path::new(&name).exists(),
            "os.tmpname returns a name, and creates nothing"
        );

        std::env::set_var("CRUCIBLE_COMPAT_PROBE", "present");
        let value: String = lua
            .load(r#"return os.getenv("CRUCIBLE_COMPAT_PROBE")"#)
            .eval()
            .unwrap();
        assert_eq!(value, "present");
        let absent: Value = lua
            .load(r#"return os.getenv("CRUCIBLE_COMPAT_ABSENT")"#)
            .eval()
            .unwrap();
        assert!(matches!(absent, Value::Nil));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.txt");
        std::fs::write(&path, "x").unwrap();
        let removed: bool = lua
            .load(format!("return os.remove({:?})", path.to_string_lossy()))
            .eval()
            .unwrap();
        assert!(removed && !path.exists());
    }

    /// Luau keeps its own clock calls; the compat layer must not shadow them.
    #[test]
    fn the_language_os_functions_survive() {
        let lua = vm();
        let kinds: String = lua
            .load("return type(os.time) .. type(os.date) .. type(os.clock)")
            .eval()
            .unwrap();
        assert_eq!(kinds, "functionfunctionfunction");
    }

    /// What a plugin may NOT reach. `io.popen` and `os.execute` would be a
    /// second, ungated way to run a command; `cru.shell` is the gated one.
    #[test]
    fn no_process_door_is_opened() {
        let lua = vm();
        let missing: bool = lua
            .load("return io.popen == nil and os.execute == nil and io.loadlib == nil")
            .eval()
            .unwrap();
        assert!(missing, "the compat layer must open no process door");
    }
}
