use omnimatrix::frontend::VideohubFrontend;
use omnimatrix::matrix::DummyRouter;
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

    let dummy = Arc::new(DummyRouter::with_config(1, 16, 16));
    let videohub = VideohubFrontend::new(dummy, 0);

    videohub
        .listen("0.0.0.0:9990".parse().unwrap())
        .await
        .unwrap();
}
