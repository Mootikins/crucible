use anyhow::Result;
use clap::Args;
use crucible_config::{CliAppConfig, WebConfig};

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
}

pub async fn handle(cmd: WebCommand) -> Result<()> {
    let config = CliAppConfig::load(None, None, None).unwrap_or_default();

    let web_config = config.web.unwrap_or_else(|| WebConfig {
        enabled: true,
        port: 3000,
        host: "127.0.0.1".to_string(),
        static_dir: None,
    });

    let final_config = WebConfig {
        enabled: true,
        port: cmd.port.unwrap_or(web_config.port),
        host: cmd.host.unwrap_or(web_config.host),
        static_dir: cmd.static_dir.or(web_config.static_dir),
    };

    crate::commands::daemon::ensure_daemon().await?;

    println!(
        "Starting web server on http://{}:{}",
        final_config.host, final_config.port
    );

    crucible_web::start_server(&final_config).await?;

    Ok(())
}
