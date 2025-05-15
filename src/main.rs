use omnimatrix::{backend::NDIRouter, frontend::VideohubFrontend};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt,
    prelude::*,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    info!("omnimatrix starting up!");

    let router = Arc::new(NDIRouter::new("OmniRouter", vec!["Public"], 32, 4).unwrap());
    let videohub = VideohubFrontend::new(router, 0);

    videohub
        .listen("0.0.0.0:9990".parse().unwrap())
        .await
        .unwrap();
}
