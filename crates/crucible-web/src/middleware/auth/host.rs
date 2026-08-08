//! Host validation — the DNS-rebinding defence.
//!
//! Everything here answers one question: *is this request addressed to an
//! authority this server actually answers to?* The rest of `auth` leans on the
//! answer, so the parsing is deliberately strict and fails closed.

use axum::http::{header, Request};
use crucible_core::config::WebConfig;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Marker inserted by [`enforce_host`](super::enforce_host) once the request's
/// authority has been checked against the server's [`HostPolicy`].
///
/// Downstream layers may only treat the `Host` header as trustworthy when this
/// is present. Without it, `Host` is just attacker-supplied text: comparing it
/// to another header from the same request (as the WebSocket Origin guard used
/// to) proves nothing at all.
#[derive(Clone, Copy)]
pub struct HostVerified;

/// The set of authorities this server answers to.
///
/// A browser puts the authority it navigated to in `Host`; it cannot be
/// overridden by page script. So an attacker page on `evil.test` — even one
/// whose DNS record points at 127.0.0.1 (rebinding) — is stuck sending
/// `Host: evil.test`. Refusing every authority that is not one of ours is
/// therefore what makes the loopback bypass and the same-origin shortcut sound.
#[derive(Clone, Debug)]
pub struct HostPolicy {
    /// Canonical `host[:port]` authorities that are accepted verbatim.
    allowed: BTreeSet<String>,
    /// Accept *any* IP-literal host on [`port`](Self::port). Set for wildcard
    /// binds (`0.0.0.0` / `::`), whose reachable LAN addresses cannot be
    /// enumerated up front. Safe because an IP literal in `Host` means the
    /// browser navigated straight to that address — rebinding needs a *name*.
    any_ip_literal: bool,
    /// The bind port, used for the IP-literal rule above.
    port: u16,
}

impl HostPolicy {
    /// Derive the policy from the web server's own configuration.
    ///
    /// Takes the config whole rather than the three fields separately, and is
    /// what the server actually calls: `[web] allowed_hosts` is the only
    /// escape hatch for reverse-proxy and tunnel deployments, and an argument
    /// a caller has to remember to forward is one that will eventually be
    /// forwarded as `&[]` — leaving a documented control with no input.
    pub fn from_web_config(config: &WebConfig) -> Self {
        Self::from_bind(&config.host, config.port, &config.allowed_hosts)
    }

    /// Derive the policy from the bind address, plus any operator-configured
    /// public names (reverse proxy / `cru tunnel`).
    ///
    /// Always accepted: `localhost`, `127.0.0.1` and `[::1]` on `port` — the
    /// three spellings of the same loopback the operator actually bound.
    /// A configured entry that carries its own port is matched exactly; one
    /// without a port is accepted both bare (a proxy on 80/443 forwards the
    /// public name with no port) and with `port` appended.
    pub fn from_bind(bind_host: &str, port: u16, extra_hosts: &[String]) -> Self {
        let mut allowed = BTreeSet::new();
        let mut allow = |authority: &str| {
            match normalize_authority(authority) {
                Some(canonical) => {
                    allowed.insert(canonical);
                }
                None => tracing::warn!(authority, "Ignoring unparseable allowed host"),
            };
        };

        for loopback in ["localhost", "127.0.0.1", "[::1]"] {
            allow(&format!("{loopback}:{port}"));
        }

        let bind_ip = parse_bind_ip(bind_host);
        let any_ip_literal = bind_ip.is_some_and(|ip| ip.is_unspecified());
        match bind_ip {
            Some(ip) if !ip.is_unspecified() => allow(&format_authority(ip, port)),
            // A wildcard bind has no single address to name; the IP-literal
            // rule below covers it.
            Some(_) => {}
            // The operator bound by name — answer to that name.
            None => allow(&format!("{bind_host}:{port}")),
        }

        for entry in extra_hosts
            .iter()
            .map(|h| h.trim())
            .filter(|h| !h.is_empty())
        {
            allow(entry);
            if normalize_authority(entry).is_some_and(|a| split_canonical(&a).1.is_none()) {
                allow(&format!("{entry}:{port}"));
            }
        }

        Self {
            allowed,
            any_ip_literal,
            port,
        }
    }

    /// Every canonical `host[:port]` authority accepted verbatim.
    ///
    /// Excludes the open-ended "any IP literal on the bind port" rule a
    /// wildcard bind adds — that set is unbounded and cannot be enumerated.
    /// Exists so the CORS allow-list derives from the same source that decides
    /// `Host`: they were built independently, and a configured `allowed_hosts`
    /// entry passed the Host check while the CSP still blocked the terminal
    /// because `connect-src` never learned about it.
    pub fn allowed_authorities(&self) -> impl Iterator<Item = &str> {
        self.allowed.iter().map(String::as_str)
    }

    /// Whether the request is addressed to an authority we answer to.
    /// Fails closed: an absent, malformed, duplicated or self-contradicting
    /// authority is refused rather than guessed at.
    pub fn accepts<B>(&self, request: &Request<B>) -> bool {
        request_authority(request).is_some_and(|a| self.accepts_authority(&a))
    }

    fn accepts_authority(&self, canonical: &str) -> bool {
        if self.allowed.contains(canonical) {
            return true;
        }
        if !self.any_ip_literal {
            return false;
        }
        match split_canonical(canonical) {
            (host, Some(port)) => port == self.port && is_ip_literal(host),
            (_, None) => false,
        }
    }
}

/// The authority a request is addressed to, canonicalized.
///
/// HTTP/1.1 puts it in `Host`; HTTP/2 puts `:authority` on the URI (hyper
/// leaves the `Host` header empty there), and an HTTP/1.1 absolute-form
/// request line sets both. Two Host headers, or a Host that disagrees with the
/// request target, are request-smuggling shapes — refuse instead of picking a
/// winner, because the layer behind us may pick the other one.
pub(super) fn request_authority<B>(request: &Request<B>) -> Option<String> {
    let mut host_headers = request.headers().get_all(header::HOST).iter();
    let host = match (host_headers.next(), host_headers.next()) {
        (_, Some(_)) => return None,
        (Some(h), None) => Some(normalize_authority(h.to_str().ok()?)?),
        (None, None) => None,
    };
    let target = match request.uri().authority() {
        Some(a) => Some(normalize_authority(a.as_str())?),
        None => None,
    };
    match (host, target) {
        (Some(h), Some(t)) if h != t => None,
        (Some(a), _) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

/// Canonicalize a bare `host[:port]` authority, or `None` if it is not one.
///
/// Lowercases, strips a single trailing dot from the host (`evil.test.` and
/// `evil.test` are the same name to a resolver, so they must be the same
/// string here), re-renders IP literals in canonical form (`[0:0:0:0:0:0:0:1]`
/// → `[::1]`) and re-renders the port numerically (`:03000` → `:3000`).
/// Anything carrying userinfo, a path, a scheme, whitespace, non-ASCII, or an
/// unbracketed IPv6 address is rejected outright.
pub(super) fn normalize_authority(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 255 || raw.bytes().any(|b| !b.is_ascii_graphic()) {
        return None;
    }
    if raw.contains(['@', '/', '\\', '?', '#', '%']) {
        return None;
    }
    let lower = raw.to_ascii_lowercase();

    let (host, port) = if let Some(rest) = lower.strip_prefix('[') {
        let (inner, after) = rest.split_once(']')?;
        let addr: Ipv6Addr = inner.parse().ok()?;
        (format!("[{addr}]"), parse_port_suffix(after)?)
    } else {
        let (host, port) = match lower.split_once(':') {
            // More than one colon and no brackets: an unbracketed IPv6
            // literal, which is not a legal HTTP authority.
            Some((_, rest)) if rest.contains(':') => return None,
            Some((host, port)) => (host, Some(port.parse::<u16>().ok()?)),
            None => (lower.as_str(), None),
        };
        // One trailing dot is the fully-qualified spelling of the same name;
        // anything else dot-shaped (`evil.test..`, `.evil.test`) is not a name
        // a browser would produce, and neither are characters outside the
        // hostname charset.
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty()
            || host.starts_with('.')
            || host.ends_with('.')
            || host.contains("..")
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        {
            return None;
        }
        match host.parse::<Ipv4Addr>() {
            Ok(ip) => (ip.to_string(), port),
            Err(_) => (host.to_string(), port),
        }
    };

    if host.is_empty() || port == Some(0) {
        return None;
    }
    Some(match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// `""` → no port, `":3000"` → port. Anything else is malformed.
fn parse_port_suffix(after_bracket: &str) -> Option<Option<u16>> {
    if after_bracket.is_empty() {
        return Some(None);
    }
    after_bracket
        .strip_prefix(':')
        .and_then(|p| p.parse::<u16>().ok())
        .map(Some)
}

/// Split a *canonical* authority (as produced by [`normalize_authority`]).
fn split_canonical(canonical: &str) -> (&str, Option<u16>) {
    let sep = match canonical.rfind(']') {
        Some(bracket) => canonical[bracket..].find(':').map(|i| bracket + i),
        None => canonical.rfind(':'),
    };
    match sep {
        Some(i) => (&canonical[..i], canonical[i + 1..].parse().ok()),
        None => (canonical, None),
    }
}

fn is_ip_literal(host: &str) -> bool {
    host.starts_with('[') || host.parse::<Ipv4Addr>().is_ok()
}

fn parse_bind_ip(bind_host: &str) -> Option<IpAddr> {
    bind_host.trim_matches(['[', ']']).parse::<IpAddr>().ok()
}

fn format_authority(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("{v4}:{port}"),
        IpAddr::V6(v6) => format!("[{v6}]:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_bind_accepts_lan_ips_but_never_a_name() {
        // `--host 0.0.0.0` is the LAN case: the machine's own addresses cannot
        // be enumerated, and an IP-literal Host cannot come from rebinding
        // (that needs a name). Names still have to be allow-listed.
        let policy = HostPolicy::from_bind("0.0.0.0", 3000, &[]);
        assert!(policy.accepts_authority("192.168.0.16:3000"));
        assert!(policy.accepts_authority("[fd00::1]:3000"));
        assert!(!policy.accepts_authority("192.168.0.16:3001"));
        assert!(!policy.accepts_authority("evil.test:3000"));
        assert!(!policy.accepts_authority("nas.local:3000"));

        // A specific bind does not get the IP-literal allowance.
        let loopback = HostPolicy::from_bind("127.0.0.1", 3000, &[]);
        assert!(!loopback.accepts_authority("192.168.0.16:3000"));
    }

    #[test]
    fn configured_public_names_are_accepted_bare_and_on_the_bind_port() {
        // `cru tunnel` / reverse proxy: the browser's Host is the public name,
        // with no port when the proxy terminates on 80 or 443.
        let policy = HostPolicy::from_bind(
            "127.0.0.1",
            3000,
            &[
                "crucible.example.com".to_string(),
                "old.example:8443".into(),
            ],
        );
        assert!(policy.accepts_authority("crucible.example.com"));
        assert!(policy.accepts_authority("crucible.example.com:3000"));
        assert!(policy.accepts_authority("old.example:8443"));
        // A configured entry that names its own port is exact.
        assert!(!policy.accepts_authority("old.example"));
        assert!(!policy.accepts_authority("old.example:3000"));
        // Neighbours of a configured name are not the configured name.
        assert!(!policy.accepts_authority("evil.crucible.example.com"));
        assert!(!policy.accepts_authority("crucible.example.com.evil.test"));
    }

    #[test]
    fn from_web_config_forwards_the_configured_allowed_hosts() {
        // The seam where a documented control silently becomes dead: drop
        // `allowed_hosts` here and every proxy deployment 403s while the config
        // key appears to work.
        let config = WebConfig {
            host: "0.0.0.0".to_string(),
            port: 3000,
            allowed_hosts: vec!["crucible.example.com".to_string()],
            ..WebConfig::default()
        };
        let policy = HostPolicy::from_web_config(&config);
        assert!(policy.accepts_authority("crucible.example.com"));
        assert!(policy.accepts_authority("192.168.0.16:3000"));
        assert!(!policy.accepts_authority("evil.test:3000"));
    }

    #[test]
    fn normalize_authority_rejects_everything_that_is_not_a_bare_authority() {
        for raw in [
            "",
            " ",
            "host:",
            "host:0",
            "host:99999",
            "::1:3000",         // unbracketed IPv6
            "[::1:3000",        // unterminated bracket
            "[not-an-ip]:3000", // brackets must hold an IPv6 literal
            "host/path",
            "host?q=1",
            "http://host:3000",
            "ho st:3000",
            "host\u{00e9}:3000",
            "%31%32%37.0.0.1:3000",
            "..:3000",
            ".host:3000",
        ] {
            assert_eq!(normalize_authority(raw), None, "{raw:?} must not parse");
        }

        // …and canonicalizes the spellings that ARE legal.
        assert_eq!(
            normalize_authority("LocalHost.:3000").as_deref(),
            Some("localhost:3000")
        );
        assert_eq!(
            normalize_authority("[0:0:0:0:0:0:0:1]:03000").as_deref(),
            Some("[::1]:3000")
        );
    }
}
