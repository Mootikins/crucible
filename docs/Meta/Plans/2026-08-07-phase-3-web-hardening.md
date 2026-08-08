---
title: Phase 3 Execution — cru web Hardening
description: Task-by-task execution guide for §3.11 of the public launch readiness plan.
tags:
  - meta
  - plan
  - security
status: active
created: 2026-08-07
---

# Phase 3 Execution — `cru web` Hardening (§3.11)

> Scope: §3.11 only. The rest of Phase 3 (plugin supply chain, kiln-content
> execution, permission-rule defects) is not started. All findings below were
> re-verified against `35460f0fe` on 2026-08-07.

**Framing correction.** The plan treats §3.11 as conditional on a VPS running
`cru web`. It is not conditional: `cru web` binds a real port on whatever machine
runs it, and two findings are reachable from any page the operator visits while
it is up (DNS rebinding, webhook CSRF). Deployment topology changes the blast
radius, not whether the defects exist.

## Verified state

| Finding | Evidence |
|---|---|
| No security headers | `crucible-web/Cargo.toml:24` — tower-http has `["fs","cors"]`, no `set-header` |
| `/api/file/raw` sniffable | `routes/kiln.rs` returns `mime_guess` content type only; no `nosniff`, no `Content-Disposition` |
| Self-service root allowlist | `routes/project.rs:13,24` → `daemon/project_manager.rs:32`; only rejects a dir named `.crucible` |
| DNS rebinding | `middleware/auth.rs:271` `origin_matches_host` compares Origin to the request's own Host — a domain resolving to 127.0.0.1 satisfies both |
| SSRF | `routes/session.rs` — hostnames skip the IP check entirely; `is_private && !ip.is_loopback()` explicitly permits loopback; IPv6 arm omits unique-local, link-local, and IPv4-mapped |
| Cookie is the API key | `routes/auth.rs:52` sets `AUTH_COOKIE={req.key}` verbatim, 30-day, no `Secure` |
| Webhook CSRF | `routes/webhook.rs` — no signature, `body: String`, so `text/plain` cross-origin `fetch` is a simple request with no preflight |
| Style attrs, no CSP | `web/src/lib/markdown.ts` allows `style`; a chat turn can overlay the viewport |

## Task groups

Partitioned by file. No two groups write the same file.

### W1 — Security headers and raw-file responses
**Owns:** `crucible-web/Cargo.toml`, `src/server.rs`, `src/routes/kiln.rs`

CSP, `X-Content-Type-Options: nosniff`, `frame-ancestors`, `Referrer-Policy`.
`/api/file/raw` must never return a document type the browser will execute on
the app origin — force a non-rendering content type plus
`Content-Disposition: attachment` for anything that is not a known-safe media
type. A CSP alone breaks the X1 chain; do both.

### W2 — Root containment
**Owns:** `src/routes/project.rs`, `crucible-daemon/src/project_manager.rs`

Registration is unauthenticated and accepts `/`. `scm.clone` already has the
right model (`scm.rs` contains to a configured base) — apply it here rather than
inventing one.

### W3 — Host validation and session token
**Owns:** `src/middleware/auth.rs`, `src/routes/auth.rs`

Validate `Host` against an expected-authority allowlist so a rebound domain
fails before the loopback bypass applies. Replace the cookie-is-the-key design
with a derived, revocable, expiring token.

### W4 — SSRF
**Owns:** `src/routes/session.rs`

Resolve hostnames before deciding; block unique-local, link-local, IPv4-mapped
IPv6, and the cloud metadata addresses. Loopback stays allowed only when the
bind is loopback.

### W5 — Webhook authentication
**Owns:** `src/routes/webhook.rs` and the daemon's `webhook_receive` path

Per-webhook HMAC over the raw body, constant-time compare, and a content-type
requirement that forces a CORS preflight.

### W6 — Frontend
**Owns:** `web/src/lib/markdown.ts`, `web/vite.config.ts`

Drop `style` from the allowlist (keep `class`). Narrow the service worker scope
so an XSS cannot install a root-scoped worker.

## Approval criteria

1. A file written into a kiln with an `.html` extension, fetched via
   `/api/file/raw`, does not execute on the app origin.
2. `POST /api/project/register {"path":"/"}` is refused.
3. A request with `Host` set to a rebound domain fails auth.
4. `validate_endpoint` rejects `[::ffff:169.254.169.254]` and a hostname
   resolving to a private address.
5. An unsigned webhook POST is refused; a cross-origin `text/plain` POST is
   preflighted.
6. The session cookie value is not the API key, and rotating invalidates it.
7. `just ci` green.
