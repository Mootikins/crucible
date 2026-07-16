use anyhow::Result;
use clap::{Args, Subcommand};
use crucible_core::config::{CliAppConfig, WebConfig};

use crate::web::middleware::auth::{api_key_path, generate_and_persist_key, resolve_api_key};

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
    /// under `[server]` in config.toml.
    Key {
        /// Generate a new key, replacing the current one. Remote devices
        /// must re-authenticate.
        #[arg(long)]
        rotate: bool,
    },
}

pub async fn handle(cmd: WebCommand) -> Result<()> {
    let config = CliAppConfig::load(None, None, None).unwrap_or_default();

    let web_config = config.web.clone().unwrap_or_else(|| WebConfig {
        enabled: true,
        port: 3000,
        host: "127.0.0.1".to_string(),
        static_dir: None,
        api_key: None,
    });

    let final_config = WebConfig {
        enabled: true,
        port: cmd.port.unwrap_or(web_config.port),
        host: cmd.host.unwrap_or(web_config.host),
        static_dir: cmd.static_dir.or(web_config.static_dir),
        api_key: web_config.api_key,
    };

    if let Some(WebSubcommand::Key { rotate }) = cmd.command {
        return handle_key(&final_config, rotate);
    }

    crate::common::daemon_client().await?;

    println!(
        "Starting web server on http://{}:{}",
        final_config.host, final_config.port
    );
    print_connect_urls(&final_config);

    crate::web::start_server(&final_config, &config).await?;

    Ok(())
}

fn handle_key(config: &WebConfig, rotate: bool) -> Result<()> {
    if matches!(config.api_key.as_deref(), Some("")) {
        println!("API auth is DISABLED (api_key = \"\" in [server] config).");
        println!("Remove that line to re-enable key auth for non-localhost clients.");
        return Ok(());
    }

    let key = if rotate {
        if config.api_key.is_some() {
            anyhow::bail!(
                "api_key is set explicitly in [server] config — edit config.toml to change it \
                 (--rotate only manages the generated key file)"
            );
        }
        let path = api_key_path()
            .ok_or_else(|| anyhow::anyhow!("could not resolve the config directory"))?;
        let key = generate_and_persist_key(&path)
            .ok_or_else(|| anyhow::anyhow!("failed to write {}", path.display()))?;
        println!("Rotated API key (remote devices must re-authenticate).");
        key
    } else {
        resolve_api_key(config.api_key.as_deref())
            .ok_or_else(|| anyhow::anyhow!("no API key available"))?
    };

    println!("API key: {}", key);
    println!();
    println!("Localhost needs no key. On a remote device, open:");
    println!("  http://{}:{}", host_for_url(config), config.port);
    println!("and paste the key into the sign-in prompt.");
    Ok(())
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
