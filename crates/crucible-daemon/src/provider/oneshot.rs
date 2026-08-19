//! One-shot completions: a single exchange against a session's own client.
//!
//! The primitive under `cru.sessions.complete`. One request, one answer — no
//! tools, no history, no turn loop, and nothing written back to the session.
//! It is the shape every "ask the model a small question about this session"
//! feature wants: titling it, summarising it, classifying it.
//!
//! What lives here is only the mechanics: build the request, bound the wait,
//! hand back the text. The prompt is the caller's, and the caller is Lua —
//! [`runtime/plugins/auto-title`] is the worked example. A prompt compiled in
//! here would be the thing this module exists to have removed.
//!
//! Lives behind the provider seam (architecture gate A3: `genai` types stay in
//! `provider/` + `agent_factory.rs`). The agent manager builds the client via
//! `agent_factory::build_chat_client_for_agent` and hands it here.

use genai::chat::{ChatMessage, ChatOptions, ChatRequest};
use std::time::Duration;

/// How long a one-shot completion may take before it is abandoned.
///
/// Named and defaulted in one place because every caller so far is a
/// background nicety: a title, a summary. None of them is worth holding a task
/// open for the provider's own timeout, and none of them has a user waiting
/// who could cancel it. A caller that needs longer says so — see
/// [`timeout_from`].
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Resolve a caller's `timeout` option into a duration.
///
/// `None` and `Some(0)` both mean "the default": a plugin that omits the key
/// and one that computes a zero from an unset config are the same mistake, and
/// a zero-second timeout would fail every call before the request was sent.
pub(crate) fn timeout_from(secs: Option<u64>) -> Duration {
    match secs {
        Some(s) if s > 0 => Duration::from_secs(s),
        _ => Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    }
}

/// One exchange against `client`, answered as text.
///
/// `system` is optional: a caller with a single self-contained prompt needs no
/// system turn. Errors are strings because every caller surfaces them to Lua,
/// which has no error enum to match on.
pub(crate) async fn complete(
    client: &genai::Client,
    model: &genai::ModelIden,
    system: Option<&str>,
    prompt: &str,
    timeout: Duration,
) -> Result<String, String> {
    let model_name = super::genai_handle::explicit_model_name(model);

    let mut messages = Vec::new();
    if let Some(system) = system.filter(|s| !s.is_empty()) {
        messages.push(ChatMessage::system(system));
    }
    messages.push(ChatMessage::user(prompt));

    let options = ChatOptions::default().with_capture_content(true);
    let call = client.exec_chat(&model_name, ChatRequest::new(messages), Some(&options));

    let resp = bounded(timeout, async {
        call.await.map_err(|e| format!("completion failed: {e}"))
    })
    .await?;
    Ok(resp.content.texts().join(""))
}

/// Wait for `call`, giving up after `timeout`.
///
/// Split from [`complete`] so the bound is exercised by a test rather than
/// asserted by reading: the future it wraps in production needs a live
/// provider, and a timeout nothing proves is a timeout nobody notices has
/// stopped working.
async fn bounded<T>(
    timeout: Duration,
    call: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(timeout, call).await {
        Ok(result) => result,
        Err(_) => Err(format!("completion timed out after {}s", timeout.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_timeout_is_the_default() {
        assert_eq!(
            timeout_from(None),
            Duration::from_secs(DEFAULT_TIMEOUT_SECS)
        );
    }

    /// A zero would otherwise fail every call before the request left the
    /// process — an unset config key computing to 0 is the way that happens.
    #[test]
    fn a_zero_timeout_is_the_default_too() {
        assert_eq!(
            timeout_from(Some(0)),
            Duration::from_secs(DEFAULT_TIMEOUT_SECS)
        );
    }

    #[test]
    fn a_callers_timeout_wins() {
        assert_eq!(timeout_from(Some(5)), Duration::from_secs(5));
    }

    /// The bound is real, not merely configured: a provider that never answers
    /// gives up rather than holding the task forever, and says so.
    #[tokio::test(start_paused = true)]
    async fn a_call_past_the_timeout_gives_up() {
        let err = bounded(
            timeout_from(Some(1)),
            std::future::pending::<Result<(), String>>(),
        )
        .await
        .expect_err("a call that never answers must not be waited on forever");
        assert_eq!(err, "completion timed out after 1s");
    }

    /// A call that answers inside the bound is untouched by it.
    #[tokio::test(start_paused = true)]
    async fn a_call_inside_the_timeout_is_returned_verbatim() {
        let answer = bounded(timeout_from(None), async { Ok::<_, String>("a title") })
            .await
            .expect("an answered call");
        assert_eq!(answer, "a title");
    }
}
