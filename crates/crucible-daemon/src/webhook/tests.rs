//! Tests for the webhook signature scheme and secrets file.

use super::*;

const SECRET: &str = "correct-horse-battery-staple";
const NOW: i64 = 1_754_524_800;

fn store() -> WebhookSecrets {
    WebhookSecrets::new(HashMap::from([("ci".to_string(), SECRET.to_string())]))
}

/// The timestamped scheme, as it arrives from `X-Crucible-Signature`.
fn stamped(header: &str) -> Option<Signature<'_>> {
    Some(Signature::Timestamped(header))
}

// --- Wire format ---

#[test]
fn signature_covers_the_timestamp_and_the_raw_body() {
    // Pinned against an independent HMAC implementation (Python hmac) so a
    // change to the signed material breaks loudly instead of silently
    // agreeing with itself.
    assert_eq!(
        sign("secret-secret-secret", 1_700_000_000, br#"{"a":1}"#),
        "t=1700000000,v1=fa345d5b654e6667128855e14a4938c54b5b813699e6b72c5c4f501aa493ba92"
    );
}

#[test]
fn body_only_signature_covers_the_raw_body_alone() {
    // GitHub's `X-Hub-Signature-256`, pinned the same way — this is the
    // value GitHub computes for this secret and payload.
    assert_eq!(
        sign_body_only("secret-secret-secret", br#"{"a":1}"#),
        "sha256=82d8fbc14179b3a6195dbb654369004c798faa3831fa7ff308b1e2c8aa9498db"
    );
}

// --- Verification: the timestamped scheme ---

#[test]
fn correctly_signed_delivery_is_accepted() {
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);
    assert_eq!(store().verify_at("ci", stamped(&header), body, NOW), Ok(()));
}

#[test]
fn unsigned_delivery_is_refused() {
    let body = br#"{"event":"push"}"#;
    assert_eq!(
        store().verify_at("ci", None, body, NOW),
        Err(WebhookAuthError::MissingSignature)
    );
}

#[test]
fn wrong_signature_is_refused() {
    let body = br#"{"event":"push"}"#;
    let header = sign("not-the-configured-secret", NOW, body);
    assert_eq!(
        store().verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::BadSignature)
    );
}

#[test]
fn signature_from_another_body_is_refused() {
    // Capturing a valid header off one delivery must not authenticate a
    // different payload.
    let header = sign(SECRET, NOW, br#"{"event":"push"}"#);
    assert_eq!(
        store().verify_at("ci", stamped(&header), br#"{"event":"rm -rf"}"#, NOW),
        Err(WebhookAuthError::BadSignature)
    );
}

#[test]
fn signature_for_another_webhook_name_is_refused() {
    let store = WebhookSecrets::new(HashMap::from([
        ("ci".to_string(), SECRET.to_string()),
        (
            "deploy".to_string(),
            "another-long-enough-secret".to_string(),
        ),
    ]));
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);
    assert_eq!(
        store.verify_at("deploy", stamped(&header), body, NOW),
        Err(WebhookAuthError::BadSignature)
    );
}

#[test]
fn replayed_old_timestamp_is_refused() {
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW - TOLERANCE_SECS - 1, body);
    assert_eq!(
        store().verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::StaleTimestamp)
    );
}

#[test]
fn far_future_timestamp_is_refused() {
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW + TOLERANCE_SECS + 1, body);
    assert_eq!(
        store().verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::StaleTimestamp)
    );
}

#[test]
fn replayed_delivery_inside_the_window_is_refused() {
    // The tolerance window bounds replay; it does not stop it. A captured
    // delivery re-sent seconds later must still be refused.
    let store = store();
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);
    assert_eq!(store.verify_at("ci", stamped(&header), body, NOW), Ok(()));
    assert_eq!(
        store.verify_at("ci", stamped(&header), body, NOW + 5),
        Err(WebhookAuthError::Replayed)
    );
}

// --- Verification: the body-only (GitHub) scheme ---

#[test]
fn github_signed_delivery_is_accepted() {
    // The shape every off-the-shelf GitHub webhook sends. If this fails,
    // the endpoint cannot be used with GitHub at all.
    let body = br#"{"zen":"Non-blocking is better than blocking."}"#;
    let header = sign_body_only(SECRET, body);
    assert_eq!(
        store().verify_at("ci", Some(Signature::BodyOnly(&header)), body, NOW),
        Ok(())
    );
}

#[test]
fn github_signed_delivery_with_the_wrong_secret_is_refused() {
    let body = br#"{"zen":"Speak like a human."}"#;
    let header = sign_body_only("not-the-configured-secret", body);
    assert_eq!(
        store().verify_at("ci", Some(Signature::BodyOnly(&header)), body, NOW),
        Err(WebhookAuthError::BadSignature)
    );
}

#[test]
fn github_signed_delivery_from_another_body_is_refused() {
    let header = sign_body_only(SECRET, br#"{"action":"opened"}"#);
    assert_eq!(
        store().verify_at(
            "ci",
            Some(Signature::BodyOnly(&header)),
            br#"{"action":"deleted"}"#,
            NOW
        ),
        Err(WebhookAuthError::BadSignature)
    );
}

#[test]
fn replayed_github_delivery_inside_the_window_is_refused() {
    // Body-only signatures carry no timestamp, so the remembered-signature
    // set is the entire replay defence. It has to hold for the window.
    let store = store();
    let body = br#"{"action":"opened"}"#;
    let header = sign_body_only(SECRET, body);
    let signature = Some(Signature::BodyOnly(&header));
    assert_eq!(store.verify_at("ci", signature, body, NOW), Ok(()));
    assert_eq!(
        store.verify_at("ci", signature, body, NOW + TOLERANCE_SECS - 1),
        Err(WebhookAuthError::Replayed)
    );
}

#[test]
fn malformed_github_signatures_are_refused() {
    let body = br#"{"action":"opened"}"#;
    let valid = sign_body_only(SECRET, body);
    let tag = valid.strip_prefix("sha256=").unwrap().to_string();

    for header in [
        String::new(),
        tag.clone(),                      // no algorithm prefix
        format!("sha1={tag}"),            // a scheme we do not verify
        format!("sha256={}", &tag[..62]), // short tag
    ] {
        assert_eq!(
            store().verify_at("ci", Some(Signature::BodyOnly(&header)), body, NOW),
            Err(WebhookAuthError::MalformedSignature),
            "header {header:?} must be refused as malformed"
        );
    }
}

// --- Which header wins ---

#[test]
fn stripe_signature_header_carries_the_timestamped_scheme() {
    // Stripe's own header, byte-identical wire format: a Stripe endpoint
    // configured with a Crucible secret verifies unmodified.
    let body = br#"{"type":"charge.succeeded"}"#;
    let header = sign(SECRET, NOW, body);
    let signature = Signature::from_headers(|name| {
        (name == STRIPE_SIGNATURE_HEADER).then_some(header.as_str())
    });
    assert_eq!(signature, Some(Signature::Timestamped(&header)));
    assert_eq!(store().verify_at("ci", signature, body, NOW), Ok(()));
}

#[test]
fn timestamped_signature_is_never_downgraded_to_the_body_only_one() {
    // With both headers present the stronger scheme is the one checked —
    // otherwise a sender that can set either picks its own difficulty.
    let body = br#"{"event":"push"}"#;
    let timestamped = sign(SECRET, NOW, body);
    let body_only = sign_body_only(SECRET, body);
    let signature = Signature::from_headers(|name| match name {
        SIGNATURE_HEADER => Some(timestamped.as_str()),
        GITHUB_SIGNATURE_HEADER => Some(body_only.as_str()),
        _ => None,
    });
    assert_eq!(signature, Some(Signature::Timestamped(&timestamped)));
}

#[test]
fn a_delivery_with_no_signature_header_at_all_has_no_signature() {
    assert_eq!(Signature::from_headers(|_| None), None);
}

// --- Secrets ---

#[test]
fn webhook_without_a_configured_secret_is_refused() {
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);
    assert_eq!(
        store().verify_at("unknown", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
    // ...including when nothing at all is configured, which is the
    // out-of-the-box state.
    assert_eq!(
        WebhookSecrets::default().verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
}

#[test]
fn webhooks_sharing_a_secret_are_all_refused() {
    // Nothing in the signature names the webhook, so a shared secret makes
    // a delivery aimed at `ci` a valid delivery to `deploy`. Both entries
    // are dropped rather than left cross-signable.
    let store = WebhookSecrets::new(HashMap::from([
        ("ci".to_string(), SECRET.to_string()),
        ("deploy".to_string(), SECRET.to_string()),
        ("release".to_string(), "a-distinct-long-secret".to_string()),
    ]));
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);

    assert_eq!(
        store.verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
    assert_eq!(
        store.verify_at("deploy", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
    // The webhook with its own secret is unaffected.
    let release = sign("a-distinct-long-secret", NOW, body);
    assert_eq!(
        store.verify_at("release", stamped(&release), body, NOW),
        Ok(())
    );
}

#[test]
fn short_secret_is_dropped_rather_than_accepted() {
    let store = WebhookSecrets::new(HashMap::from([("ci".to_string(), "hunter2".to_string())]));
    let body = br#"{"event":"push"}"#;
    let header = sign("hunter2", NOW, body);
    assert_eq!(
        store.verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
}

// --- Header parsing ---

#[test]
fn malformed_signature_headers_are_refused() {
    let body = br#"{"event":"push"}"#;
    let valid = sign(SECRET, NOW, body);
    let tag = valid.split_once("v1=").unwrap().1.to_string();

    for header in [
        String::new(),
        "garbage".to_string(),
        format!("v1={tag}"),                   // no timestamp
        format!("t={NOW}"),                    // no signature
        format!("t=,v1={tag}"),                // empty timestamp
        format!("t={NOW},v1="),                // empty tag
        format!("t={NOW},v1={}", &tag[..62]),  // short tag
        format!("t={NOW},v1={tag}00"),         // long tag
        format!("t={NOW},v1=zz{}", &tag[2..]), // non-hex
        format!("t={NOW},v1={tag},v1={tag}"),  // duplicate tag
        format!("t={NOW},t={NOW},v1={tag}"),   // duplicate timestamp
        // GitHub's value in the timestamped header: no `t`, no `v1`, so
        // there is nothing here this scheme can check.
        format!("sha256={tag}"),
    ] {
        assert_eq!(
            store().verify_at("ci", stamped(&header), body, NOW),
            Err(WebhookAuthError::MalformedSignature),
            "header {header:?} must be refused as malformed"
        );
    }
}

#[test]
fn unknown_signature_fields_are_ignored() {
    // Stripe sends `v0=` alongside `v1=`, and any sender may add a field
    // we have never seen. Refusing what we do not understand makes the
    // scheme unimplementable by anyone but us.
    let body = br#"{"event":"push"}"#;
    let valid = sign(SECRET, NOW, body);
    let (timestamp, tag) = valid.split_once(",v1=").unwrap();
    for header in [
        format!("{timestamp},v0=deadbeef,v1={tag}"),
        format!("{timestamp},v1={tag},v0=deadbeef"),
        format!("{timestamp},v1={tag},scheme=whatever"),
    ] {
        assert_eq!(
            store().verify_at("ci", stamped(&header), body, NOW),
            Ok(()),
            "header {header:?} must be accepted"
        );
    }
}

#[test]
fn uppercase_hex_signature_is_accepted() {
    let body = br#"{"event":"push"}"#;
    let valid = sign(SECRET, NOW, body);
    let (timestamp, tag) = valid.split_once(",v1=").unwrap();
    let header = format!("{timestamp},v1={}", tag.to_uppercase());
    assert_eq!(store().verify_at("ci", stamped(&header), body, NOW), Ok(()));
}

#[test]
fn non_utf8_body_bytes_are_signed_verbatim() {
    // Verification works on bytes, never on a lossily-decoded string.
    let store = store();
    let body = [0xffu8, 0xfe, 0x00, 0x41];
    let header = sign(SECRET, NOW, &body);
    assert_eq!(store.verify_at("ci", stamped(&header), &body, NOW), Ok(()));
}

// --- Loading ---

#[test]
fn secrets_load_from_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(
        &path,
        format!("[webhooks.ci]\nsecret = \"{SECRET}\"\n[webhooks.short]\nsecret = \"tiny\"\n"),
    )
    .unwrap();

    let store = WebhookSecrets::load(Some(&path));
    let body = br#"{"event":"push"}"#;
    let ci = sign(SECRET, NOW, body);
    let short = sign("tiny", NOW, body);
    assert_eq!(store.verify_at("ci", stamped(&ci), body, NOW), Ok(()));
    assert_eq!(
        store.verify_at("short", stamped(&short), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
}

#[test]
fn unreadable_or_malformed_secrets_file_closes_the_ingress() {
    let dir = tempfile::tempdir().unwrap();
    let body = br#"{"event":"push"}"#;
    let header = sign(SECRET, NOW, body);

    let missing = dir.path().join("absent.toml");
    assert_eq!(
        WebhookSecrets::load(Some(&missing)).verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );

    let malformed = dir.path().join("bad.toml");
    std::fs::write(&malformed, "this is not toml [[[").unwrap();
    assert_eq!(
        WebhookSecrets::load(Some(&malformed)).verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );

    assert_eq!(
        WebhookSecrets::load(None).verify_at("ci", stamped(&header), body, NOW),
        Err(WebhookAuthError::NoSecret)
    );
}

// --- Minting ---

#[test]
fn minted_secret_is_usable_immediately() {
    // The whole point: an operator with no `webhooks.toml` runs one command
    // and the endpoint is open to a sender holding the printed secret.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("webhooks.toml");

    let secret = mint_secret(&path, "ci", false).unwrap();
    assert!(secret.len() >= MIN_SECRET_LEN, "{secret:?}");

    let store = WebhookSecrets::load(Some(&path));
    let body = br#"{"event":"push"}"#;
    let header = sign(&secret, NOW, body);
    assert_eq!(store.verify_at("ci", stamped(&header), body, NOW), Ok(()));
}

#[cfg(unix)]
#[test]
fn minted_secrets_file_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    mint_secret(&path, "ci", false).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "{mode:o}");
}

#[test]
fn minting_twice_needs_rotate_and_retires_the_old_secret() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    let body = br#"{"event":"push"}"#;

    let first = mint_secret(&path, "ci", false).unwrap();
    // Without `--rotate` an existing secret is never silently replaced.
    assert!(mint_secret(&path, "ci", false).is_err());
    assert_eq!(
        WebhookSecrets::load(Some(&path)).verify_at(
            "ci",
            stamped(&sign(&first, NOW, body)),
            body,
            NOW
        ),
        Ok(())
    );

    let second = mint_secret(&path, "ci", true).unwrap();
    assert_ne!(first, second);
    let store = WebhookSecrets::load(Some(&path));
    assert_eq!(
        store.verify_at("ci", stamped(&sign(&first, NOW, body)), body, NOW),
        Err(WebhookAuthError::BadSignature)
    );
    assert_eq!(
        store.verify_at("ci", stamped(&sign(&second, NOW, body)), body, NOW),
        Ok(())
    );
}

#[test]
fn minting_keeps_the_other_webhooks_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, format!("[webhooks.ci]\nsecret = \"{SECRET}\"\n")).unwrap();

    mint_secret(&path, "deploy", false).unwrap();

    let store = WebhookSecrets::load(Some(&path));
    let body = br#"{"event":"push"}"#;
    assert_eq!(
        store.verify_at("ci", stamped(&sign(SECRET, NOW, body)), body, NOW),
        Ok(())
    );
}

#[test]
fn minting_refuses_a_name_that_is_not_a_url_path_segment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    for name in ["", "../../etc/passwd", "ci/deploy", "ci deploy", "ci\"]"] {
        assert!(
            mint_secret(&path, name, false).is_err(),
            "{name:?} must be refused"
        );
    }
    assert!(!path.exists(), "a refused name must not create the file");
}

#[test]
fn minting_never_overwrites_a_file_it_cannot_parse() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, "this is not toml [[[").unwrap();

    assert!(mint_secret(&path, "ci", true).is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "this is not toml [[["
    );
}
