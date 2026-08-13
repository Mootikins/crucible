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
    #[error(
        "[web] allowed_hosts entry {entry:?} is not a `host`, `host:port`, or `.suffix` entry \
         — a glob like `*.example.com` is spelled `.example.com`"
    )]
    Unparseable { entry: String },

    #[error("[web] allowed_hosts entry {entry:?} names nothing after its leading dot")]
    NoSuffix { entry: String },

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
        return Err(InvalidAllowedHost::NoSuffix {
            entry: entry.to_string(),
        });
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
    /// are unbounded and cannot be enumerated. That costs the CORS list
    /// nothing: a suffix entry describes a proxied or tunnelled deployment,
    /// which the browser sees as *same*-origin, so CORS is never consulted for
    /// it (see `build_cross_origin_allowlist`).
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
mod tests {
    use super::*;

    /// [`HostPolicy::from_bind_with_local_names`] for a case whose entries are
    /// well-formed. The rejections live in their own tests below, where the
    /// error is the assertion rather than a `.expect` nobody reads.
    fn policy_for_bind(
        bind_host: &str,
        port: u16,
        extra_hosts: &[String],
        local_names: &[String],
    ) -> HostPolicy {
        HostPolicy::from_bind_with_local_names(bind_host, port, extra_hosts, local_names)
            .expect("well-formed allowed_hosts")
    }

    #[test]
    fn a_wildcard_bind_accepts_lan_ips_but_never_an_unrelated_name() {
        // `--host 0.0.0.0` is the LAN case: the machine's own addresses cannot
        // be enumerated, and an IP-literal Host cannot come from rebinding
        // (that needs a name). Every name other than this machine's own still
        // has to be allow-listed — supplied empty here so the assertions pin
        // the IP-literal rule alone, whatever the box running them is called.
        let policy = policy_for_bind("0.0.0.0", 3000, &[], &[]);
        assert!(policy.accepts_authority("192.168.0.16:3000"));
        assert!(policy.accepts_authority("[fd00::1]:3000"));
        assert!(!policy.accepts_authority("192.168.0.16:3001"));
        assert!(!policy.accepts_authority("evil.test:3000"));
        assert!(!policy.accepts_authority("nas.local:3000"));

        // A specific bind does not get the IP-literal allowance.
        let loopback = policy_for_bind("127.0.0.1", 3000, &[], &[]);
        assert!(!loopback.accepts_authority("192.168.0.16:3000"));
    }

    #[test]
    fn configured_public_names_are_accepted_bare_and_on_the_bind_port() {
        // `cru tunnel` / reverse proxy: the browser's Host is the public name,
        // with no port when the proxy terminates on 80 or 443.
        let policy = policy_for_bind(
            "127.0.0.1",
            3000,
            &[
                "crucible.example.com".to_string(),
                "old.example:8443".into(),
            ],
            &[],
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
        let policy = policy_for_bind("0.0.0.0", 3000, &[], &names);

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
        let policy = policy_for_bind("192.168.0.16", 3000, &[], &names);

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
            let policy = policy_for_bind(bind, 3000, &[], &names);
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
        let policy = policy_for_bind("0.0.0.0", 3000, &[], &names);

        assert!(policy.accepts_authority("node7.example.com:3000"));
        assert!(policy.accepts_authority("node7.example.com"));
        assert!(policy.accepts_authority("node7:3000"));
        // The FQDN's neighbours are still not the FQDN.
        assert!(!policy.accepts_authority("evil.example.com:3000"));
        assert!(!policy.accepts_authority("node7.example.com.evil.test:3000"));
        assert!(!policy.accepts_authority("node7.example.com:3001"));

        // …and a loopback bind still answers to no LAN name, FQDN included.
        let loopback = policy_for_bind("127.0.0.1", 3000, &[], &names);
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
        let policy = HostPolicy::from_web_config(&config).expect("well-formed config");
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

    #[test]
    fn the_host_shapes_upstream_host_checks_get_wrong_are_parsed_or_refused() {
        // Ported from the regression suites of Django, Rails and Werkzeug —
        // every one of these is a shape that got some framework's Host check
        // wrong at least once.
        for raw in [
            "example.com:80",
            "[2001:19f0:feee::dead:beef:cafe]:8080",
            // Punycode is ASCII by construction, so an IDN needs no special case.
            "xn--4ca9at.com",
            // The fully-qualified spelling of the same name.
            "example.com.",
        ] {
            assert!(normalize_authority(raw).is_some(), "{raw:?} must parse");
        }

        for raw in [
            "example.com..",
            "example.com@evil.tld",
            "example.com:80/badpath",
            "www.example.com:80:80",
            // Django accepts a leading dot in the *Host* header, which turns
            // its own suffix syntax into a bypass. This one does not.
            ".example.com",
            "..example.com",
        ] {
            assert_eq!(normalize_authority(raw), None, "{raw:?} must not parse");
        }
    }

    #[test]
    fn a_suffix_entry_admits_the_apex_and_exactly_one_label_beneath_it() {
        // `.example.com` is the leading-dot syntax Django, Rails, Vite,
        // webpack-dev-server and Werkzeug all share — with Rails' narrower
        // depth rule, because any-depth matching inherits every delegated
        // subtree under the apex.
        let policy = policy_for_bind("127.0.0.1", 3000, &[".example.com".to_string()], &[]);

        assert!(policy.accepts_authority("example.com"));
        assert!(policy.accepts_authority("app.example.com"));
        assert!(policy.accepts_authority("app.example.com:3000"));
        // Two labels deep is where a dangling NS record under the apex would
        // hand an attacker a name inside it.
        assert!(!policy.accepts_authority("a.b.example.com"));
        assert!(!policy.accepts_authority("a.b.example.com:3000"));
    }

    #[test]
    fn a_suffix_entry_refuses_a_name_that_merely_ends_with_its_apex() {
        let policy = policy_for_bind("127.0.0.1", 3000, &[".example.com".to_string()], &[]);

        // `ends_with("example.com")` — the classic form of this bug.
        assert!(!policy.accepts_authority("evilexample.com"));
        // Rails' CVE-2021-22903: the apex interpolated into a regex unescaped,
        // where `.` matched the `-`.
        assert!(!policy.accepts_authority("sub-example.com"));
        // The apex as a prefix of somebody else's name.
        assert!(!policy.accepts_authority("example.com.evil.test"));
        // A Host that is only the suffix — refused at the parse step, and again
        // by the empty-label rule if it ever got past it.
        assert!(!policy.answers_to(".example.com"));
        assert!(!policy.answers_to("..example.com"));
        // A different apex of the same shape.
        assert!(!policy.accepts_authority("app.example.test"));
    }

    #[test]
    fn an_ip_literal_host_is_never_matched_by_a_suffix_entry() {
        // The entry is contrived on purpose: a rule that admits *names* under a
        // domain must have no way to reach an ADDRESS, whatever the apex looks
        // like. Without that rule `.0.0.1` would hand out every 10.0.0.x host,
        // and the IP-literal path is exactly the one that is safe only because
        // there is no name involved.
        let policy = policy_for_bind("127.0.0.1", 3000, &[".0.0.1".to_string()], &[]);

        assert!(!policy.accepts_authority("10.0.0.1:3000"));
        assert!(!policy.accepts_authority("10.0.0.1"));
        assert!(!policy.accepts_authority("[::1]:3001"));
        // The loopback bind's own authority is unaffected.
        assert!(policy.accepts_authority("127.0.0.1:3000"));
    }

    #[test]
    fn a_suffix_entry_follows_the_same_port_rule_as_an_exact_entry() {
        let policy = policy_for_bind(
            "127.0.0.1",
            3000,
            &[".example.com".to_string(), ".old.example:8443".to_string()],
            &[],
        );

        // No port on the entry: bare (a proxy terminating on 80/443) or the
        // port we bound.
        assert!(policy.accepts_authority("app.example.com"));
        assert!(policy.accepts_authority("app.example.com:3000"));
        assert!(!policy.accepts_authority("app.example.com:3001"));
        // A port on the entry is exact.
        assert!(policy.accepts_authority("app.old.example:8443"));
        assert!(policy.accepts_authority("old.example:8443"));
        assert!(!policy.accepts_authority("app.old.example"));
        assert!(!policy.accepts_authority("app.old.example:3000"));
    }

    #[test]
    fn a_malformed_allowed_hosts_entry_refuses_to_start() {
        // The regression this exists for: `*.example.test` was dropped with one
        // `tracing::warn!` line, so the operator got a 403 from an allow-list
        // that looked configured and was empty.
        let refusals = [
            ("*.example.test", "a glob is not the syntax"),
            ("*", "nor is a bare star"),
            (".", "a bare dot names nothing"),
            ("..", "and neither does two"),
            (".:3000", "a port is not a name"),
            (
                "..example.test",
                "a doubled dot is not a name a browser sends",
            ),
            (".192.168.0.1", "an address cannot be a name suffix"),
            ("http://example.test", "a URL is not an authority"),
            ("example.test/path", "nor is a path"),
        ];
        for (entry, why) in refusals {
            let err = HostPolicy::from_bind_with_local_names(
                "127.0.0.1",
                3000,
                &[entry.to_string()],
                &[],
            )
            .expect_err(why);
            // The message has to name the entry, or the operator cannot find it
            // in a config file with a dozen of them.
            assert!(err.to_string().contains(entry), "{err}");
        }
    }

    #[test]
    fn a_public_suffix_allowed_hosts_entry_refuses_to_start() {
        // Vite's documentation puts it plainly: "you should never add Top-Level
        // Domains like .com to the list." Every name anyone can register under
        // one of these would be an authority this server answers to.
        for entry in [
            ".com",
            ".io",
            ".test",
            ".local",
            ".internal",
            ".co.uk",
            ".github.io",
            ".trycloudflare.com",
        ] {
            let err = HostPolicy::from_bind_with_local_names(
                "127.0.0.1",
                3000,
                &[entry.to_string()],
                &[],
            )
            .expect_err(entry);
            assert_eq!(
                err,
                InvalidAllowedHost::PublicSuffix {
                    entry: entry.to_string()
                },
                "{entry} must be refused as a public suffix"
            );
        }

        // …while a domain someone actually controls under one of them is fine.
        let policy = policy_for_bind("127.0.0.1", 3000, &[".crucible.co.uk".to_string()], &[]);
        assert!(policy.accepts_authority("app.crucible.co.uk"));
    }

    #[test]
    fn a_suffix_entry_inside_an_undelegated_namespace_refuses_to_start() {
        // A registered domain gets you a subtree; mDNS and `.internal` do not.
        // Whoever answers the query owns the name while they answer it, so
        // `.node7.local` reads as "under my machine" and means "any name any LAN
        // peer claims" — which is the rebinding vehicle the allow-list exists to
        // refuse. Depth changes nothing about who may answer.
        for entry in [
            ".node7.local",
            ".deep.node7.local",
            ".svc.internal",
            ".box.home.arpa",
        ] {
            let err = HostPolicy::from_bind_with_local_names(
                "127.0.0.1",
                3000,
                &[entry.to_string()],
                &[],
            )
            .expect_err(entry);
            assert_eq!(
                err,
                InvalidAllowedHost::PublicSuffix {
                    entry: entry.to_string()
                },
                "{entry} must be refused however deep it sits"
            );
        }

        // The EXACT form stays available, and is the one to use: it names a
        // single authority instead of a namespace, and the real host already
        // holds that name against mDNS conflict resolution.
        let policy = policy_for_bind("0.0.0.0", 3000, &["node7.local".to_string()], &[]);
        assert!(policy.accepts_authority("node7.local:3000"));
        assert!(!policy.accepts_authority("evil.node7.local:3000"));
    }
}
