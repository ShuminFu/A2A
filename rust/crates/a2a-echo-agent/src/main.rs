//! Serves the sample agent over the A2A JSON-RPC binding.

mod agent;

use a2a_server::{http, A2aService};
use clap::Parser;

#[derive(Parser)]
#[command(about = "A sample A2A v1.0 agent serving text tools over JSON-RPC")]
struct Args {
    /// Address to bind.
    #[arg(long, default_value = "127.0.0.1:9999")]
    bind: String,
    /// The base URL clients reach this agent on; it is what the Agent Card advertises.
    #[arg(long)]
    public_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let public_url = args
        .public_url
        .unwrap_or_else(|| format!("http://{}", args.bind));

    let service = A2aService::new(agent::agent_card(&public_url), agent::TextToolsAgent)
        .with_extended_agent_card(agent::extended_agent_card(&public_url));

    let listener = tokio::net::TcpListener::bind(&args.bind).await?;
    tracing::info!(
        "serving Text Tools on {} (agent card at {}{})",
        public_url,
        public_url,
        a2a_types::AGENT_CARD_WELL_KNOWN_PATH
    );
    axum::serve(listener, http::router(service)).await?;
    Ok(())
}
