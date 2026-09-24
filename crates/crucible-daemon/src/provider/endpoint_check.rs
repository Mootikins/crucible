//! The check on a provider endpoint that a request names.
//!
//! A session's `endpoint` is a URL the daemon dials for chat, model listing
//! and the context-length probe. A request can name it: `session.create`,
//! `session.configure_agent` over RPC, the TUI's `cru session configure
//! --endpoint`, a Lua plugin's `configure_agent`, and `crucible-web`, which
//! relays the choice of a browser that can be on another machine. So the
//! daemon is the one component that sees every such endpoint, and the check
//! is here.
//!
//! The policy:
//!
//! - An endpoint whose origin the operator already configured is accepted
//!   with no further check. That is every `llm.providers` endpoint, every
//!   backend's compiled-in default (the local Ollama at
//!   `http://localhost:11434` among them), `OLLAMA_HOST` and `chat.endpoint`.
//!   The daemon dials those with no request at all, so a request that names
//!   one gets no new reach.
//! - Any other endpoint must use `http` or `https`, and every address its
//!   host maps to must be a globally routable unicast address. Loopback,
//!   private, link-local (the cloud metadata address `169.254.169.254`),
//!   CGNAT and every other non-global range are refused.
//!
//! Hostnames are resolved, and one internal answer refuses the endpoint. This
//! is a check at request time only: the dialer resolves the host again, so a
//! DNS record that changes between the two lookups (DNS rebinding) is not
//! stopped here. The HTTP clients in `model_listing` and `context_length`
//! refuse redirects for the same reason.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crucible_core::config::{ollama_endpoint_from_env, BackendType, LlmConfig};

/// The endpoints the operator configured, which a request may name freely.
///
/// `chat_endpoint` is the config store's `chat.endpoint`, passed in because
/// the store is read live by the caller.
pub(crate) fn configured_endpoints(
    llm: Option<&LlmConfig>,
    chat_endpoint: Option<String>,
) -> Vec<String> {
    let defaults = BackendType::all()
        .iter()
        .filter_map(BackendType::default_endpoint)
        .map(str::to_string);
    let providers = llm
        .into_iter()
        .flat_map(|llm| llm.providers.values())
        .map(|provider| provider.endpoint());
    defaults
        .chain(providers)
        .chain(ollama_endpoint_from_env())
        .chain(chat_endpoint)
        .collect()
}

/// Refuse `endpoint` if the daemon must not dial it for a request.
///
/// The error is a sentence for the caller: the caller can fix it.
pub(crate) async fn check_request_endpoint(
    endpoint: &str,
    configured: &[String],
) -> Result<(), String> {
    check_request_endpoint_with(endpoint, configured, resolve_host).await
}

type ResolvedAddrs = std::io::Result<Vec<IpAddr>>;

async fn resolve_host(host: String, port: u16) -> ResolvedAddrs {
    Ok(tokio::net::lookup_host((host.as_str(), port))
        .await?
        .map(|addr| addr.ip())
        .collect())
}

/// [`check_request_endpoint`] with the resolver injected, so a test can stand
/// in for DNS.
async fn check_request_endpoint_with<F, Fut>(
    endpoint: &str,
    configured: &[String],
    resolve: F,
) -> Result<(), String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: std::future::Future<Output = ResolvedAddrs>,
{
    let url = reqwest::Url::parse(endpoint)
        .map_err(|e| format!("Invalid endpoint URL {endpoint}: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Unsupported endpoint URL scheme: {scheme}")),
    }

    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| format!("Endpoint URL {endpoint} has no host"))?
        .to_string();

    if is_configured(&url, configured) {
        return Ok(());
    }

    // `host_str` is the normalized host: `http://2130706433` and
    // `http://0x7f.1` are already "127.0.0.1" here, and an IPv6 literal keeps
    // its brackets.
    let literal = host.trim_start_matches('[').trim_end_matches(']').parse();

    let addrs = match literal {
        Ok(ip) => vec![ip],
        Err(_) => {
            let port = url.port_or_known_default().unwrap_or(80);
            // An unresolvable host is an unknown host, not a safe one.
            let addrs = resolve(host.clone(), port)
                .await
                .map_err(|e| format!("Endpoint host {host} could not be resolved: {e}"))?;
            if addrs.is_empty() {
                return Err(format!("Endpoint host {host} resolved to no addresses"));
            }
            addrs
        }
    };

    match addrs.into_iter().find(|ip| is_internal_target(*ip)) {
        Some(ip) => Err(format!(
            "Endpoint must not target a private/internal address: {host} → {ip}. \
             To use a server on this machine or on a private network, add its \
             endpoint to a provider under `llm.providers` in the config."
        )),
        None => Ok(()),
    }
}

/// Whether `url` has the origin (scheme, host and port) of a configured
/// endpoint. The path does not matter: the same server answers it.
fn is_configured(url: &reqwest::Url, configured: &[String]) -> bool {
    configured
        .iter()
        .filter_map(|endpoint| reqwest::Url::parse(endpoint).ok())
        .any(|configured| configured.origin() == url.origin())
}

/// The IPv4 address an IPv6 address actually reaches, if any.
///
/// `::ffff:a.b.c.d` (v4-mapped), `::a.b.c.d` (v4-compatible),
/// `::ffff:0:a.b.c.d` (v4-translated), `2002:a.b.c.d::/16` (6to4) and
/// `64:ff9b::a.b.c.d` (NAT64) are all ways to write an IPv4 destination, so
/// the check judges them as that IPv4 address.
fn embedded_ipv4(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    fn from_halves(a: u16, b: u16) -> Option<Ipv4Addr> {
        Some(Ipv4Addr::from((u32::from(a) << 16) | u32::from(b)))
    }
    match v6.segments() {
        [0x2002, a, b, ..] => from_halves(a, b),
        // NAT64 well-known prefix 64:ff9b::/96. The other RFC 6052
        // embeddings scatter the IPv4 bytes; the 2000::/3 allow-list in
        // `is_internal_target` refuses them instead.
        [0x0064, 0xff9b, 0, 0, 0, 0, a, b] => from_halves(a, b),
        // ::ffff:0:a.b.c.d (v4-translated, RFC 2765)
        [0, 0, 0, 0, 0xffff, 0, a, b] => from_halves(a, b),
        // ::ffff:a.b.c.d and ::a.b.c.d
        _ => v6.to_ipv4(),
    }
}

/// Whether an address is not a globally routable unicast destination.
///
/// Written as a refusal of everything non-global rather than a list of
/// "private" ranges, so oddities (0.0.0.0/8, CGNAT, 240/4, multicast) are
/// refused too.
fn is_internal_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                // 169.254.0.0/16, which holds the cloud metadata address
                || v4.is_link_local()
                || v4.is_multicast()
                // 0.0.0.0/8 "this host" (0.x reaches localhost on Linux); this
                // also covers `is_unspecified`, as `a >= 240` covers
                // `is_broadcast`.
                || a == 0
                || (a == 100 && (64..128).contains(&b)) // 100.64.0.0/10 CGNAT
                || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24 IETF assignments
                || (a == 198 && b & 0xfe == 18) // 198.18.0.0/15 benchmarking
                || a >= 240 // 240.0.0.0/4 reserved
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = embedded_ipv4(v6) {
                return is_internal_target(IpAddr::V4(v4));
            }
            // Only global unicast (2000::/3) is a public destination. ::1, ::,
            // fc00::/7, fe80::/10, fec0::/10, ff00::/8 and every other
            // reserved prefix fall outside it.
            let segments = v6.segments();
            segments[0] & 0xe000 != 0x2000 || segments[..2] == [0x2001, 0x0db8] // 2001:db8::/32 documentation
        }
    }
}

#[cfg(test)]
mod tests;
