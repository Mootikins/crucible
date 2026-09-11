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
//! `io.open` answers.
//!
//! `io.popen` and `os.execute` are here too, and they were not always. The
//! host held them back while `cru.shell` looked like the one gated door to a
//! process. It is not a door with a lock: `PluginShellPolicy::default()`
//! names four blocked commands and no allow-list, and the check reads only
//! the command name, so `cru.shell.exec("sh", { "-c", … })` already runs
//! anything. A shipped plugin (`runtime/plugins/oci`) depends on exactly that
//! shape. The owner ruled that a plugin is code the operator installed, and
//! that it gets the API the way an editor plugin gets the editor. Withholding
//! two functions bought no containment and cost every plugin that PUC Lua
//! wrote for.
//!
//! `loadlib` stays out, and so does `os.exit`. `loadlib` loads native code,
//! which no policy can inspect. `os.exit` ends the daemon process: every
//! session, every socket and every pending write go with it, and a plugin
//! that calls it by accident cannot be told from one that means it.

use mlua::{Lua, MultiValue, UserData, UserDataMethods, Value};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::Mutex;

/// An open handle, or a closed one. `Mutex` rather than `RefCell` because the
/// `send` feature puts the VM behind a lock and every userdata with it.
///
/// A file and a pipe share this ONE userdata type. The alternative — a second
/// type for `io.popen` — forks `read`, `lines`, `write`, `seek` and `flush`
/// into two copies that must stay equal, and makes `io.type` try two borrows
/// before it answers. The difference between the two is what they move bytes
/// through and what `close` answers, so that is where the host puts it: a
/// `Stream` enum and an optional child process.
struct LuaFile {
    handle: Mutex<Option<OpenFile>>,
    /// What the handle came from: a path, or the command a pipe runs. It
    /// names the handle in the message a closed handle raises.
    origin: String,
}

/// What a handle moves bytes through.
///
/// The stream carries the direction, and `OpenFile` does not. A separate
/// `writable` flag beside it made the pipe arm of [`Stream::writer`] dead:
/// the flag refused a read pipe first, so no test could reach the arm and
/// prove it. One answer, in one place.
enum Stream {
    /// A file on disk. It seeks, and it writes when the mode allows it.
    File {
        file: BufReader<File>,
        writable: bool,
    },
    /// The standard output of a child process, for `io.popen(cmd, "r")`.
    PipeRead(BufReader<ChildStdout>),
    /// The standard input of a child process, for `io.popen(cmd, "w")`.
    PipeWrite(ChildStdin),
}

impl Stream {
    fn reader(&mut self) -> mlua::Result<&mut dyn BufRead> {
        match self {
            Stream::File { file, .. } => Ok(file),
            Stream::PipeRead(pipe) => Ok(pipe),
            Stream::PipeWrite(_) => Err(mlua::Error::runtime("the handle is not open for reading")),
        }
    }

    fn writer(&mut self) -> mlua::Result<&mut dyn Write> {
        match self {
            Stream::File {
                file,
                writable: true,
            } => Ok(file.get_mut()),
            Stream::PipeWrite(pipe) => Ok(pipe),
            Stream::File { .. } | Stream::PipeRead(_) => {
                Err(mlua::Error::runtime("the handle is not open for writing"))
            }
        }
    }
}

/// A child process this handle must not leave as a zombie.
///
/// `std::process::Child` does not wait when it drops, so a plugin that never
/// closes an `io.popen` handle collects one dead process for each call. The
/// host must not wait in `drop` either: the garbage collector runs `drop`
/// while it holds the VM, and a child that never exits stops every session.
/// A short thread does the wait instead.
///
/// The thread is also what makes the drop order safe. A child that reads its
/// input runs until the pipe closes, and `drop` cannot close the pipe before
/// this field runs. Because the wait is on its own thread, the pipe closes a
/// moment later and the thread finishes; a wait in place would not.
///
/// [`close_handle`] is the other path, and it is synchronous. There the host
/// drops the stream FIRST and then waits, or a `"w"` handle deadlocks.
struct ChildReaper(Option<Child>);

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

struct OpenFile {
    stream: Stream,
    reaper: ChildReaper,
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
                self.origin
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
        let reader = open.stream.reader()?;
        match self {
            ReadFormat::All => {
                let mut buffer = String::new();
                reader
                    .read_to_string(&mut buffer)
                    .map_err(mlua::Error::external)?;
                Ok(Value::String(lua.create_string(&buffer)?))
            }
            ReadFormat::Line { keep_newline } => {
                let mut buffer = String::new();
                let read = reader
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
                let read = reader
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
                    let read = reader
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

        // `for line in handle:lines()` — the generic `for` calls the
        // iterator with the STATE, so the file has to be the state. Returning
        // `()` there left the iterator reading its argument as nil.
        methods.add_function(
            "lines",
            |lua, (this, format): (mlua::AnyUserData, Value)| {
                let format = ReadFormat::parse(&format)?;
                let iterator = lua.create_function_mut(move |lua, this: mlua::AnyUserData| {
                    let file = this.borrow::<LuaFile>()?;
                    file.with_open(|open| format.read(lua, open))
                })?;
                Ok((iterator, this))
            },
        );

        // `write` answers with the file itself, as Lua's does, so a caller
        // can test the return to tell a failed write from a good one. No
        // shipped plugin does: all three that read `local wrote, err =
        // handle:write(...)` had a dead error branch, because this method
        // RAISES on failure rather than answering nil.
        methods.add_function(
            "write",
            |_, (this, values): (mlua::AnyUserData, MultiValue)| {
                {
                    let file = this.borrow::<LuaFile>()?;
                    file.with_open(|open| {
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
                            open.stream
                                .writer()?
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
                    let Stream::File { file, .. } = &mut open.stream else {
                        return Err(mlua::Error::runtime("cannot seek a process pipe"));
                    };
                    file.seek(position).map_err(mlua::Error::external)
                })
            },
        );

        // A read handle has nothing to flush, and Lua does not fail one that
        // asks. So an unwritable stream flushes nothing and answers well.
        methods.add_function("flush", |_, this: mlua::AnyUserData| {
            {
                let file = this.borrow::<LuaFile>()?;
                file.with_open(|open| {
                    if let Ok(writer) = open.stream.writer() {
                        writer.flush().map_err(mlua::Error::external)?;
                    }
                    Ok(())
                })?;
            }
            Ok(this)
        });

        methods.add_method("close", |lua, this, ()| close_handle(lua, this));
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
            let handle = lua.create_userdata(LuaFile {
                handle: Mutex::new(Some(OpenFile {
                    stream: Stream::File {
                        file: BufReader::new(file),
                        writable: cleaned != "r",
                    },
                    reaper: ChildReaper(None),
                })),
                origin: path.clone(),
            })?;
            Ok(MultiValue::from_iter([Value::UserData(handle)]))
        }
        Err(e) => Ok(MultiValue::from_iter([
            Value::Nil,
            Value::String(lua.create_string(format!("{path}: {e}"))?),
        ])),
    }
}

/// The shell a command string runs under.
///
/// PUC Lua hands the string to the C library's `system`, which starts
/// `/bin/sh -c` on every POSIX host. The host does the same, so a plugin that
/// writes `os.execute("mkdir -p a/b")` gets the shell it expects.
fn shell_command(command: &str) -> Command {
    let mut process = Command::new("/bin/sh");
    process.arg("-c").arg(command);
    process
}

/// The three values Lua answers a process call with.
///
/// PUC Lua 5.4 gives `true` when the command ended with status zero, and
/// `nil` otherwise; then `"exit"` or `"signal"`; then the number. `os.execute`
/// answers this, and so does `close` on a handle `io.popen` made.
///
/// A number, not an integer: Luau has one numeric type, and an exit code and
/// a signal both fit a double exactly.
fn status_values(lua: &Lua, status: &ExitStatus) -> mlua::Result<MultiValue> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Ok(MultiValue::from_iter([
                Value::Nil,
                Value::String(lua.create_string("signal")?),
                Value::Number(f64::from(signal)),
            ]));
        }
    }
    let code = status.code().unwrap_or(-1);
    let ok = if code == 0 {
        Value::Boolean(true)
    } else {
        Value::Nil
    };
    Ok(MultiValue::from_iter([
        ok,
        Value::String(lua.create_string("exit")?),
        Value::Number(f64::from(code)),
    ]))
}

/// The shape Lua reports when the command never started at all.
///
/// `luaL_fileresult` answers `nil`, a message and the C error number. The
/// host keeps that shape, so one `if not ok then print(message) end` covers
/// both a shell that failed to start and a command that failed to run.
fn spawn_failure(lua: &Lua, command: &str, error: &std::io::Error) -> mlua::Result<MultiValue> {
    Ok(MultiValue::from_iter([
        Value::Nil,
        Value::String(lua.create_string(format!("{command}: {error}"))?),
        Value::Number(f64::from(error.raw_os_error().unwrap_or(-1))),
    ]))
}

/// Close one handle, and answer as Lua does.
///
/// A file answers `true`. A pipe answers the status triple, because PUC Lua
/// closes a popen handle with `pclose` and reports what the child did. The
/// stream closes first: a child that reads its input runs until it sees
/// end-of-file, and the open handle is that end.
fn close_handle(lua: &Lua, file: &LuaFile) -> mlua::Result<MultiValue> {
    let mut slot = file
        .handle
        .lock()
        .map_err(|_| mlua::Error::runtime("file handle lock poisoned"))?;
    let Some(mut open) = slot.take() else {
        return Ok(MultiValue::from_iter([Value::Boolean(true)]));
    };
    if let Ok(writer) = open.stream.writer() {
        let _ = writer.flush();
    }
    let Some(mut child) = open.reaper.0.take() else {
        return Ok(MultiValue::from_iter([Value::Boolean(true)]));
    };
    drop(open);
    let status = child.wait().map_err(mlua::Error::external)?;
    status_values(lua, &status)
}

/// Start a command under the shell, and hand back one of its pipes.
fn popen(lua: &Lua, command: String, mode: Option<String>) -> mlua::Result<MultiValue> {
    let mode = mode.unwrap_or_else(|| "r".to_string());
    let cleaned: String = mode.chars().filter(|c| *c != 'b').collect();
    let reading = match cleaned.as_str() {
        "r" => true,
        "w" => false,
        other => {
            return Err(mlua::Error::runtime(format!(
                "bad popen mode '{other}' (expected r or w)"
            )))
        }
    };

    let mut process = shell_command(&command);
    if reading {
        process.stdout(Stdio::piped());
    } else {
        process.stdin(Stdio::piped());
    }

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => return spawn_failure(lua, &command, &error),
    };

    let stream = if reading {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| mlua::Error::runtime("the child process gave no standard output"))?;
        Stream::PipeRead(BufReader::new(stdout))
    } else {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| mlua::Error::runtime("the child process took no standard input"))?;
        Stream::PipeWrite(stdin)
    };

    let handle = lua.create_userdata(LuaFile {
        handle: Mutex::new(Some(OpenFile {
            stream,
            reaper: ChildReaper(Some(child)),
        })),
        origin: format!("popen {command}"),
    })?;
    Ok(MultiValue::from_iter([Value::UserData(handle)]))
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
            lua.create_function(|lua, handle: mlua::AnyUserData| {
                let file = handle.borrow::<LuaFile>()?;
                close_handle(lua, &file)
            })?,
        )?;
        io.set(
            "popen",
            lua.create_function(|lua, (command, mode): (String, Option<String>)| {
                popen(lua, command, mode)
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

    if matches!(os.get::<Value>("execute")?, Value::Nil) {
        os.set(
            "execute",
            lua.create_function(|lua, command: Option<String>| {
                // No argument asks one question: is a shell there? Lua answers
                // a plain boolean, with no status pair after it. The host
                // starts a shell that does nothing, because a shell file that
                // is present and not executable would pass a test of the path.
                let Some(command) = command else {
                    let available = shell_command("exit 0").status().is_ok();
                    return Ok(MultiValue::from_iter([Value::Boolean(available)]));
                };
                match shell_command(&command).status() {
                    Ok(status) => status_values(lua, &status),
                    Err(error) => spawn_failure(lua, &command, &error),
                }
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

    /// `write` answers with the file, as Lua's does. Failure RAISES, so the
    /// `local wrote, err = handle:write(...)` shape those three plugins used
    /// could never see the error it named.
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

    /// Both iteration forms. The generic `for` hands the iterator the state,
    /// so the file must BE the state.
    #[test]
    fn lines_iterates_a_file_both_ways() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("three.txt");
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        let path = path.to_string_lossy().to_string();

        let joined: String = lua
            .load(format!(
                r#"
                local out = {{}}
                for line in io.lines({path:?}) do out[#out + 1] = line end
                local handle = assert(io.open({path:?}, "r"))
                for line in handle:lines() do out[#out + 1] = line end
                handle:close()
                return table.concat(out, "")
                "#
            ))
            .eval()
            .expect("both forms iterate");
        assert_eq!(joined, "abcabc");
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

    /// A plugin reaches a process, and still reaches no native code.
    ///
    /// The host once withheld `io.popen` and `os.execute`, on the reading
    /// that `cru.shell` was the one gated door to a command. That reading was
    /// wrong: `PluginShellPolicy::default()` blocks four command names with
    /// no allow-list, and `prepare_command` reads the command name and never
    /// the arguments, so `cru.shell.exec("sh", { "-c", … })` runs anything
    /// today. `runtime/plugins/oci` ships that exact call. The owner ruled
    /// that a plugin is code the operator installed, and that it gets the
    /// API the way an editor plugin gets the editor.
    ///
    /// `loadlib` is a different question and keeps its old answer. It loads
    /// native code, which no policy can read; see the `package` tests in
    /// `executor.rs` for the import half of the same rule.
    #[test]
    fn a_plugin_reaches_a_process_but_not_native_code() {
        let lua = vm();
        let kinds: String = lua
            .load("return type(io.popen) .. type(os.execute)")
            .eval()
            .unwrap();
        assert_eq!(kinds, "functionfunction", "both must exist");

        let ran: bool = lua
            .load(r#"return (os.execute("exit 0"))"#)
            .eval()
            .expect("os.execute runs");
        assert!(ran, "a command that succeeds answers true");

        let out: String = lua
            .load(
                r#"
                local handle = assert(io.popen("printf hello"))
                local text = handle:read("a")
                handle:close()
                return text
                "#,
            )
            .eval()
            .expect("io.popen reads");
        assert_eq!(out, "hello");

        let native: bool = lua.load("return io.loadlib == nil").eval().unwrap();
        assert!(native, "native code stays out of reach");
    }

    /// A plugin runs a command and still cannot end the daemon.
    ///
    /// `os.exit` ends the process it runs in, and a plugin runs in the
    /// daemon: every session, every socket and every write that has not
    /// landed would go with it. A plugin that means to stop asks the host.
    /// Luau omits `os.exit`, and the compat layer must not put it back.
    #[test]
    fn a_plugin_cannot_end_the_daemon() {
        let lua = vm();
        let absent: bool = lua.load("return os.exit == nil").eval().unwrap();
        assert!(absent, "os.exit must stay out of a plugin's reach");
    }

    /// `os.execute` answers as PUC Lua 5.4 does: the success flag, then the
    /// reason, then the number. A plugin branches on all three.
    #[test]
    fn os_execute_reports_the_exit_status() {
        let lua = vm();
        let (ok, reason, code): (Value, String, f64) = lua
            .load(r#"return os.execute("exit 7")"#)
            .eval()
            .expect("os.execute answers three values");
        assert!(matches!(ok, Value::Nil), "a failure answers nil, not false");
        assert_eq!(reason, "exit");
        assert_eq!(code, 7.0);

        let (ok, reason, code): (Value, String, f64) =
            lua.load(r#"return os.execute("exit 0")"#).eval().unwrap();
        assert!(matches!(ok, Value::Boolean(true)));
        assert_eq!(reason, "exit");
        assert_eq!(code, 0.0);
    }

    /// A signal is the other reason a command ends. PUC Lua names it, and a
    /// plugin that retries must tell it from an ordinary non-zero exit.
    #[cfg(unix)]
    #[test]
    fn os_execute_names_a_signal() {
        let lua = vm();
        let (ok, reason, number): (Value, String, f64) = lua
            .load(r#"return os.execute("kill -TERM $$")"#)
            .eval()
            .expect("os.execute answers");
        assert!(matches!(ok, Value::Nil));
        assert_eq!(reason, "signal", "a killed command is not an exit");
        assert_eq!(number, 15.0, "SIGTERM is 15");
    }

    /// No argument asks whether a shell is there. Lua answers one boolean.
    #[test]
    fn os_execute_with_no_command_reports_the_shell() {
        let lua = vm();
        let values: MultiValue = lua.load("return os.execute()").eval().unwrap();
        assert_eq!(values.len(), 1, "one value, not a status triple");
        assert!(matches!(values.front(), Some(Value::Boolean(true))));
    }

    /// Mode `"w"` writes to the command's input. The child sees end-of-file
    /// when `close` drops the pipe, so the file it writes is complete.
    #[test]
    fn popen_writes_to_a_command() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("piped.txt").to_string_lossy().to_string();
        let ok: bool = lua
            .load(format!(
                r#"
                local handle = assert(io.popen("cat > {path}", "w"))
                handle:write("one\ntwo\n")
                return (handle:close())
                "#
            ))
            .eval()
            .expect("io.popen writes");
        assert!(ok, "the command ended well");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    }

    /// Closing a pipe reports the child, and closing a file reports `true`.
    /// The two answers differ, and a plugin reads both from `close`.
    #[test]
    fn closing_a_pipe_reports_the_child_and_a_file_does_not() {
        let lua = vm();
        let (ok, reason, code): (Value, String, f64) = lua
            .load(r#"local h = assert(io.popen("exit 3")) return h:close()"#)
            .eval()
            .expect("close answers the status");
        assert!(matches!(ok, Value::Nil));
        assert_eq!(reason, "exit");
        assert_eq!(code, 3.0);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.txt");
        std::fs::write(&path, "x").unwrap();
        let values: MultiValue = lua
            .load(format!(
                r#"local h = assert(io.open({:?}, "r")) return h:close()"#,
                path.to_string_lossy()
            ))
            .eval()
            .unwrap();
        assert_eq!(values.len(), 1, "a file has no exit status");
        assert!(matches!(values.front(), Some(Value::Boolean(true))));
    }

    /// `io.type` reads one userdata type, so it must answer for a pipe too.
    /// `io.close` must take a pipe and report the child in the same shape.
    #[test]
    fn io_type_and_io_close_accept_a_pipe() {
        let lua = vm();
        let kinds: String = lua
            .load(
                r#"
                local pipe = assert(io.popen("exit 0"))
                local before = io.type(pipe)
                io.close(pipe)
                return before .. "/" .. io.type(pipe) .. "/" .. tostring(io.type(7))
                "#,
            )
            .eval()
            .expect("io.type answers for a pipe");
        assert_eq!(kinds, "file/closed file/nil");
    }

    /// A pipe is not a file. Seeking one must say so, rather than report a
    /// position the caller cannot use.
    #[test]
    fn a_pipe_refuses_to_seek() {
        let lua = vm();
        let err = lua
            .load(r#"local h = assert(io.popen("exit 0")) return h:seek("set", 0)"#)
            .eval::<Value>()
            .expect_err("a pipe must refuse a seek");
        assert!(err.to_string().contains("cannot seek"), "got: {err}");
    }

    /// A plugin drops a pipe handle without closing it, and the host still
    /// reaps the child and still delivers what the plugin wrote.
    ///
    /// The reaper thread is the reason both hold. Without it the child stays
    /// a zombie for as long as the daemon runs.
    #[test]
    fn a_dropped_pipe_handle_finishes_its_child() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dropped.txt").to_string_lossy().to_string();
        lua.load(format!(
            r#"
                local handle = assert(io.popen("cat > {path}", "w"))
                handle:write("dropped\n")
                "#
        ))
        .exec()
        .expect("the plugin writes and forgets the handle");

        lua.gc_collect().expect("collect");
        lua.gc_collect().expect("collect again");

        // Poll rather than sleep: the reaper thread and `cat` both run on
        // their own schedule, and a fixed wait would guess at it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if std::fs::read_to_string(&path).unwrap_or_default() == "dropped\n" {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the child never received the write"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The reaper waits, so no dead child stays in the process table.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_reaper_waits_for_the_child() {
        let child = shell_command("exit 0").spawn().expect("spawn");
        let pid = child.id();
        drop(ChildReaper(Some(child)));

        // Linux keeps `/proc/<pid>` until someone waits for the child. The
        // directory going away IS the wait.
        let entry = format!("/proc/{pid}");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::path::Path::new(&entry).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the child stayed a zombie"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// A file opened for reading refuses a write, and says so. The stream
    /// carries the direction now, so a file and a pipe give the same answer.
    #[test]
    fn a_read_only_file_refuses_a_write() {
        let lua = vm();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("read_only.txt");
        std::fs::write(&path, "x").unwrap();
        let err = lua
            .load(format!(
                r#"local h = assert(io.open({:?}, "r")) return h:write("y")"#,
                path.to_string_lossy()
            ))
            .eval::<Value>()
            .expect_err("a read-only file must refuse a write");
        assert!(
            err.to_string().contains("not open for writing"),
            "got: {err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
    }

    /// A read pipe holds no input, and a write pipe holds no output. Each
    /// must refuse the other direction rather than answer nothing.
    #[test]
    fn a_pipe_refuses_the_wrong_direction() {
        let lua = vm();
        let err = lua
            .load(r#"local h = assert(io.popen("cat > /dev/null", "w")) return h:read("a")"#)
            .eval::<Value>()
            .expect_err("a write pipe must refuse a read");
        assert!(
            err.to_string().contains("not open for reading"),
            "got: {err}"
        );

        let err = lua
            .load(r#"local h = assert(io.popen("exit 0")) return h:write("x")"#)
            .eval::<Value>()
            .expect_err("a read pipe must refuse a write");
        assert!(
            err.to_string().contains("not open for writing"),
            "got: {err}"
        );
    }

    /// A mode Lua does not have is an author's mistake, and `io.open` in this
    /// file raises on one. `io.popen` keeps that rule.
    #[test]
    fn popen_refuses_a_mode_it_does_not_have() {
        let lua = vm();
        let err = lua
            .load(r#"return io.popen("exit 0", "rw")"#)
            .eval::<Value>()
            .expect_err("a bad mode must raise");
        assert!(err.to_string().contains("bad popen mode"), "got: {err}");
    }

    /// `lines` reads one userdata type, so it iterates a pipe as it does a
    /// file. The generic `for` hands the iterator the handle as the state.
    #[test]
    fn lines_iterates_a_pipe() {
        let lua = vm();
        let joined: String = lua
            .load(
                r#"
                local out = {}
                local handle = assert(io.popen("printf 'a\nb\nc\n'"))
                for line in handle:lines() do out[#out + 1] = line end
                handle:close()
                return table.concat(out, "")
                "#,
            )
            .eval()
            .expect("a pipe iterates");
        assert_eq!(joined, "abc");
    }
}
