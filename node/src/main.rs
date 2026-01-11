#![allow(deprecated)]
mod grpc;
mod handlers;
mod models;
mod state;
mod tasks;

use axum::{
    Router, middleware,
    routing::{delete, get, post},
};
use bollard::Docker;
use std::fs;
use std::net::SocketAddr;
use tracing::{error, info, warn};
use tracing_subscriber;

use grpc::MyNodeService;
use grpc::node::node_service_server::NodeServiceServer;
use handlers::{
    auth::{auth_middleware, update_token_handler},
    docker::{create_container, delete_container, list_containers},
    health::health_check,
    update::self_update_handler,
};
use models::NodeConfig;
use state::NodeState;
use tasks::start_heartbeat_task;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load env if .env file exists (optional fallback)
    dotenv::dotenv().ok();

    info!("Starting Yunexal Node Agent...");

    // Try to load config.yml
    let config_content = fs::read_to_string("config.yml").unwrap_or_default();
    let config: Option<NodeConfig> = serde_yaml::from_str(&config_content).ok();

    let (token, node_id, panel_url, port, ram_limit, disk_limit) = if let Some(mut cfg) = config {
        info!("Loaded configuration from config.yml");

        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        // Auto-configure RAM Limit (95%)
        if cfg.ram_limit == 0 {
            let total_ram_mb = sys.total_memory() / 1024 / 1024;
            cfg.ram_limit = (total_ram_mb as f64 * 0.95) as u64;
            info!(
                "Auto-configured RAM limit to {:.2} GB (95% of {:.2} GB)",
                cfg.ram_limit as f64 / 1024.0,
                total_ram_mb as f64 / 1024.0
            );
        }

        // Auto-configure Disk Limit (95%)
        if cfg.disk_limit == 0 {
            let disks = sysinfo::Disks::new_with_refreshed_list();
            let mut total_space_mb = 0;
            for disk in &disks {
                if disk.mount_point() == std::path::Path::new("/") {
                    total_space_mb = disk.total_space() / 1024 / 1024;
                    break;
                }
            }
            if total_space_mb == 0 && !disks.is_empty() {
                total_space_mb = disks[0].total_space() / 1024 / 1024;
            }

            cfg.disk_limit = (total_space_mb as f64 * 0.95) as u64;
            info!(
                "Auto-configured Disk limit to {:.2} GB (95% of {:.2} GB)",
                cfg.disk_limit as f64 / 1024.0,
                total_space_mb as f64 / 1024.0
            );
        }

        (
            cfg.token,
            cfg.node_id,
            cfg.panel_url,
            cfg.port,
            cfg.ram_limit,
            cfg.disk_limit,
        )
    } else {
        warn!("config.yml not found or invalid, falling back to environment variables");
        dotenvy::dotenv().ok();
        let token = std::env::var("APP_KEY").expect("APP_KEY environment variable must be set");
        let node_id = std::env::var("NODE_ID").expect("NODE_ID environment variable must be set");
        let panel_url = std::env::var("PANEL_URL").expect("PANEL_URL environment variable must be set");
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "3001".to_string())
            .parse()
            .expect("PORT environment variable must be a valid number");
        (token, node_id, panel_url, port, 0, 0)
    };

    info!("Node ID: {}", node_id);
    info!("Panel URL: {}", panel_url);
    info!("Port: {}", port);

    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()?;

    let state = NodeState {
        docker,
        token: std::sync::Arc::new(tokio::sync::RwLock::new(token)),
        node_id,
        panel_url,
        port,
        ram_limit,
        disk_limit,
    };

    // 1. Start gRPC Server (Primary)
    let grpc_addr = format!("0.0.0.0:{}", port).parse()?;
    let grpc_service = MyNodeService {
        state: state.clone(),
    };

    info!("Starting gRPC server on {}", grpc_addr);
    let grpc_server = Server::builder()
        .accept_http1(true)
        .add_service(tonic_web::enable(NodeServiceServer::new(grpc_service)))
        .serve(grpc_addr);

    // 2. Start Axum Server (Compatibility/Heartbeat/HTTP APIs)
    // We run it on port + 1
    let http_port = port + 1;
    let http_addr = SocketAddr::from(([0, 0, 0, 0], http_port));

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", get(list_containers))
        .route("/containers", post(create_container))
        .route("/containers/:uuid", delete(delete_container))
        .route("/update-token", post(update_token_handler))
        .route("/self-update", post(self_update_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    info!("Starting HTTP server (Axum) on http://{}", http_addr);
    let http_listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
    let http_server = axum::serve(http_listener, app);

    // 3. Start Heartbeat Task
    tokio::spawn(start_heartbeat_task(state));

    // Run both servers concurrently
    tokio::select! {
        res = grpc_server => {
            if let Err(e) = res {
                error!("gRPC server error: {}", e);
            }
        }
        res = http_server => {
            if let Err(e) = res {
                error!("HTTP server error: {}", e);
            }
        }
    }

    Ok(())
}
