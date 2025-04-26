use std::{env, sync::Arc};

use axum::{
    Router,
    routing::{any, post},
};
use futures::future::try_join_all;
use mcp_manager::{config::get_config, error_method, workspace_handler};
use tokio::{io, net::TcpListener, sync::RwLock};
use tower_http::add_extension::AddExtensionLayer;
use tracing::{Level, event};
use tracing_subscriber::EnvFilter;

const CONFIG_FILE: &str = "config.yaml";

#[tokio::main]
async fn main() -> io::Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    let config_file = env::var_os("MCP_MANAGER_CONFIG").map_or(CONFIG_FILE.to_owned(), |var| {
        var.into_string().unwrap_or(CONFIG_FILE.to_owned())
    });

    let config = get_config(&config_file)?;

    let mut futures = Vec::new();

    for (listener, config) in config.listeners {
        let router = Router::new()
            .route("/{*path}", post(workspace_handler))
            .route("/{*path}", any(error_method))
            .layer(AddExtensionLayer::new(Arc::new(RwLock::new(config))));

        event!(Level::INFO, "Starting listener {listener}");

        futures.push(
            axum::serve(
                TcpListener::bind(listener)
                    .await
                    .expect("Couldn't start listener: {listener}"),
                router,
            )
            .into_future(),
        );
    }

    try_join_all(futures).await?;

    Ok(())
}
