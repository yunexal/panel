#![allow(deprecated)]
use bollard::Docker;
use axum::{
    routing::{get, post},
    Router,
    middleware,
};
use std::net::SocketAddr;
use std::fs;

mod models;
mod state;
mod handlers;
mod tasks;

use models::NodeConfig;
use state::NodeState;
use handlers::{
    auth::{auth_middleware, update_token_handler},
    docker::{list_containers, create_container},
    health::health_check,
    update::self_update_handler,
};
use tasks::start_heartbeat_task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    // Load env if .env file exists (optional fallback)
    dotenv::dotenv().ok(); 

    println!("Starting Yunexal Node Agent...");

    // Try to load config.yml
    let config_content = fs::read_to_string("config.yml").unwrap_or_default();
    let config: Option<NodeConfig> = serde_yaml::from_str(&config_content).ok();

    let (token, node_id, panel_url, port, ram_limit, disk_limit) = if let Some(mut cfg) = config {
        println!("Loaded configuration from config.yml");
        
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        
        // Auto-configure RAM Limit (95%)
        if cfg.ram_limit == 0 {
             let total_ram_mb = sys.total_memory() / 1024 / 1024;
             cfg.ram_limit = (total_ram_mb as f64 * 0.95) as u64;
             println!("Auto-configured RAM limit to {} MB (95% of {} MB)", cfg.ram_limit, total_ram_mb);
        }

        // Auto-configure Disk Limit (95%)
        // Note: sysinfo disk usage is usually per disk, we'll take the disk where current executable resides or root
        if cfg.disk_limit == 0 {
            let disks = sysinfo::Disks::new_with_refreshed_list();
            // Simple heuristic: Find largest available space or root
            let mut total_space_mb = 0;
            for disk in &disks {
                 if disk.mount_point() == std::path::Path::new("/") {
                     total_space_mb = disk.total_space() / 1024 / 1024;
                     break;
                 }
            }
             // Fallback if / not found
            if total_space_mb == 0 && !disks.is_empty() {
                total_space_mb = disks[0].total_space() / 1024 / 1024;
            }

            cfg.disk_limit = (total_space_mb as f64 * 0.95) as u64;
            println!("Auto-configured Disk limit to {} MB (95% of {} MB)", cfg.disk_limit, total_space_mb);
        }

        (cfg.token, cfg.node_id, cfg.panel_url, cfg.port, cfg.ram_limit, cfg.disk_limit)
    } else {
        println!("config.yml not found or invalid, falling back to environment variables");
        let token = std::env::var("APP_KEY").expect("APP_KEY environment variable must be set");
        let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| "unknown".to_string());
        let panel_url = std::env::var("PANEL_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let port = std::env::var("PORT").unwrap_or("3001".to_string()).parse().unwrap_or(3001);
        (token, node_id, panel_url, port, 0, 0)
    };

    println!("Node ID: {}", node_id);
    println!("Panel URL: {}", panel_url);
    println!("Port: {}", port);

    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()?;
    
    // Verify connection
    let version = docker.version().await?;
    println!("Connected to Docker daemon version: {:?}", version.version.unwrap_or_default());

    let state = NodeState { 
        docker,
        token: std::sync::Arc::new(tokio::sync::RwLock::new(token)),
        node_id,
        panel_url,
        port,
        ram_limit,
        disk_limit,
    };

    // Build our application with routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", get(list_containers))
        .route("/containers", post(create_container))
        .route("/update-token", post(update_token_handler))
        .route("/self-update", post(self_update_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    // Run it on configured port
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Node Agent listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    // Start heartbeat task
    tokio::spawn(start_heartbeat_task(state));

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

