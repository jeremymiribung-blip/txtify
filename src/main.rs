#![allow(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use txtify::app::App;
use txtify::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    tracing::debug!(?cli, "parsed cli args");

    let app = App::from_config().map_err(|e| anyhow::anyhow!(e))?;
    app.run(cli).await.map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}

fn init_tracing() {
    let env_filter = match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => tracing_subscriber::EnvFilter::new("info"),
    };

    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);

    let result = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init();

    if let Err(e) = result {
        eprintln!("failed to init tracing: {e}");
    }
}
