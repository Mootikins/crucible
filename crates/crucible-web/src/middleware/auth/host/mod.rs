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
    /// Leading-dot `allowed_hosts` entries (`.example.com`), each admitting its
    /// apex plus exactly one label beneath it.
    suffixes: Vec<SuffixRule>,
    /// Accept *any* IP-literal host on [`port`](Self::port). Set for wildcard
    /// binds (`0.0.0.0` / `::`), whose reachable LAN addresses cannot be
    /// enumerated up front. Safe because an IP literal in `Host` means the
    /// browser navigated straight to that address — rebinding needs a *name*.
    any_ip_literal: bool,
    /// The bind port, used for the IP-literal rule above.
    port: u16,
}

/// A `[web] allowed_hosts` entry the server refuses to run with.
///
/// A hard error rather than the warn-and-drop this replaced. That version cost
/// this project real time: a `*.example.test` entry produced a 403 and a single
/// `tracing::warn!` line nobody was looking at, so the allow-list read as
/// configured and behaved as empty. An entry an operator wrote is either
/// honoured or it stops the server.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidAllowedHost {
    /// Covers the bare `.` too, which names nothing after its dot. That had its
    /// own variant, reachable for exactly that one input and handled — like
    /// every variant here — by `server.rs` stringifying it; YAGNI says the
    /// message can carry the distinction instead of the type.
    #[error(
        "[web] allowed_hosts entry {entry:?} is not a `host`, `host:port`, or `.suffix` entry \
         — a glob like `*.example.com` is spelled `.example.com`, and a lone `.` names nothing"
    )]
    Unparseable { entry: String },

    #[error(
        "[web] allowed_hosts entry {entry:?} is a public suffix — it would answer to every \
         name anyone can register under it. Name the domain you control (`.crucible.example.com`)"
    )]
    PublicSuffix { entry: String },
}

/// A leading-dot `allowed_hosts` entry: the apex, plus exactly one label under it.
#[derive(Clone, Debug)]
struct SuffixRule {
    /// The apex WITH its leading dot (`.example.com`) — see
    /// [`SuffixRule::matches_host`] for why the dot is stored rather than
    /// stripped.
    dotted_apex: String,
    /// The port the entry named, if it named one. Same meaning as for an exact
    /// entry: `Some` matches that port only, `None` matches bare or the bind port.
    port: Option<u16>,
}

impl SuffixRule {
    /// Whether `host` is this rule's apex, or exactly ONE label beneath it.
    ///
    /// One label, not any depth — Rails' rule rather than Django's. Any-depth
    /// matching inherits every delegated subtree beneath the apex, and one
    /// dangling NS record down there hands an attacker A-record control of a
    /// name inside it, which is exactly the primitive DNS rebinding needs. The
    /// operator who really does want `a.b.example.com` can list it.
    ///
    /// The compared suffix keeps its dot (`.example.com`, never `example.com`)
    /// because `ends_with("example.com")` also accepts `evilexample.com` — the
    /// classic form of this bug. And the comparison is `strip_suffix`, not a
    /// regex: Rails' CVE-2021-22903 was an apex interpolated unescaped into a
    /// pattern, where `.` matched any character and `sub-example.com` passed as
    /// `sub.example.com`.
    fn matches_host(&self, host: &str) -> bool {
        // An IP literal is reached by address, so there is no name to rebind and
        // nothing to delegate; it keeps the exact / any-IP-literal paths only.
        if is_ip_literal(host) {
            return false;
        }
        if host == self.apex() {
            return true;
        }
        match host.strip_suffix(self.dotted_apex.as_str()) {
            // Present, and one label. `normalize_authority` already refuses a
            // leading or doubled dot, so this is the second lock on a `Host` of
            // `.example.com` or `..example.com` — both of which Django accepts.
            Some(label) => !label.is_empty() && !label.contains('.'),
            None => false,
        }
    }

    fn apex(&self) -> &str {
        &self.dotted_apex[1..]
    }

    /// The port rule, identical to an exact entry's: an entry naming a port
    /// matches that port only, and one naming none matches bare (a proxy
    /// terminating on 80/443 forwards the name without a port) or the bind port.
    fn accepts_port(&self, port: Option<u16>, bind_port: u16) -> bool {
        match self.port {
            Some(expected) => port == Some(expected),
            None => port.is_none() || port == Some(bind_port),
        }
    }
}

/// One parsed `allowed_hosts` entry.
enum AllowedHost {
    /// A canonical `host[:port]` matched verbatim.
    Exact(String),
    Suffix(SuffixRule),
}

/// Parse one `[web] allowed_hosts` entry, or say why it cannot be used.
///
/// Every rejection here is a startup failure, so each one has to be a genuine
/// mistake rather than a taste: an entry that cannot parse, an entry with no
/// name after its dot, and an entry whose suffix is public — none of which can
/// ever admit the authority its author meant.
fn parse_allowed_host(entry: &str) -> Result<AllowedHost, InvalidAllowedHost> {
    let unparseable = || InvalidAllowedHost::Unparseable {
        entry: entry.to_string(),
    };
    let Some(after_dot) = entry.strip_prefix('.') else {
        return normalize_authority(entry)
            .map(AllowedHost::Exact)
            .ok_or_else(unparseable);
    };
    if after_dot.is_empty() {
        return Err(unparseable());
    }
    let canonical = normalize_authority(after_dot).ok_or_else(unparseable)?;
    let (apex, port) = split_canonical(&canonical);
    // `.192.168.0.1` parses but can never match anything, since suffix matching
    // declines IP literals — the same invisible nothing this whole error type
    // exists to stop.
    if is_ip_literal(apex) {
        return Err(unparseable());
    }
    if is_public_suffix(apex) {
        return Err(InvalidAllowedHost::PublicSuffix {
            entry: entry.to_string(),
        });
    }
    Ok(AllowedHost::Suffix(SuffixRule {
        dotted_apex: format!(".{apex}"),
        port,
    }))
}

/// Whether wildcarding this apex would hand the server to strangers.
///
/// A single-label apex is a TLD — Vite's docs put it plainly: "you should never
/// add Top-Level Domains like .com to the list" — and that rule alone catches
/// `.com`, `.io`, `.local` and `.internal`. The two lists below extend it, and
/// they are checked differently because they are unsafe for different reasons.
///
/// [`UNDELEGATED_SPACES`] is refused **at any depth**. Nobody owns a subtree of
/// mDNS or `.internal`: whoever answers the query owns the name for as long as
/// they answer it, so `.node7.local` is not "names under my machine" but "any
/// name any LAN peer cares to claim". That is a rebinding vehicle — a peer
/// answers `app.node7.local` with their own address, serves hostile script from
/// it, then answers with this server's address, and the origin never changes.
///
/// [`SHARED_APEXES`] is refused **only as the apex itself**, because there the
/// sub-name genuinely is yours: `.myapp.trycloudflare.com` is your tunnel, while
/// bare `.trycloudflare.com` hands the next subdomain along to a stranger.
fn is_public_suffix(apex: &str) -> bool {
    !apex.contains('.')
        || SHARED_APEXES.contains(&apex)
        || UNDELEGATED_SPACES
            .iter()
            .any(|space| apex == *space || apex.ends_with(&format!(".{space}")))
}

/// Namespaces with no ownership at all, refused at every depth.
const UNDELEGATED_SPACES: &[&str] = &["local", "internal", "home.arpa", "intranet", "lan", "corp"];

/// Multi-label apexes refused for the same reason a TLD is: the label to the
/// left of them belongs to whoever registered it, which is not necessarily you.
///
/// Deliberately a short hand-written list and **best-effort**, not the Public
/// Suffix List: a weekly-changing dependency is out of proportion to catching a
/// config mistake, and the single-label rule above already covers every real
/// TLD. What is left is what an operator here would plausibly type — the shared
/// apexes of the tunnels and hosts this project documents. An apex missing from
/// this list is accepted, so the check narrows the mistake rather than closing
/// the class.
const SHARED_APEXES: &[&str] = &[
    "trycloudflare.com",
    "ngrok.io",
    "ngrok.app",
    "ngrok-free.app",
    "ngrok-free.dev",
    "loca.lt",
    "serveo.net",
    "github.io",
    "pages.dev",
    "workers.dev",
    "vercel.app",
    "netlify.app",
    "herokuapp.com",
    "onrender.com",
    "fly.dev",
    "railway.app",
    "azurewebsites.net",
    "s3.amazonaws.com",
    "co.uk",
    "org.uk",
    "ac.uk",
    "me.uk",
    "gov.uk",
    "com.au",
    "co.jp",
    "co.nz",
    "com.br",
    "co.za",
];

impl HostPolicy {
    /// Derive the policy from the web server's own configuration.
    ///
    /// Takes the config whole rather than the three fields separately, and is
    /// what the server actually calls: `[web] allowed_hosts` is the only
    /// escape hatch for reverse-proxy and tunnel deployments, and an argument
    /// a caller has to remember to forward is one that will eventually be
    /// forwarded as `&[]` — leaving a documented control with no input.
    ///
    /// Fallible because a malformed `allowed_hosts` entry stops the server; see
    /// [`InvalidAllowedHost`].
    pub fn from_web_config(config: &WebConfig) -> Result<Self, InvalidAllowedHost> {
        Self::from_bind(&config.host, config.port, &config.allowed_hosts)
    }

    /// Derive the policy from the bind address, plus any operator-configured
    /// public names (reverse proxy / `cru tunnel`).
    ///
    /// Always accepted: `localhost`, `127.0.0.1` and `[::1]` on `port` — the
    /// three spellings of the same loopback the operator actually bound.
    /// A configured entry that carries its own port is matched exactly; one
    /// without a port is accepted both bare (a proxy on 80/443 forwards the
    /// public name with no port) and with `port` appended. An entry that starts
    /// with a dot (`.example.com`) is a suffix — see [`SuffixRule`].
    ///
    /// A bind that other machines can reach also answers to this machine's own
    /// names — see [`local_names`].
    pub fn from_bind(
        bind_host: &str,
        port: u16,
        extra_hosts: &[String],
    ) -> Result<Self, InvalidAllowedHost> {
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
    ) -> Result<Self, InvalidAllowedHost> {
        let mut allowed = BTreeSet::new();
        // Warn-and-drop survives here, for the authorities this function derives
        // itself: the bind address and whatever the OS calls this machine are
        // environment, not something an operator typed, and a hostname the
        // resolver reports in a shape no `Host` header could carry is not a
        // reason to refuse to serve loopback. Operator entries are the fallible
        // path below.
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

        let mut suffixes = Vec::new();
        for entry in extra_hosts
            .iter()
            .map(|h| h.trim())
            .filter(|h| !h.is_empty())
        {
            match parse_allowed_host(entry)? {
                AllowedHost::Exact(canonical) => {
                    if split_canonical(&canonical).1.is_none() {
                        allowed.insert(format!("{canonical}:{port}"));
                    }
                    allowed.insert(canonical);
                }
                AllowedHost::Suffix(rule) => suffixes.push(rule),
            }
        }

        Ok(Self {
            allowed,
            suffixes,
            any_ip_literal,
            port,
        })
    }

    /// Every canonical `host[:port]` authority accepted verbatim.
    ///
    /// Excludes the open-ended rules — "any IP literal on the bind port" from a
    /// wildcard bind, and any `.example.com` suffix entry — because those sets
    /// are unbounded and cannot be enumerated. Usually that costs nothing: a
    /// suffix entry describes a proxied or tunnelled deployment, which the
    /// browser sees as *same*-origin, so CORS is never consulted for it (see
    /// `build_cross_origin_allowlist`).
    ///
    /// The exception is a proxy that does NOT rewrite `Host` — stock
    /// `proxy_pass http://127.0.0.1:3000` without `proxy_set_header Host $host`
    /// forwards `Host: 127.0.0.1:3000` while the browser sends
    /// `Origin: https://app.example.com`. `websocket_origin_guard` then cannot
    /// take its same-origin shortcut and falls back to this list, so migrating
    /// an exact entry to `.example.com` can 403 the terminal upgrade. Keep the
    /// exact entry alongside the suffix one, set `CRUCIBLE_CORS_ORIGINS`, or
    /// (better) have the proxy forward the real `Host`.
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
        let (host, port) = split_canonical(canonical);
        if self
            .suffixes
            .iter()
            .any(|rule| rule.accepts_port(port, self.port) && rule.matches_host(host))
        {
            return true;
        }
        if !self.any_ip_literal {
            return false;
        }
        match port {
            Some(port) => port == self.port && is_ip_literal(host),
            None => false,
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
        .filter(|c| {
            c.strip_prefix(&format!("{host}."))
                .is_some_and(|d| !d.is_empty())
        })
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
mod tests;
