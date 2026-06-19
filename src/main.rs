use insight::pb::service::insight::insight_service_server::InsightServiceServer;
use insight::handler::InsightHandler;
use insight::manager::biz::InsightBiz;
use insight::manager::client::{BudgetClient, EntryClient};
use insight::manager::repository::InsightRepository;
use axum::{routing::get, Json, Router};
use std::{net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let rust_log = std::env::var("RUST_LOG").ok();
    philand_logging::init("insight", rust_log.as_deref().or(Some("insight=debug")));

    let app_info = philand_application::from_env_with_prefix("INSIGHT_APP");
    tracing::info!("starting {}", app_info.user_agent());

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let grpc_host = std::env::var("GRPC_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let grpc_port: u16 = std::env::var("GRPC_PORT")
        .unwrap_or_else(|_| "50108".to_string())
        .parse()?;
    let http_host = std::env::var("HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let http_port: u16 = std::env::var("HTTP_PORT")
        .unwrap_or_else(|_| "9108".to_string())
        .parse()?;
    let entry_url =
        std::env::var("ENTRY_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:50105".to_string());
    let budget_url =
        std::env::var("BUDGET_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:50103".to_string());

    let entry_client = EntryClient::connect(&entry_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to entry gRPC: {e}"))?;
    tracing::info!("Entry gRPC client connected to {}", entry_url);

    let budget_client = BudgetClient::connect(&budget_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to budget gRPC: {e}"))?;
    tracing::info!("Budget gRPC client connected to {}", budget_url);

    let repo = InsightRepository::new(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to init repository: {e}"))?;
    tracing::info!("Storage initialized");

    let biz = Arc::new(InsightBiz::new(Arc::new(repo), entry_client, budget_client));
    let grpc_handler = InsightHandler::new(biz);

    let grpc_addr: SocketAddr = format!("{grpc_host}:{grpc_port}").parse()?;
    let grpc_server = tonic::transport::Server::builder()
        .add_service(InsightServiceServer::new(grpc_handler))
        .serve(grpc_addr);
    tracing::info!("gRPC server listening on {}", grpc_addr);

    let http_addr: SocketAddr = format!("{http_host}:{http_port}").parse()?;
    let http_app = Router::new().route("/health", get(health_check));
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("HTTP server listening on {}", http_addr);

    tokio::select! {
        res = grpc_server => { if let Err(e) = res { tracing::error!("gRPC error: {}", e); } }
        res = axum::serve(http_listener, http_app) => { if let Err(e) = res { tracing::error!("HTTP error: {}", e); } }
    }
    Ok(())
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "insight" }))
}