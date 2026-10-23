//! Binary entrypoint for `iopool2mqtt`.

use anyhow::Context;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::EnvFilter;

use iopool2mqtt::{Bridge, Config, IopoolClient, RumqttcPublisher};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env early so RUST_LOG from .env can take effect if not already set in the environment
    let _ = dotenvy::dotenv();

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,iopool2mqtt=info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    let config = Config::from_env().context("failed to load iopool2mqtt configuration")?;
    info!(
        version = env!("CARGO_PKG_VERSION"),
        api_url = %config.iopool_api_url,
        mqtt_host = %config.mqtt_host,
        mqtt_port = config.mqtt_port,
        mqtt_base_topic = %config.mqtt_base_topic,
        ha_discovery_enabled = config.ha_discovery_enabled,
        poll_interval_secs = config.poll_interval.as_secs(),
        run_once = config.run_once,
        "Starting iopool2mqtt"
    );

    let api_client =
        IopoolClient::from_config(&config).context("failed to initialize iopool HTTP client")?;
    let mqtt_publisher = RumqttcPublisher::connect(&config)
        .await
        .context("failed to initialize MQTT publisher")?;

    let cancel = CancellationToken::new();
    let cancel_for_signals = cancel.clone();

    tokio::spawn(async move {
        shutdown_signal().await;
        cancel_for_signals.cancel();
    });

    let mut bridge = Bridge::new(config, api_client, mqtt_publisher);
    bridge
        .run(cancel)
        .await
        .context("iopool2mqtt bridge exited with an error")?;

    info!("iopool2mqtt stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sigterm.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
