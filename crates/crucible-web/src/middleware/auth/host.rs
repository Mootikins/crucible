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
    ///
    /// A bind that other machines can reach also answers to this machine's own
    /// names — see [`local_names`].
    pub fn from_bind(bind_host: &str, port: u16, extra_hosts: &[String]) -> Self {
        Self::from_bind_with_local_names(bind_host, port, extra_hosts, &local_names())
    }

    /// [`HostPolicy::from_bind`] with this machine's names supplied rather than
    /// read from the OS, so the rule can be tested without depending on what
    /// the box running the test happens to be called.
    pub fn from_bind_with_local_names(
        bind_host: &str,
        port: u16,
        extra_hosts: &[String],
        local_names: &[String],
    ) -> Self {
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

        // A socket other machines can reach is addressed by NAME from those
        // machines, and until this existed no name reached it: the IP-literal
        // rule below covers `http://192.168.0.16:3000` and nothing covered
        // `http://node7:3000`, so a LAN bind that worked by address 403'd by
        // hostname and read as if it had never left loopback.
        //
        // The rebinding cost, stated rather than glossed: the IP-literal rule
        // is safe *because* there is no name to rebind, and this gives that up
        // for these names alone. The attacker it admits is one who controls
        // resolution of this machine's own hostname on the victim's network —
        // who is already positioned between the operator and the machine. A
        // loopback bind buys none of that back and so gets none of these names.
        if !bind_ip.is_some_and(|ip| ip.is_loopback()) && bind_host != "localhost" {
            for name in local_names {
                allow(name);
                allow(&format!("{name}:{port}"));
            }
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

    /// [`HostPolicy::accepts`] asked of a bare `host[:port]` rather than a
    /// request.
    ///
    /// For code that *renders* an authority instead of receiving one — `cru
    /// web`'s startup banner checks every URL it is about to print, so a URL
    /// the guard would refuse can never reach the operator's screen.
    pub fn answers_to(&self, authority: &str) -> bool {
        normalize_authority(authority).is_some_and(|canonical| self.accepts_authority(&canonical))
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

/// The names this machine answers to, as a browser elsewhere on the LAN would
/// spell them.
///
/// Read from the OS at policy-construction time. Callers that need this to be
/// deterministic take [`HostPolicy::from_bind_with_local_names`] instead.
pub fn local_names() -> Vec<String> {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    local_names_from(&host, canonical_name(&host).as_deref())
}

/// [`local_names`] minus the OS calls: the hostname, its mDNS spelling when the
/// hostname is a bare label, and the resolver's canonical name for it.
///
/// `<host>.local` is included because that is the name Avahi/Bonjour publishes
/// and therefore the one a phone or laptop on the same network actually
/// resolves; a hostname that already carries a domain is left alone. Anything
/// that is not a usable authority — empty, whitespace, or `localhost`, which
/// the loopback rule already owns — yields nothing rather than putting a name
/// in the allow-list that no `Host` header could ever match.
///
/// `canonical` is what [`canonical_name`] resolved, and is accepted only when it
/// *extends* the hostname (`node7` → `node7.example.com`). A resolver that
/// answers with an unrelated name is not describing this machine's own identity,
/// and admitting it would let whoever controls resolution choose a rebinding
/// target — the one thing the whole allow-list exists to prevent. Operators
/// whose public name is genuinely unrelated to the hostname have
/// `[web] allowed_hosts`, which is explicit and auditable.
fn local_names_from(hostname: &str, canonical: Option<&str>) -> Vec<String> {
    let host = hostname.trim().to_ascii_lowercase();
    if host.is_empty() || host == "localhost" || normalize_authority(&host).is_none() {
        return Vec::new();
    }
    let mut names = vec![host.clone()];
    // Bare label: add the mDNS spelling the network actually publishes.
    if !host.contains('.') {
        names.push(format!("{host}.local"));
    }
    if let Some(fqdn) = canonical
        .map(|c| c.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|c| c != &host && !c.is_empty())
        .filter(|c| c.strip_prefix(&format!("{host}.")).is_some_and(|d| !d.is_empty()))
        .filter(|c| normalize_authority(c).is_some())
    {
        names.push(fqdn);
    }
    names
}

/// The resolver's canonical name for `hostname`, or `None`.
///
/// This is `getaddrinfo(AI_CANONNAME)` (RFC 3493) — the portable mechanism, and
/// the same one `hostname --fqdn` uses. It is what makes a normally-configured
/// host reachable by its real domain without the operator listing it: a machine
/// whose `/etc/hostname` is an FQDN, or whose DNS/`hosts` canonicalizes the bare
/// label, resolves here. A machine where nothing declares an FQDN gets the bare
/// hostname back and nothing is added — deliberately, because a resolver
/// `search` suffix is not a standard statement that the host answers to
/// `host.suffix`, and scraping `resolv.conf` for one is glibc-specific and
/// meaningless behind a systemd-resolved stub.
///
/// Runs once, at policy construction. A lookup of the machine's own name
/// normally resolves from `hosts` without touching the network; a resolver that
/// stalls would delay startup by its own timeout, which is the same exposure
/// `hostname --fqdn` has always had.
/// `dns-lookup` rather than raw FFI: this file is the rebinding defence, so it
/// is the last place to hand-roll `unsafe` pointer walking over `addrinfo`.
/// `libc` still supplies `AI_CANONNAME`, whose value is per-platform — reading a
/// constant needs no `unsafe`, and there is no pure-Rust substitute for asking
/// the *system* resolver (a DNS-only client would skip `hosts` and nsswitch,
/// which is exactly the configuration this has to honour).
fn canonical_name(hostname: &str) -> Option<String> {
    let hints = dns_lookup::AddrInfoHints {
        flags: libc::AI_CANONNAME,
        socktype: dns_lookup::SockType::Stream.into(),
        ..Default::default()
    };
    dns_lookup::getaddrinfo(Some(hostname.trim()), None, Some(hints))
        .ok()?
        .filter_map(Result::ok)
        .find_map(|info| info.canonname)
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
    fn a_wildcard_bind_accepts_lan_ips_but_never_an_unrelated_name() {
        // `--host 0.0.0.0` is the LAN case: the machine's own addresses cannot
        // be enumerated, and an IP-literal Host cannot come from rebinding
        // (that needs a name). Every name other than this machine's own still
        // has to be allow-listed — supplied empty here so the assertions pin
        // the IP-literal rule alone, whatever the box running them is called.
        let policy = HostPolicy::from_bind_with_local_names("0.0.0.0", 3000, &[], &[]);
        assert!(policy.accepts_authority("192.168.0.16:3000"));
        assert!(policy.accepts_authority("[fd00::1]:3000"));
        assert!(!policy.accepts_authority("192.168.0.16:3001"));
        assert!(!policy.accepts_authority("evil.test:3000"));
        assert!(!policy.accepts_authority("nas.local:3000"));

        // A specific bind does not get the IP-literal allowance.
        let loopback = HostPolicy::from_bind_with_local_names("127.0.0.1", 3000, &[], &[]);
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
    fn a_remotely_reachable_bind_answers_to_this_machines_own_names() {
        // The LAN case the IP-literal rule does not cover: a browser on another
        // machine addresses this box by NAME, and that Host used to be refused
        // no matter how the operator bound the socket.
        let names = ["node7".to_string(), "node7.local".to_string()];
        let policy = HostPolicy::from_bind_with_local_names("0.0.0.0", 3000, &[], &names);

        assert!(policy.accepts_authority("node7:3000"));
        assert!(policy.accepts_authority("node7.local:3000"));
        // Same as a configured entry: bare too, for a proxy terminating on 80/443.
        assert!(policy.accepts_authority("node7"));
        // Everything the rule did NOT open stays shut.
        assert!(!policy.accepts_authority("node7:3001"));
        assert!(!policy.accepts_authority("evil.test:3000"));
        assert!(!policy.accepts_authority("node7.evil.test:3000"));
        assert!(!policy.accepts_authority("evil.node7:3000"));
    }

    #[test]
    fn a_bind_to_one_lan_interface_also_answers_to_this_machines_names() {
        // Narrowing the bind to one interface does not change how a browser
        // spells the machine, so the names have to come along.
        let names = ["node7".to_string()];
        let policy = HostPolicy::from_bind_with_local_names("192.168.0.16", 3000, &[], &names);

        assert!(policy.accepts_authority("node7:3000"));
        assert!(policy.accepts_authority("192.168.0.16:3000"));
        // Still not a wildcard bind: other IP literals are not ours.
        assert!(!policy.accepts_authority("10.0.0.5:3000"));
    }

    #[test]
    fn a_loopback_bind_answers_to_no_name_but_its_own() {
        // Nothing off this machine can reach a loopback socket, so there is no
        // LAN case to serve — and every name admitted here would be a
        // rebinding target bought for nothing.
        let names = ["node7".to_string(), "node7.local".to_string()];
        for bind in ["127.0.0.1", "::1", "localhost"] {
            let policy = HostPolicy::from_bind_with_local_names(bind, 3000, &[], &names);
            assert!(
                !policy.accepts_authority("node7:3000"),
                "{bind} bind must not admit the machine's LAN name"
            );
            assert!(policy.accepts_authority("localhost:3000"), "{bind}");
        }
    }

    #[test]
    fn the_local_names_are_the_hostname_and_its_mdns_spelling() {
        // Derivation only — what the OS reports is environment, not contract.
        assert_eq!(local_names_from("node7", None), ["node7", "node7.local"]);
        // Already qualified: `.local` is not appended to a dotted name.
        assert_eq!(local_names_from("node7.lan", None), ["node7.lan"]);
        assert_eq!(local_names_from("Node7", None), ["node7", "node7.local"]);
        // A hostname that is not a usable authority yields nothing rather than
        // poisoning the allow-list with something unparseable.
        assert!(local_names_from("", None).is_empty());
        assert!(local_names_from("localhost", None).is_empty());
        assert!(local_names_from("not a hostname", None).is_empty());
    }

    #[test]
    fn the_resolvers_canonical_name_extends_the_hostname_or_is_ignored() {
        // The case this exists for: a bare `/etc/hostname` whose FQDN lives in
        // DNS or `hosts`. Without it, a LAN browser at the machine's real
        // domain 403s while the same box answers to the bare name and an IP.
        assert_eq!(
            local_names_from("node7", Some("node7.example.com")),
            ["node7", "node7.local", "node7.example.com"]
        );
        // Spelling differences in the resolver's answer are not new names.
        assert_eq!(
            local_names_from("node7", Some("NODE7.example.com.")),
            ["node7", "node7.local", "node7.example.com"]
        );
        // Canonical name == hostname (the common answer): nothing to add.
        assert_eq!(
            local_names_from("node7", Some("node7")),
            ["node7", "node7.local"]
        );
        // An FQDN hostname whose resolver agrees stays a single name.
        assert_eq!(
            local_names_from("node7.example.com", Some("node7.example.com")),
            ["node7.example.com"]
        );

        // A canonical name that does NOT extend the hostname is not this
        // machine describing itself — admitting it would hand whoever controls
        // resolution a rebinding target, which is what the allow-list is for.
        for hostile in [
            "evil.test",
            "node7-evil.test",
            "notnode7.example.com",
            "evil.node7",
            "node7.",
            "",
        ] {
            assert_eq!(
                local_names_from("node7", Some(hostile)),
                ["node7", "node7.local"],
                "{hostile:?} must not reach the allow-list"
            );
        }
    }

    #[test]
    fn a_reachable_bind_answers_to_the_resolved_fqdn() {
        // End to end through the policy: the FQDN is accepted bare and on the
        // bind port, exactly like the hostname it extends.
        let names = local_names_from("node7", Some("node7.example.com"));
        let policy = HostPolicy::from_bind_with_local_names("0.0.0.0", 3000, &[], &names);

        assert!(policy.accepts_authority("node7.example.com:3000"));
        assert!(policy.accepts_authority("node7.example.com"));
        assert!(policy.accepts_authority("node7:3000"));
        // The FQDN's neighbours are still not the FQDN.
        assert!(!policy.accepts_authority("evil.example.com:3000"));
        assert!(!policy.accepts_authority("node7.example.com.evil.test:3000"));
        assert!(!policy.accepts_authority("node7.example.com:3001"));

        // …and a loopback bind still answers to no LAN name, FQDN included.
        let loopback = HostPolicy::from_bind_with_local_names("127.0.0.1", 3000, &[], &names);
        assert!(!loopback.accepts_authority("node7.example.com:3000"));
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
