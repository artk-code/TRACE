use std::env;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "trace_server=info,tower_http=info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let addr = env::var("TRACE_SERVER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse::<SocketAddr>()?;
    let root = env::var("TRACE_ROOT").unwrap_or_else(|_| ".".to_string());

    trace_server::serve(addr, root).await?;
    Ok(())
}
