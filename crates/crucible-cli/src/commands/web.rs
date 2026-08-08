use anyhow::Result;
use clap::{Args, Subcommand};
use crucible_core::config::{CliAppConfig, WebConfig};
use std::net::{TcpStream, ToSocketAddrs};
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
        "Starting web server on http://{}:{}",
        final_config.host, final_config.port
    );
    print_connect_urls(&final_config);

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

/// Print ready-to-open URLs after startup. The key is deliberately NOT
/// embedded in the URL — query-string tokens leak through browser history,
/// server logs, and referrers. Remote devices sign in once via the in-UI
/// prompt (POST /api/auth/login → HttpOnly session cookie).
fn print_connect_urls(config: &WebConfig) {
    println!("  Local:  http://localhost:{}", config.port);

    if !binds_remote(&config.host) {
        return;
    }
    match resolve_api_key(config.api_key.as_deref()) {
        Some(_) => println!(
            "  Remote: http://{}:{}  (sign in with the key from `cru web key`)",
            host_for_url(config),
            config.port
        ),
        None => println!(
            "  Remote: http://{}:{}  (WARNING: API auth disabled)",
            host_for_url(config),
            config.port
        ),
    }
}

fn binds_remote(host: &str) -> bool {
    !matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Best-effort address other devices can reach: for wildcard binds, discover
/// the primary outbound IP (UDP connect sends no packets); otherwise use the
/// configured host.
fn host_for_url(config: &WebConfig) -> String {
    if config.host == "0.0.0.0" || config.host == "::" {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("1.1.1.1:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        return "<this-host>".to_string();
    }
    config.host.clone()
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
