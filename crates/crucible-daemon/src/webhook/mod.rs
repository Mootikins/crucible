//! Sender authentication for the webhook ingress (`POST /api/webhook/{name}`).
//!
//! The ingress is an HTTP endpoint that injects events into plugin streams, so
//! it needs a sender credential of its own: the web server's bearer auth waves
//! loopback callers through, which means any page the operator visits can reach
//! it cross-origin. Enforcement lives at the HTTP edge (`crucible-web`'s
//! `routes/webhook.rs`) because that is the only place the *raw* request bytes
//! exist; this module holds the scheme so the edge has exactly one thing to
//! call.
//!
//! # The two shapes a delivery may carry
//!
//! **Timestamped** — Stripe's wire format, in Stripe's header or ours, so a
//! Stripe endpoint verifies against its own `whsec_…` with nothing bespoke in
//! between:
//!
//! ```text
//! X-Crucible-Signature: t=1754524800,v1=<hex sha256 hmac>
//! Stripe-Signature:     t=1754524800,v1=<hex sha256 hmac>
//! ```
//!
//! The signed material is `<t> "." <raw body bytes>`. Folding the timestamp
//! into the signature and bounding it with [`TOLERANCE_SECS`] is what buys
//! replay resistance, which is why this is the shape to prefer.
//!
//! Fields other than `t` and `v1` are ignored rather than refused — Stripe
//! itself sends `v0=` alongside `v1=`, and a verifier that rejects every field
//! it has not seen before cannot survive its own senders adding one.
//!
//! **Body-only** — GitHub's, so an off-the-shelf GitHub webhook works as
//! shipped:
//!
//! ```text
//! X-Hub-Signature-256: sha256=<hex sha256 hmac>
//! ```
//!
//! GitHub signs the body alone, so nothing in the signature says *when* it was
//! sent. A captured delivery is refused for as long as we remember it — the
//! same [`TOLERANCE_SECS`] window — and is replayable after that. That is
//! GitHub's scheme rather than a weakening of ours, but it is the reason a
//! sender that can send the timestamped shape should.
//!
//! # Configuring it
//!
//! Secrets live in `~/.config/crucible/webhooks.toml`
//! ([`default_secrets_path`]), one per webhook:
//!
//! ```toml
//! [webhooks.ci]
//! secret = "…"
//! ```
//!
//! [`mint_secret`] generates one and writes it there (0600); a webhook with no
//! secret refuses every delivery.

use anyhow::{bail, Context};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Crucible's own header for the timestamped scheme. Lowercase:
/// `http::HeaderMap` normalises names, and this is compared against
/// already-normalised keys.
pub const SIGNATURE_HEADER: &str = "x-crucible-signature";

/// Stripe's header. Same wire format and same signed material as
/// [`SIGNATURE_HEADER`] — the scheme was copied from Stripe, so honouring the
/// header costs nothing and makes a real Stripe endpoint work unmodified.
pub const STRIPE_SIGNATURE_HEADER: &str = "stripe-signature";

/// GitHub's header for the body-only scheme.
pub const GITHUB_SIGNATURE_HEADER: &str = "x-hub-signature-256";

/// Every header a signature may arrive in, strongest scheme first. The HTTP
/// edge redacts all of them before a delivery reaches a plugin's inbox.
pub const SIGNATURE_HEADERS: [&str; 3] = [
    SIGNATURE_HEADER,
    STRIPE_SIGNATURE_HEADER,
    GITHUB_SIGNATURE_HEADER,
];

/// How far the signed timestamp may sit from our clock, in either direction.
/// Stripe uses the same 5 minutes; skew tolerance has to be symmetric or a
/// sender whose clock runs fast is permanently rejected.
///
/// Doubles as how long a verified signature is remembered for replay
/// detection, which is the *only* replay bound a body-only delivery gets.
pub const TOLERANCE_SECS: i64 = 300;

/// Secrets shorter than this are refused at load. With a captured signed
/// delivery in hand an attacker can brute-force a short secret offline, and a
/// webhook secret is not a password a human needs to type.
pub const MIN_SECRET_LEN: usize = 16;

/// Cap on remembered signatures. Only signatures that already verified are
/// remembered, so this can't be grown by an unauthenticated caller; it just
/// keeps a chatty legitimate sender from growing the set without bound.
const MAX_REMEMBERED: usize = 4096;

/// Why a webhook delivery was refused.
///
/// Deliberately *not* surfaced to the caller verbatim — the HTTP edge collapses
/// every variant into one 401 so the endpoint is not an oracle for which
/// webhook names exist. It is carried separately for the server log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WebhookAuthError {
    #[error("no secret is configured for this webhook")]
    NoSecret,
    #[error("missing signature header")]
    MissingSignature,
    #[error("malformed signature header")]
    MalformedSignature,
    #[error("signature timestamp is outside the accepted window")]
    StaleTimestamp,
    #[error("signature does not match")]
    BadSignature,
    #[error("signature has already been used")]
    Replayed,
}

/// The signature a delivery presented, and therefore which scheme verifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signature<'a> {
    /// `t=<unix>,v1=<hex>`: signs `<t> "." <body>`. Replay-resistant.
    Timestamped(&'a str),
    /// `sha256=<hex>`: signs the body alone. Replay-bounded only by how long
    /// the tag stays remembered.
    BodyOnly(&'a str),
}

impl<'a> Signature<'a> {
    /// Pick the signature to verify from a delivery's headers.
    ///
    /// Strongest scheme first, and a *present* timestamped header is never
    /// downgraded to the body-only one: otherwise a caller who could supply
    /// either would get to choose the weaker check.
    pub fn from_headers(get: impl Fn(&str) -> Option<&'a str>) -> Option<Self> {
        if let Some(header) = get(SIGNATURE_HEADER).or_else(|| get(STRIPE_SIGNATURE_HEADER)) {
            return Some(Self::Timestamped(header));
        }
        get(GITHUB_SIGNATURE_HEADER).map(Self::BodyOnly)
    }
}

/// Per-webhook secrets plus the short-term memory that stops replay.
#[derive(Debug, Default)]
pub struct WebhookSecrets {
    secrets: HashMap<String, String>,
    /// `(unix timestamp, signature)` of every delivery accepted inside the
    /// current tolerance window.
    seen: Mutex<Vec<(i64, [u8; 32])>>,
}

#[derive(Debug, Default, Deserialize)]
struct WebhookSecretsFile {
    #[serde(default)]
    webhooks: HashMap<String, SecretEntry>,
}

#[derive(Debug, Deserialize)]
struct SecretEntry {
    secret: String,
}

/// Default location of the secrets file, next to the web API key
/// (`~/.config/crucible/webhooks.toml`).
pub fn default_secrets_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("crucible").join("webhooks.toml"))
}

impl WebhookSecrets {
    /// Build from an explicit `name -> secret` map. Unusable entries are
    /// dropped rather than accepted quietly:
    ///
    /// - a secret shorter than [`MIN_SECRET_LEN`];
    /// - a secret shared by more than one webhook. Nothing in either signature
    ///   names the webhook (Stripe's material is unambiguous precisely because
    ///   it has no variable-length name in it), so two webhooks holding the
    ///   same secret are the same webhook: a delivery captured off one
    ///   authenticates the other. Refusing the collision is the fail-closed
    ///   reading.
    pub fn new(secrets: HashMap<String, String>) -> Self {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for secret in secrets.values() {
            *counts.entry(secret.as_str()).or_default() += 1;
        }

        let secrets: HashMap<String, String> = secrets
            .iter()
            .filter(|(name, secret)| {
                if secret.len() < MIN_SECRET_LEN {
                    tracing::warn!(
                        webhook = %name,
                        "Ignoring webhook secret shorter than {MIN_SECRET_LEN} bytes — deliveries for this webhook will be refused"
                    );
                    return false;
                }
                if counts.get(secret.as_str()).copied().unwrap_or(0) > 1 {
                    tracing::warn!(
                        webhook = %name,
                        "Ignoring webhook secret shared with another webhook — give each webhook its own secret"
                    );
                    return false;
                }
                true
            })
            .map(|(name, secret)| (name.clone(), secret.clone()))
            .collect();

        Self {
            secrets,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Load `[webhooks.<name>] secret = "..."` entries from a TOML file.
    ///
    /// A missing or unparseable file yields an empty set, which refuses every
    /// delivery. Failing to read the secrets must never mean "let it through".
    pub fn load(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(path = %path.display(), "No webhook secrets file; webhook ingress is closed");
                return Self::default();
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "Cannot read webhook secrets; webhook ingress stays closed");
                return Self::default();
            }
        };
        match toml::from_str::<WebhookSecretsFile>(&raw) {
            Ok(file) => Self::new(
                file.webhooks
                    .into_iter()
                    .map(|(name, entry)| (name, entry.secret))
                    .collect(),
            ),
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "Malformed webhook secrets file; webhook ingress stays closed");
                Self::default()
            }
        }
    }

    /// Verify a delivery against the current clock. See [`Self::verify_at`].
    pub fn verify(
        &self,
        name: &str,
        signature: Option<Signature<'_>>,
        raw_body: &[u8],
    ) -> Result<(), WebhookAuthError> {
        self.verify_at(name, signature, raw_body, unix_now())
    }

    /// Verify a delivery as of `now_unix`.
    ///
    /// `raw_body` must be the bytes exactly as received — signing a re-encoded
    /// form (pretty-printed JSON, a reserialized `Value`) would verify a
    /// different document than the one the handler goes on to process.
    pub fn verify_at(
        &self,
        name: &str,
        signature: Option<Signature<'_>>,
        raw_body: &[u8],
        now_unix: i64,
    ) -> Result<(), WebhookAuthError> {
        let secret = self.secrets.get(name).ok_or(WebhookAuthError::NoSecret)?;

        let (sent_at, provided) = match signature.ok_or(WebhookAuthError::MissingSignature)? {
            Signature::Timestamped(header) => {
                let (timestamp, provided) = parse_timestamped(header)?;

                // Parsed for the window check, but the *literal* text is what
                // gets signed — re-formatting `t` (`007` -> `7`) would hash
                // different bytes than the sender hashed.
                let sent_at: i64 = timestamp
                    .parse()
                    .map_err(|_| WebhookAuthError::MalformedSignature)?;
                if now_unix.saturating_sub(sent_at).saturating_abs() > TOLERANCE_SECS {
                    return Err(WebhookAuthError::StaleTimestamp);
                }

                verify_tag(secret, &signed_material(timestamp, raw_body), &provided)?;
                (sent_at, provided)
            }
            Signature::BodyOnly(header) => {
                let provided = parse_body_only(header)?;
                verify_tag(secret, raw_body, &provided)?;
                // The signature carries no timestamp, so the delivery is only
                // as fresh as the moment it arrived: remember it from now, and
                // it stays un-replayable exactly as long as we remember it.
                (now_unix, provided)
            }
        };

        self.claim(sent_at, provided, now_unix)
    }

    /// Remember a verified signature so the same delivery cannot be replayed
    /// inside the tolerance window (the window alone only bounds replay, it
    /// does not prevent it).
    fn claim(
        &self,
        sent_at: i64,
        signature: [u8; 32],
        now_unix: i64,
    ) -> Result<(), WebhookAuthError> {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        // Anything outside the window is already refused by the clock check.
        seen.retain(|(ts, _)| now_unix.saturating_sub(*ts).saturating_abs() <= TOLERANCE_SECS);
        if seen.iter().any(|(_, sig)| *sig == signature) {
            return Err(WebhookAuthError::Replayed);
        }
        if seen.len() >= MAX_REMEMBERED {
            seen.remove(0);
        }
        seen.push((sent_at, signature));
        Ok(())
    }
}

/// Build the timestamped signature value (`X-Crucible-Signature`,
/// `Stripe-Signature`) a sender must send. Exposed so senders (and tests) share
/// one definition of the scheme with the verifier.
pub fn sign(secret: &str, timestamp: i64, raw_body: &[u8]) -> String {
    let timestamp = timestamp.to_string();
    let material = signed_material(&timestamp, raw_body);
    format!("t={timestamp},v1={}", hex::encode(tag(secret, &material)))
}

/// Build the body-only signature value (`X-Hub-Signature-256`) GitHub would
/// send for `raw_body`.
pub fn sign_body_only(secret: &str, raw_body: &[u8]) -> String {
    format!("sha256={}", hex::encode(tag(secret, raw_body)))
}

/// Mint a fresh secret for `name` and write it into the secrets file at `path`,
/// creating the file 0600. Returns the secret so the caller can print it once:
/// nothing recovers it afterwards except reading the file.
///
/// Refuses to replace an existing entry unless `rotate` — rotating breaks every
/// sender still configured with the old value, so it has to be asked for.
///
/// The file is rewritten from its parsed form, so any comments in it are lost;
/// a file that does not parse is an error rather than something to overwrite.
pub fn mint_secret(path: &Path, name: &str, rotate: bool) -> anyhow::Result<String> {
    // The name is a URL path segment and a TOML key. Keeping it to an
    // unambiguous alphabet means neither has to be escaped, and a delivery URL
    // matches the config entry by simple equality.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("webhook name must be ASCII letters, digits, `-` or `_`, got {name:?}");
    }

    let mut doc: toml::Table = match std::fs::read_to_string(path) {
        Ok(raw) => raw.parse().with_context(|| {
            format!(
                "{} is not valid TOML — fix or remove it before minting a secret",
                path.display()
            )
        })?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("reading {}", path.display()));
        }
    };

    let webhooks = doc
        .entry("webhooks".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .with_context(|| format!("`webhooks` in {} is not a table", path.display()))?;

    if webhooks.contains_key(name) && !rotate {
        bail!(
            "webhook `{name}` already has a secret in {}; pass --rotate to replace it \
             (every sender using the old secret will stop working)",
            path.display()
        );
    }

    let secret = generate_secret();
    let mut entry = toml::Table::new();
    entry.insert("secret".to_string(), toml::Value::String(secret.clone()));
    webhooks.insert(name.to_string(), toml::Value::Table(entry));

    write_private(path, &toml::to_string(&doc)?)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(secret)
}

/// 256 bits of randomness, hex-encoded: far past [`MIN_SECRET_LEN`], and safe
/// to paste into any sender's secret field.
fn generate_secret() -> String {
    use rand::Rng;
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    hex::encode(raw)
}

/// Write a credential file readable only by its owner, including when it
/// already exists with looser permissions.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn signed_material(timestamp: &str, raw_body: &[u8]) -> Vec<u8> {
    let mut material = Vec::with_capacity(timestamp.len() + 1 + raw_body.len());
    material.extend_from_slice(timestamp.as_bytes());
    material.push(b'.');
    material.extend_from_slice(raw_body);
    material
}

fn keyed_mac(secret: &str, message: &[u8]) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts a key of any length");
    mac.update(message);
    mac
}

fn tag(secret: &str, message: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&keyed_mac(secret, message).finalize().into_bytes());
    out
}

/// `verify_slice` is the `hmac` crate's own constant-time comparison, so there
/// is no hand-written tag compare here to get wrong.
fn verify_tag(secret: &str, message: &[u8], provided: &[u8; 32]) -> Result<(), WebhookAuthError> {
    keyed_mac(secret, message)
        .verify_slice(provided)
        .map_err(|_| WebhookAuthError::BadSignature)
}

/// Parse `t=<digits>,v1=<64 hex>` into the literal timestamp text and the
/// 32-byte tag.
///
/// Fields that are not `t` or `v1` are ignored: Stripe sends `v0=` next to
/// `v1=`, and a sender that adds a field must not thereby become unable to
/// deliver. Duplicate `t`/`v1` keys are still refused rather than resolved — a
/// verifier that picks one of two candidate signatures is a verifier an
/// attacker gets to aim.
fn parse_timestamped(header: &str) -> Result<(&str, [u8; 32]), WebhookAuthError> {
    let mut timestamp: Option<&str> = None;
    let mut tag: Option<&str> = None;

    for field in header.split(',') {
        let Some((key, value)) = field.trim().split_once('=') else {
            continue;
        };
        let slot = match key {
            "t" => &mut timestamp,
            "v1" => &mut tag,
            _ => continue,
        };
        if slot.is_some() {
            return Err(WebhookAuthError::MalformedSignature);
        }
        *slot = Some(value);
    }

    let (timestamp, tag) = (
        timestamp.ok_or(WebhookAuthError::MalformedSignature)?,
        tag.ok_or(WebhookAuthError::MalformedSignature)?,
    );
    if timestamp.is_empty() {
        return Err(WebhookAuthError::MalformedSignature);
    }
    Ok((timestamp, decode_tag(tag)?))
}

/// Parse GitHub's `sha256=<64 hex>`. The algorithm prefix is required: an
/// unprefixed or `sha1=` value is a different (or unknown) scheme, not this
/// one.
fn parse_body_only(header: &str) -> Result<[u8; 32], WebhookAuthError> {
    let tag = header
        .trim()
        .strip_prefix("sha256=")
        .ok_or(WebhookAuthError::MalformedSignature)?;
    decode_tag(tag)
}

fn decode_tag(hex_tag: &str) -> Result<[u8; 32], WebhookAuthError> {
    let mut decoded = [0u8; 32];
    hex::decode_to_slice(hex_tag, &mut decoded)
        .map_err(|_| WebhookAuthError::MalformedSignature)?;
    Ok(decoded)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
