use anyhow::Result;
use clap::{Args, Subcommand};
use crucible_core::config::{CliAppConfig, WebConfig};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crucible_web::middleware::auth::{api_key_path, generate_and_persist_key, resolve_api_key};

#[derive(Args)]
pub struct WebCommand {
    /// Port to listen on (overrides config)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Host to bind to (overrides config)
    #[arg(long)]
    pub host: Option<String>,

    /// Directory containing static assets (overrides config)
    #[arg(long)]
    pub static_dir: Option<String>,

    /// Allow AUTHENTICATED non-localhost clients to use the terminal/shell
    /// (overrides `[web] remote_shell` in config; requires an API key).
    #[arg(long)]
    pub remote_shell: bool,

    #[command(subcommand)]
    pub command: Option<WebSubcommand>,
}

#[derive(Subcommand)]
pub enum WebSubcommand {
    /// Show (or rotate) the API key remote clients need.
    ///
    /// Localhost requests never need the key. Non-localhost clients must
    /// present it — open the printed URL once on the remote device, or paste
    /// the key into the web UI's Settings → API Access. Disable auth
    /// entirely (NOT recommended on a 0.0.0.0 bind) with `api_key = ""`
    /// under `[web]` in config.toml.
    Key {
        /// Write a new key to the key file. A server that is already running
        /// keeps serving the OLD key until it is restarted.
        #[arg(long)]
        rotate: bool,
    },

    /// Mint the shared secret a webhook sender signs its deliveries with.
    ///
    /// Writes `[webhooks.<name>]` into `~/.config/crucible/webhooks.toml` at
    /// mode 0600, leaving any other entries alone. A webhook with no secret
    /// refuses every delivery, so this is the only way to open one.
    Webhook {
        /// Webhook name. This is the URL path segment: `/api/webhook/<name>`.
        name: String,

        /// Replace the existing secret. Without it, minting over a configured
        /// webhook is refused rather than silently invalidating its sender.
        #[arg(long)]
        rotate: bool,
    },
}

pub async fn handle(cmd: WebCommand, standalone: bool) -> Result<()> {
    let config = CliAppConfig::load(None, None, None).unwrap_or_default();

    // `..default()` on both, so a new security-relevant `WebConfig` field
    // (registration_roots, allowed_hosts, …) is carried through instead of
    // failing the build or, worse, being silently dropped here.
    let web_config = config.web.clone().unwrap_or(WebConfig {
        enabled: true,
        ..WebConfig::default()
    });

    let final_config = WebConfig {
        enabled: true,
        port: cmd.port.unwrap_or(web_config.port),
        host: cmd.host.unwrap_or_else(|| web_config.host.clone()),
        static_dir: cmd.static_dir.or_else(|| web_config.static_dir.clone()),
        remote_shell: cmd.remote_shell || web_config.remote_shell,
        ..web_config
    };

    match cmd.command {
        Some(WebSubcommand::Key { rotate }) => return handle_key(&final_config, rotate),
        Some(WebSubcommand::Webhook { name, rotate }) => {
            return handle_webhook_secret(&final_config, &name, rotate)
        }
        None => {}
    }

    crate::common::daemon_client().await?;

    println!(
        "{}",
        banner(
            &final_config,
            &crucible_web::middleware::auth::local_names(),
            outbound_ip()
        )
    );

    crucible_web::start_server(&final_config, &config, standalone).await?;

    Ok(())
}

fn handle_key(config: &WebConfig, rotate: bool) -> Result<()> {
    if matches!(config.api_key.as_deref(), Some("")) {
        println!("API auth is DISABLED (api_key = \"\" in [web] config).");
        println!("Remove that line to re-enable key auth for non-localhost clients.");
        return Ok(());
    }

    let (key, notice) = if rotate {
        if config.api_key.is_some() {
            anyhow::bail!(
                "api_key is set explicitly in [web] config — edit config.toml to change it \
                 (--rotate only manages the generated key file)"
            );
        }
        let path = api_key_path()
            .ok_or_else(|| anyhow::anyhow!("could not resolve the config directory"))?;
        let key = generate_and_persist_key(&path)
            .ok_or_else(|| anyhow::anyhow!("failed to write {}", path.display()))?;
        let notice = rotation_notice(running_server_addr(config).as_deref());
        (key, Some(notice))
    } else {
        let key = resolve_api_key(config.api_key.as_deref())
            .ok_or_else(|| anyhow::anyhow!("no API key available"))?;
        (key, None)
    };

    println!("API key: {}", key);
    println!();
    println!("Localhost needs no key. On a remote device, open:");
    println!("  http://{}:{}", host_for_url(config), config.port);
    println!("and paste the key into the sign-in prompt.");

    // Last, so it is the thing still on screen when the operator stops reading.
    if let Some(notice) = notice {
        println!();
        println!("{notice}");
    }
    Ok(())
}

/// How long to wait for the probe in [`running_server_addr`]. Generous for a
/// loopback connect, short enough that `cru web key --rotate` never feels hung
/// on a host where the port silently blackholes.
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// The address of a server already listening where `cru web` would bind, if
/// there is one.
///
/// A wildcard bind is probed on loopback — the one address the same machine can
/// always reach its own server on. A refusal, a timeout, or a name that does not
/// resolve all mean "nothing found", which downgrades the wording of
/// [`rotation_notice`] but never makes it claim more than it knows.
fn running_server_addr(config: &WebConfig) -> Option<String> {
    let probe_host = match config.host.as_str() {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        other => other,
    };
    let addr = (probe_host, config.port).to_socket_addrs().ok()?.next()?;
    TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).ok()?;
    Some(addr.to_string())
}

fn handle_webhook_secret(config: &WebConfig, name: &str, rotate: bool) -> Result<()> {
    let path = crucible_daemon::webhook::default_secrets_path()
        .ok_or_else(|| anyhow::anyhow!("could not resolve the config directory"))?;
    let secret = crucible_daemon::webhook::mint_secret(&path, name, rotate)?;

    println!("Webhook:  {name}");
    println!("Secret:   {secret}");
    println!("Stored:   {} (mode 0600)", path.display());
    println!();
    println!("Sign each delivery with HMAC-SHA256 under that secret, in either shape:");
    println!("  X-Crucible-Signature: t=<unix>,v1=<hex>   over \"<t>.<raw body>\"  (preferred)");
    println!("  X-Hub-Signature-256:  sha256=<hex>        over the raw body       (GitHub)");
    println!("Deliveries must be Content-Type: application/json.");
    println!();
    // Same read-once property as the API key, and the same failure mode if it
    // goes unsaid: the operator mints a secret, the sender starts signing with
    // it, and every delivery 401s against a server still holding the old file.
    println!(
        "{}",
        match running_server_addr(config) {
            Some(addr) => format!(
                "A web server is LISTENING on {addr}. It read webhooks.toml once at\n\
                 startup, so restart it before the sender uses this secret."
            ),
            None => "Any `cru web` started before now read webhooks.toml at startup;\n\
                     restart it before the sender uses this secret."
                .to_string(),
        }
    );
    Ok(())
}

/// What rotation actually did.
///
/// `--rotate` writes the key file; it does not reach into a running server. The
/// server resolves the key once, at startup, and `bearer_auth` compares against
/// that in-memory string — and `SessionStore` binds every browser session to the
/// same string, so outstanding sessions are not retired either. The result is
/// the exact inverse of what this used to print ("remote devices must
/// re-authenticate"): the old key keeps working and the new one is rejected
/// until the process is restarted. Say that, in those terms, rather than let an
/// operator believe they have locked a device out.
fn rotation_notice(listening_on: Option<&str>) -> String {
    let found = match listening_on {
        Some(addr) => format!(
            "A web server is LISTENING on {addr}, and it is still serving\n\
             the OLD key: it read the key file once at startup and never\n\
             re-reads it."
        ),
        None => "Any `cru web` started before now is still serving the OLD key: it\n\
                 read the key file once at startup and never re-reads it."
            .to_string(),
    };
    format!(
        "Rotated the key in the key file — but NOT in any running server.\n\
         {found}\n\
         \n\
         RESTART the server for this rotation to take effect. Until then the old\n\
         key still works, the new key above is REJECTED, and browser sessions\n\
         minted from the old key stay signed in."
    )
}

/// The startup banner: what the socket is bound to, then the URLs that reach
/// it. The API key is deliberately NOT embedded in any of them — query-string
/// tokens leak through browser history, server logs, and referrers. Remote
/// devices sign in once via the in-UI prompt (POST /api/auth/login → HttpOnly
/// session cookie).
///
/// Two rules, both of which the banner this replaced broke. It names the
/// socket before it names any URL: `Starting web server on http://0.0.0.0:3000`
/// above `Local: http://localhost:3000` reads as a loopback bind, and that
/// reading is how a working wildcard bind gets reported as "binds to
/// localhost". And it prints only URLs that reach this process AND satisfy the
/// Host guard — a single-interface bind has nothing listening on localhost, and
/// a URL the guard would 403 sends the operator hunting the bind instead of the
/// allow-list. `local_names` is the same list [`HostPolicy`] admits, so the two
/// cannot drift.
fn banner(config: &WebConfig, local_names: &[String], outbound: Option<IpAddr>) -> String {
    let (host, port) = (config.host.as_str(), config.port);
    let wildcard = matches!(host, "0.0.0.0" | "::");
    let loopback_bind = matches!(host, "127.0.0.1" | "localhost" | "::1");

    let interfaces = match (host, wildcard, loopback_bind) {
        ("0.0.0.0", ..) => "every IPv4 interface on this machine",
        (_, true, _) => "every interface on this machine",
        (_, _, true) => "this machine only",
        _ => "that interface only",
    };
    let mut lines = vec![format!("Listening on {host}:{port} — {interfaces}")];

    if wildcard || loopback_bind {
        lines.push(format!("  Local:  http://localhost:{port}"));
    }

    let mut remote = Vec::new();
    if !loopback_bind {
        match (wildcard, outbound) {
            // A wildcard bind has no address of its own to print; the outbound
            // probe supplies one, and when it cannot, the names below stand
            // alone rather than a placeholder standing in for a URL.
            (true, Some(ip)) => remote.push(format!("http://{ip}:{port}")),
            (true, None) => {}
            (false, _) => remote.push(format!("http://{host}:{port}")),
        }
        remote.extend(
            local_names
                .iter()
                .map(|name| format!("http://{name}:{port}")),
        );
        remote.dedup();
    }
    for (i, url) in remote.iter().enumerate() {
        lines.push(match i {
            0 => format!("  LAN:    {url}"),
            _ => format!("          {url}"),
        });
    }
    if !remote.is_empty() {
        lines.push(match resolve_api_key(config.api_key.as_deref()) {
            Some(_) => {
                "  Clients off this machine sign in with the key from `cru web key`.".to_string()
            }
            None => "  WARNING: API auth is disabled — anything that can reach this port \
                     has full access."
                .to_string(),
        });
    }
    lines.join("\n")
}

/// Best-effort address another device can reach this machine on: the primary
/// outbound IP, discovered by a UDP connect that sends no packets.
fn outbound_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// The host to put in a URL for a remote device: the outbound address for a
/// wildcard bind, this machine's own name if that probe found nothing, and
/// otherwise whatever was bound.
fn host_for_url(config: &WebConfig) -> String {
    if !matches!(config.host.as_str(), "0.0.0.0" | "::") {
        return config.host.clone();
    }
    outbound_ip()
        .map(|ip| ip.to_string())
        .or_else(|| {
            crucible_web::middleware::auth::local_names()
                .into_iter()
                .next()
        })
        .unwrap_or_else(|| config.host.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn config_bound_to(host: &str, port: u16) -> WebConfig {
        WebConfig {
            enabled: true,
            host: host.to_string(),
            port,
            ..WebConfig::default()
        }
    }

    /// A listener on an ephemeral loopback port, and the port it took.
    fn listening_on_a_free_port() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn a_running_server_is_detected_on_the_address_cru_web_would_bind() {
        let (listener, port) = listening_on_a_free_port();

        assert_eq!(
            running_server_addr(&config_bound_to("127.0.0.1", port)).as_deref(),
            Some(format!("127.0.0.1:{port}").as_str())
        );
        // A wildcard bind has no single address; loopback is the one the same
        // machine can always reach its own server on.
        assert_eq!(
            running_server_addr(&config_bound_to("0.0.0.0", port)).as_deref(),
            Some(format!("127.0.0.1:{port}").as_str())
        );

        drop(listener);
        assert_eq!(
            running_server_addr(&config_bound_to("127.0.0.1", port)),
            None,
            "a closed port is not a running server"
        );
    }

    /// Every URL the banner prints, as `host:port`.
    fn advertised_authorities(banner: &str) -> Vec<String> {
        banner
            .lines()
            .filter_map(|line| line.split("http://").nth(1))
            .map(|rest| {
                rest.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('/')
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_banner_never_says_localhost_for_a_bind_that_does_not_include_it() {
        // The line that started this: `Starting web server on http://0.0.0.0:3000`
        // followed by `Local: http://localhost:3000` reads as "bound to
        // localhost", and for a single-interface bind the localhost URL is not
        // merely misleading — nothing is listening there.
        let banner = banner(
            &config_bound_to("192.168.0.16", 3000),
            &["impulse".to_string()],
            None,
        );

        assert!(
            !banner.contains("localhost"),
            "nothing answers on localhost for this bind: {banner}"
        );
        assert!(banner.contains("192.168.0.16:3000"), "{banner}");
    }

    #[test]
    fn the_banner_states_the_bind_before_any_url() {
        let wildcard = banner(&config_bound_to("0.0.0.0", 3000), &[], None);
        let first = wildcard.lines().next().unwrap();
        assert!(first.contains("0.0.0.0:3000"), "{first}");
        assert!(
            first.contains("every IPv4 interface"),
            "a wildcard bind has to name what it opened: {first}"
        );

        let loopback = banner(&config_bound_to("127.0.0.1", 3000), &[], None);
        let first = loopback.lines().next().unwrap();
        assert!(first.contains("127.0.0.1:3000"), "{first}");
        assert!(first.contains("this machine only"), "{first}");
    }

    #[test]
    fn a_loopback_bind_advertises_nothing_off_this_machine() {
        let banner = banner(
            &config_bound_to("127.0.0.1", 3000),
            &["impulse".to_string()],
            Some("192.168.0.16".parse().unwrap()),
        );
        assert_eq!(advertised_authorities(&banner), ["localhost:3000"]);
    }

    #[test]
    fn a_wildcard_bind_advertises_the_lan_address_and_the_machines_names() {
        let banner = banner(
            &config_bound_to("0.0.0.0", 3000),
            &["impulse".to_string(), "impulse.local".to_string()],
            Some("192.168.0.16".parse().unwrap()),
        );
        assert_eq!(
            advertised_authorities(&banner),
            [
                "localhost:3000",
                "192.168.0.16:3000",
                "impulse:3000",
                "impulse.local:3000"
            ]
        );
    }

    #[test]
    fn the_banner_only_advertises_urls_the_host_policy_accepts() {
        // The invariant that keeps this honest: a printed URL that the Host
        // guard would 403 is worse than no URL at all, because it sends the
        // operator hunting the bind. Both derive from the same names.
        let config = WebConfig {
            allowed_hosts: vec!["crucible.example.com".to_string()],
            ..config_bound_to("0.0.0.0", 3000)
        };
        let names = crucible_web::middleware::auth::local_names();
        let policy = crucible_web::middleware::auth::HostPolicy::from_bind_with_local_names(
            &config.host,
            config.port,
            &config.allowed_hosts,
            &names,
        );
        let banner = banner(&config, &names, Some("192.168.0.16".parse().unwrap()));

        for authority in advertised_authorities(&banner) {
            assert!(
                policy.answers_to(&authority),
                "banner advertises {authority}, which the Host guard refuses:\n{banner}"
            );
        }
    }

    #[test]
    fn an_undiscoverable_outbound_address_drops_the_line_rather_than_guessing() {
        // No route to the internet: the old banner printed `http://<this-host>:3000`.
        let banner = banner(
            &config_bound_to("0.0.0.0", 3000),
            &["impulse".to_string()],
            None,
        );
        assert!(!banner.contains('<'), "{banner}");
        assert_eq!(
            advertised_authorities(&banner),
            ["localhost:3000", "impulse:3000"]
        );
    }

    #[test]
    fn the_rotation_notice_says_the_old_key_still_works_until_a_restart() {
        // The message this replaced was the exact inverse of the truth: the
        // live process holds the key it resolved at startup, so after
        // `--rotate` the OLD key is the one that still works and the NEW one is
        // refused. Both wordings — server found or not — must say so.
        for listening_on in [None, Some("127.0.0.1:3000")] {
            let notice = rotation_notice(listening_on);
            let lower = notice.to_lowercase();

            assert!(lower.contains("restart"), "notice: {notice}");
            assert!(lower.contains("old key"), "notice: {notice}");
            assert!(
                lower.contains("rejected"),
                "the notice must say the new key does not work yet: {notice}"
            );
            assert!(
                !lower.contains("must re-authenticate"),
                "rotation does not make remote devices re-authenticate: {notice}"
            );
        }
    }

    #[test]
    fn the_rotation_notice_names_the_server_it_found_listening() {
        let notice = rotation_notice(Some("127.0.0.1:3000"));
        assert!(notice.contains("127.0.0.1:3000"), "notice: {notice}");
        assert!(notice.contains("LISTENING"), "notice: {notice}");

        // With nothing found we may not claim a server exists — only that one
        // started earlier would be stale.
        let unknown = rotation_notice(None);
        assert!(!unknown.contains("LISTENING"), "notice: {unknown}");
    }
}
