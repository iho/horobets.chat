use clap::Parser;
mod limits;
#[derive(Parser)]
struct Config {
    #[arg(long, env = "MESSENGER_DATA_DIR", default_value = "data")]
    data_dir: std::path::PathBuf,
    #[arg(long, env = "MESSENGER_LISTEN", default_value = "127.0.0.1:3000")]
    listen: std::net::SocketAddr,
    #[arg(long, env = "MESSENGER_JWT_SECRET")]
    jwt_secret: String,
    #[arg(long, default_value = "local", value_parser = ["local", "s3"])]
    blob_store: String,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let c = Config::parse();
    limits::configure();
    let mut app = messenger_server::App::open(c.data_dir, &c.jwt_secret)?;
    if c.blob_store == "s3" {
        #[cfg(feature = "s3")]
        {
            app.blobs = std::sync::Arc::new(messenger_server::storage::S3Store::from_env()?);
        }
        #[cfg(not(feature = "s3"))]
        anyhow::bail!("S3 requires building with --features s3");
    }
    // Keep the binding mutable for optional storage backends.
    let _ = &mut app;
    let listener = tokio::net::TcpListener::bind(c.listen).await?;
    tracing::info!(address=%listener.local_addr()?,"messenger listening");
    axum::serve(listener, messenger_server::api::router(app))
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("install SIGTERM handler");
                tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
        })
        .await?;
    Ok(())
}
