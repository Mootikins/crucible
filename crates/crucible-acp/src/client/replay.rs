//! Fixture-driven replay transport for ACP tests.
//!
//! Loads a JSONL fixture produced by [`super::recording::Recorder`] and serves
//! the recorded incoming frames back to a [`super::CrucibleAcpClient`] in
//! response to its outgoing requests. Outgoing requests are validated against
//! the recorded outgoing sequence (by method) so a divergence in client
//! behavior surfaces as a test failure rather than silent drift.
//!
//! Usage:
//!
//! ```rust,ignore
//! let fixture = ReplayFixture::load("path/to/fixture.jsonl")?;
//! let (writer, reader, driver) = fixture.into_transport();
//! let client = CrucibleAcpClient::with_transport(config, writer, reader);
//! tokio::spawn(driver);
//! // ... drive the client; it will receive recorded responses
//! ```

use std::path::Path;
use std::pin::Pin;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::recording::{Direction, FixtureHeader, FrameRecord};
use super::{BoxedReader, BoxedWriter};

/// Loaded fixture, ready to be turned into a transport.
pub struct ReplayFixture {
    pub header: FixtureHeader,
    pub records: Vec<FrameRecord>,
}

/// Mismatch between the client's outgoing request and what the fixture
/// expected. Exposed so tests can assert on specific divergences.
#[derive(Debug, Clone)]
pub enum DivergenceKind {
    /// Client sent a request, fixture had no more outgoing frames.
    UnexpectedOutgoing { method: String },
    /// Client sent an incoming-shaped frame from the wrong side. (Unlikely;
    /// the client never reads from its own writer.)
    WrongDirection,
    /// Outgoing method didn't match. Compares JSON-RPC `method` field.
    MethodMismatch { expected: String, actual: String },
    /// Client closed the writer with frames remaining in the fixture.
    EarlyClose { remaining_outgoing: usize },
    /// Outgoing line wasn't valid JSON.
    UnparseableOutgoing { raw: String },
}

impl std::fmt::Display for DivergenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedOutgoing { method } => {
                write!(
                    f,
                    "client sent unexpected request {method:?}; fixture exhausted"
                )
            }
            Self::WrongDirection => write!(f, "transport received frame from wrong side"),
            Self::MethodMismatch { expected, actual } => {
                write!(f, "method mismatch: expected {expected:?}, got {actual:?}")
            }
            Self::EarlyClose { remaining_outgoing } => write!(
                f,
                "client closed writer with {remaining_outgoing} outgoing frames remaining"
            ),
            Self::UnparseableOutgoing { raw } => {
                write!(f, "outgoing frame not parseable as JSON: {raw}")
            }
        }
    }
}

impl std::error::Error for DivergenceKind {}

impl ReplayFixture {
    /// Load and parse a fixture file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Parse fixture body from text. Public for tests that build fixtures
    /// inline.
    pub fn parse(text: &str) -> Result<Self, ReplayError> {
        let mut lines = text.lines();
        let header_line = lines
            .next()
            .ok_or_else(|| ReplayError::Format("empty fixture".into()))?;
        let header: FixtureHeader = serde_json::from_str(header_line)
            .map_err(|e| ReplayError::Format(format!("bad header: {e}")))?;
        let mut records = Vec::new();
        for (i, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: FrameRecord = serde_json::from_str(line)
                .map_err(|e| ReplayError::Format(format!("bad frame at line {}: {e}", i + 2)))?;
            records.push(record);
        }
        Ok(Self { header, records })
    }

    /// Convert into a transport that the ACP client can use.
    ///
    /// Returns:
    /// - `writer` — pass to `CrucibleAcpClient::with_transport`
    /// - `reader` — pass to `CrucibleAcpClient::with_transport`
    /// - `driver` — a future that runs the replay loop. Spawn it on the
    ///   tokio runtime; await it after the client is done to collect any
    ///   divergences.
    pub fn into_transport(
        self,
    ) -> (
        BoxedWriter,
        BoxedReader,
        Pin<Box<dyn std::future::Future<Output = ReplayOutcome> + Send>>,
    ) {
        // Two duplex pairs: one for client→driver (the client writes
        // requests, the driver reads them), one for driver→client (the
        // driver writes responses, the client reads them).
        let (client_side_to_agent, our_inbox) = tokio::io::duplex(64 * 1024);
        let (our_outbox, client_side_from_agent) = tokio::io::duplex(64 * 1024);

        let writer: BoxedWriter = Box::pin(client_side_to_agent);
        let reader: BoxedReader = Box::pin(BufReader::new(client_side_from_agent));

        let records = self.records;
        let driver = async move { run_driver(records, our_inbox, our_outbox).await };

        (writer, reader, Box::pin(driver))
    }
}

#[derive(Debug)]
pub enum ReplayError {
    Format(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(s) => write!(f, "fixture format error: {s}"),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Result of running the driver to completion (or hitting an error).
#[derive(Debug, Default)]
pub struct ReplayOutcome {
    pub divergences: Vec<DivergenceKind>,
    pub frames_consumed: usize,
}

impl ReplayOutcome {
    pub fn is_clean(&self) -> bool {
        self.divergences.is_empty()
    }
}

async fn run_driver(
    records: Vec<FrameRecord>,
    our_inbox: tokio::io::DuplexStream,
    mut our_outbox: tokio::io::DuplexStream,
) -> ReplayOutcome {
    let mut outcome = ReplayOutcome::default();
    let mut iter = records.into_iter().peekable();
    let mut reader = BufReader::new(our_inbox);
    // Map fixture id → actual id sent by the client. Used to rewrite the
    // `id` field on incoming responses so the client's response-matching
    // logic finds them. Notifications have no id and are passed through.
    let mut id_remap: std::collections::HashMap<serde_json::Value, serde_json::Value> =
        std::collections::HashMap::new();

    loop {
        // Find the next "out" record (what the client should send next).
        let next_out = loop {
            match iter.peek() {
                Some(rec) if rec.dir == Direction::Out => break Some(()),
                Some(_) => {
                    // Drain any pending "in" records — they belong to a
                    // previous response that was already consumed before
                    // the client sent its next request. Emit them now.
                    let rec = iter.next().unwrap();
                    if let Err(err) = emit_incoming(&mut our_outbox, &rec, &id_remap).await {
                        tracing::warn!(?err, "failed to emit pending incoming frame");
                    }
                    outcome.frames_consumed += 1;
                }
                None => break None,
            }
        };

        if next_out.is_none() {
            // Fixture exhausted — wait for the client to drop its writer
            // (signals shutdown). If the client tries to send more, that's
            // a divergence.
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // clean EOF
                Ok(_) => {
                    let method = parse_method(&line).unwrap_or_else(|| "<unknown>".into());
                    outcome
                        .divergences
                        .push(DivergenceKind::UnexpectedOutgoing { method });
                }
                Err(_) => break,
            }
            continue;
        }

        // Read the client's next outgoing line.
        let mut line = String::new();
        let n = match reader.read_line(&mut line).await {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            let remaining = iter.filter(|r| r.dir == Direction::Out).count();
            outcome.divergences.push(DivergenceKind::EarlyClose {
                remaining_outgoing: remaining,
            });
            break;
        }

        let trimmed = line.trim_end();
        let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                outcome
                    .divergences
                    .push(DivergenceKind::UnparseableOutgoing {
                        raw: trimmed.into(),
                    });
                continue;
            }
        };
        let actual_method = parsed
            .get("method")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();
        let actual_id = parsed.get("id").cloned();

        let expected_record = iter.next().expect("peeked Some(Out) above");
        outcome.frames_consumed += 1;
        let expected_method = expected_record
            .frame
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let expected_id = expected_record.frame.get("id").cloned();

        if expected_method != actual_method {
            outcome.divergences.push(DivergenceKind::MethodMismatch {
                expected: expected_method,
                actual: actual_method,
            });
        }

        // Record the id remap so subsequent "in" responses can be rewritten.
        if let (Some(fix_id), Some(act_id)) = (expected_id, actual_id) {
            id_remap.insert(fix_id, act_id);
        }

        // Emit every "in" record up to (but not including) the next "out".
        while let Some(peek) = iter.peek() {
            if peek.dir == Direction::Out {
                break;
            }
            let rec = iter.next().unwrap();
            if let Err(err) = emit_incoming(&mut our_outbox, &rec, &id_remap).await {
                tracing::warn!(?err, "failed to emit incoming frame");
                break;
            }
            outcome.frames_consumed += 1;
        }
    }

    outcome
}

async fn emit_incoming(
    out: &mut tokio::io::DuplexStream,
    rec: &FrameRecord,
    id_remap: &std::collections::HashMap<serde_json::Value, serde_json::Value>,
) -> std::io::Result<()> {
    let mut frame = rec.frame.clone();
    if let Some(obj) = frame.as_object_mut() {
        if let Some(id) = obj.get("id").cloned() {
            if let Some(remapped) = id_remap.get(&id) {
                obj.insert("id".to_string(), remapped.clone());
            }
        }
    }
    let mut bytes = serde_json::to_vec(&frame).unwrap_or_else(|_| b"null".to_vec());
    bytes.push(b'\n');
    out.write_all(&bytes).await?;
    out.flush().await?;
    Ok(())
}

fn parse_method(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    v.get("method")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_fixture(body: &str) -> String {
        format!(
            r#"{{"version":1,"agent":"test","recorded_at":"2026-04-19T00:00:00Z"}}
{body}"#
        )
    }

    #[test]
    fn parses_header_and_records() {
        let body = r#"{"t_ms":0,"dir":"out","frame":{"jsonrpc":"2.0","id":1,"method":"initialize"}}
{"t_ms":1,"dir":"in","frame":{"jsonrpc":"2.0","id":1,"result":{"ok":true}}}"#;
        let fixture = ReplayFixture::parse(&build_fixture(body)).unwrap();
        assert_eq!(fixture.header.agent, "test");
        assert_eq!(fixture.records.len(), 2);
        assert_eq!(fixture.records[0].dir, Direction::Out);
    }

    #[tokio::test]
    async fn driver_emits_recorded_responses() {
        let body = r#"{"t_ms":0,"dir":"out","frame":{"jsonrpc":"2.0","id":1,"method":"initialize"}}
{"t_ms":1,"dir":"in","frame":{"jsonrpc":"2.0","id":1,"result":{"ok":true}}}"#;
        let fixture = ReplayFixture::parse(&build_fixture(body)).unwrap();
        let (writer, reader, driver) = fixture.into_transport();
        let driver_handle = tokio::spawn(driver);

        // Pretend to be the client: write a request, read a response.
        let mut writer = writer;
        let mut reader = reader;
        writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
            .await
            .unwrap();
        writer.flush().await.unwrap();

        let mut response = String::new();
        reader.read_line(&mut response).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["result"]["ok"], true);

        // Closing the writer signals end-of-session.
        drop(writer);
        let outcome = driver_handle.await.unwrap();
        assert!(outcome.is_clean(), "divergences: {:?}", outcome.divergences);
        assert_eq!(outcome.frames_consumed, 2);
    }

    #[tokio::test]
    async fn driver_flags_method_mismatch() {
        let body = r#"{"t_ms":0,"dir":"out","frame":{"jsonrpc":"2.0","id":1,"method":"initialize"}}
{"t_ms":1,"dir":"in","frame":{"jsonrpc":"2.0","id":1,"result":{}}}"#;
        let fixture = ReplayFixture::parse(&build_fixture(body)).unwrap();
        let (mut writer, _reader, driver) = fixture.into_transport();
        let driver_handle = tokio::spawn(driver);

        // Send the wrong method.
        writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session/new\"}\n")
            .await
            .unwrap();
        writer.flush().await.unwrap();
        drop(writer);

        let outcome = driver_handle.await.unwrap();
        assert_eq!(outcome.divergences.len(), 1);
        match &outcome.divergences[0] {
            DivergenceKind::MethodMismatch { expected, actual } => {
                assert_eq!(expected, "initialize");
                assert_eq!(actual, "session/new");
            }
            other => panic!("wrong divergence: {other:?}"),
        }
    }
}
