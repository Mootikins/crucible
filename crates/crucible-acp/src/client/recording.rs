//! Wire-level recorder for ACP JSON-RPC traffic.
//!
//! Records every request/response/notification line that flows in or out of
//! the ACP client to a JSONL fixture file. Activated by the
//! `CRUCIBLE_ACP_RECORD_DIR` environment variable.
//!
//! Fixture format (see `thoughts/shared/plans/2026-04-19-acp-test-infrastructure.md`
//! for full design):
//!
//! ```jsonl
//! {"version":1,"agent":"claude","agent_version":"unknown","recorded_at":"...","scenario":"basic-chat"}
//! {"t_ms":0,"dir":"out","frame":{...}}
//! {"t_ms":42,"dir":"in","frame":{...}}
//! ```

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

const FIXTURE_VERSION: u32 = 1;

/// Direction of a recorded frame relative to the client.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Client → agent (request, notification, response to agent request).
    Out,
    /// Agent → client (response, notification, request).
    In,
}

/// Header metadata written as the first line of every fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureHeader {
    pub version: u32,
    pub agent: String,
    #[serde(default)]
    pub agent_version: Option<String>,
    pub recorded_at: String,
    #[serde(default)]
    pub scenario: Option<String>,
}

/// One frame of JSON-RPC traffic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameRecord {
    pub t_ms: u64,
    pub dir: Direction,
    pub frame: serde_json::Value,
}

/// Live recorder. Owned by `CrucibleAcpClient`; tees frames at the I/O
/// boundary in `client/io.rs`.
pub struct Recorder {
    writer: BufWriter<File>,
    started: Instant,
    path: PathBuf,
}

impl Recorder {
    /// Construct a recorder if the recording env var is set, else `None`.
    ///
    /// `agent_name` is used to name the fixture file:
    /// `<dir>/<agent>-<unix_ms>.jsonl`. Multiple sessions in the same dir
    /// don't clobber each other.
    pub fn from_env(agent_name: &str) -> Option<Self> {
        let dir = std::env::var("CRUCIBLE_ACP_RECORD_DIR").ok()?;
        let dir = PathBuf::from(dir);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!(?dir, ?err, "failed to create ACP record dir");
            return None;
        }
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let scenario = std::env::var("CRUCIBLE_ACP_RECORD_SCENARIO").ok();
        let safe_agent = sanitize(agent_name);
        let filename = format!("{safe_agent}-{unix_ms}.jsonl");
        let path = dir.join(filename);
        Self::create(&path, agent_name, scenario)
            .map_err(|err| tracing::warn!(?path, ?err, "failed to start ACP recorder"))
            .ok()
    }

    /// Create a recorder writing to `path`. Used by tests.
    pub fn create(
        path: &Path,
        agent_name: &str,
        scenario: Option<String>,
    ) -> std::io::Result<Self> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        let header = FixtureHeader {
            version: FIXTURE_VERSION,
            agent: agent_name.to_string(),
            agent_version: None,
            recorded_at: now_iso8601(),
            scenario,
        };
        serde_json::to_writer(&mut writer, &header)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        tracing::info!(?path, agent = agent_name, "ACP recorder started");
        Ok(Self {
            writer,
            started: Instant::now(),
            path: path.to_path_buf(),
        })
    }

    /// Record one JSON-RPC line. `raw_line` is the wire bytes (without the
    /// trailing newline). Parsing happens here so a malformed line surfaces
    /// in the trace as a `null` frame rather than corrupting the file.
    pub fn record_line(&mut self, dir: Direction, raw_line: &str) {
        let frame: serde_json::Value =
            serde_json::from_str(raw_line).unwrap_or(serde_json::Value::Null);
        let t_ms = self.started.elapsed().as_millis() as u64;
        let record = FrameRecord { t_ms, dir, frame };
        if let Err(err) = serde_json::to_writer(&mut self.writer, &record) {
            tracing::warn!(?err, "failed to write ACP record");
            return;
        }
        if let Err(err) = self.writer.write_all(b"\n") {
            tracing::warn!(?err, "failed to write ACP record newline");
        }
        // Flush after each line — fixture is small and we want partial
        // captures even if the process is killed mid-session.
        if let Err(err) = self.writer.flush() {
            tracing::warn!(?err, "failed to flush ACP recorder");
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder").field("path", &self.path).finish()
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // RFC 3339 minus subsec — good enough for fixture provenance
    let (y, mo, d, h, mi, s) = unix_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Unix seconds → (year, month, day, hour, min, sec). Uses a basic algorithm;
/// good through 2400. We don't pull `chrono` just for this.
fn unix_to_ymdhms(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let mdays: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0;
    while days >= mdays[month] {
        days -= mdays[month];
        month += 1;
    }
    (year, (month + 1) as u64, days + 1, h, m, s)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn records_header_and_frames_in_order() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut rec = Recorder::create(&path, "claude", Some("basic".into())).unwrap();
        rec.record_line(Direction::Out, r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        rec.record_line(Direction::In, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
        drop(rec);

        let body = std::fs::read_to_string(&path).unwrap();
        let mut lines = body.lines();

        let header: FixtureHeader = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header.version, FIXTURE_VERSION);
        assert_eq!(header.agent, "claude");
        assert_eq!(header.scenario.as_deref(), Some("basic"));

        let r1: FrameRecord = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(r1.dir, Direction::Out);
        assert_eq!(r1.frame["method"], "initialize");

        let r2: FrameRecord = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(r2.dir, Direction::In);
        assert_eq!(r2.frame["id"], 1);

        assert!(lines.next().is_none(), "expected exactly two frames");
    }

    #[test]
    fn malformed_line_recorded_as_null_frame() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut rec = Recorder::create(&path, "test", None).unwrap();
        rec.record_line(Direction::In, "not valid json");
        drop(rec);

        let body = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "header + 1 frame");
        let r: FrameRecord = serde_json::from_str(lines[1]).unwrap();
        assert!(r.frame.is_null(), "malformed → null, file still parseable");
    }

    #[test]
    fn from_env_returns_none_without_env_var() {
        // Ensure env var is unset for this test
        // SAFETY: tests run in isolation per process by default in nextest;
        // if running serially via cargo test, this could race. Acceptable for
        // a smoke check.
        std::env::remove_var("CRUCIBLE_ACP_RECORD_DIR");
        assert!(Recorder::from_env("any").is_none());
    }
}
