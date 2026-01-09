use axum::{
    routing::{get, post, delete},
    Router,
};
use std::net::SocketAddr;
use sqlx::postgres::PgPoolOptions;
use redis::Client as RedisClient;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod models;
mod state;
mod handlers;

use state::AppState;
use handlers::{
    dashboard::nodes_page_handler,
    overview::overview_handler,
    nodes::{create_node_page_handler, create_node_handler, setup_node_page_handler, edit_node_page_handler, update_node_handler, delete_node_handler, trigger_node_update},
    allocations::{allocations_page_handler, create_allocations_handler, delete_allocations_handler},
    auth::rotate_token_handler,
    api::heartbeat_handler,
    scripts::{install_script_handler, uninstall_script_handler},
    logs::logs_handler,
    servers::servers_page_handler,
};

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    // Initialize Logging
    let file_appender = tracing_appender::rolling::daily("logs", "panel.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
        )
        .init();

    tracing::info!("Starting Yunexal Panel...");

    // Initialize Database Connection
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:password@localhost/yunexal".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to Postgres");

    // Ensure tables exist
    let _ = sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS nodes (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            token TEXT NOT NULL
        )
    "#)
    .execute(&pool)
    .await;
    
    // Attempt to add token column if it doesn't exist (migration hack for dev)
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS token TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS sftp_port INTEGER DEFAULT 2022").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS ram_limit INTEGER DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS disk_limit INTEGER DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS cpu_limit INTEGER DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS version TEXT DEFAULT ''").execute(&pool).await;

    // Allocations Table
    let _ = sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS allocations (
            id UUID PRIMARY KEY,
            node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            server_id UUID,
            UNIQUE(node_id, ip, port)
        )
    "#)
    .execute(&pool)
    .await;


    // Initialize Redis
    let redis_url = std::env::var("REDIS_URL").ok();
    let redis_manager = if let Some(url) = redis_url {
        match RedisClient::open(url.clone()) {
            Ok(client) => {
                match client.get_connection_manager().await {
                    Ok(manager) => {
                        tracing::info!("Connected to Redis at {}", url);
                        Some(manager)
                    },
                    Err(e) => {
                        tracing::error!("Failed to connect to Redis: {}", e);
                        None
                    }
                }
            },
            Err(e) => {
                tracing::error!("Failed to open Redis client: {}", e);
                None
            }
        }
    } else {
        tracing::warn!("REDIS_URL not set, running without Redis cache.");
        None
    };

    let state = AppState {
        db: pool,
        redis: redis_manager,
        http_client: reqwest::Client::new(),
        panel_name: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_NAME").unwrap_or_else(|_| "Yunexal Panel".to_string())
        )),
        nodes_cache: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        heartbeats_cache: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    };

    // Build our application with a route
    let app = Router::new()
        .route("/", get(overview_handler))
        .route("/settings/update", post(handlers::overview::update_settings_handler))
        .route("/nodes", get(nodes_page_handler).post(create_node_handler))
        .route("/servers", get(servers_page_handler))
        .route("/logs", get(logs_handler))
        .route("/nodes/new", get(create_node_page_handler))
        .route("/nodes/{id}/setup", get(setup_node_page_handler))
        .route("/nodes/{id}/edit", get(edit_node_page_handler))
        .route("/nodes/{id}/allocations", get(allocations_page_handler).post(create_allocations_handler))
        .route("/nodes/{id}/allocations/delete", post(delete_allocations_handler))
        .route("/nodes/{id}/update", post(update_node_handler))
        .route("/nodes/{id}/trigger-update", post(trigger_node_update))
        .route("/nodes/{id}/rotate-token", post(rotate_token_handler))
        .route("/nodes/{id}", delete(delete_node_handler))
        .route("/nodes/{id}/heartbeat", post(heartbeat_handler))
        .route("/install/{id}", get(install_script_handler))
        .route("/uninstall/{id}", get(uninstall_script_handler))
        .nest_service("/downloads", ServeDir::new("public"))
        .with_state(state);

    // Run it
    // Bind to 0.0.0.0 to allow external access
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Panel listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}