use axum::{
    Router,
    routing::{delete, get, post},
};
use redis::Client as RedisClient;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod http;
mod models;
mod repositories;
mod services;
mod state;

use http::handlers::{
    allocations::{
        allocations_page_handler, create_allocations_handler, delete_allocations_handler,
    },
    api::heartbeat_handler,
    auth::{self, rotate_token_handler, auth_routes},
    dashboard::nodes_page_handler,
    logs::logs_handler,
    nodes::{
        create_node_handler, create_node_page_handler, delete_node_handler, edit_node_page_handler,
        setup_node_page_handler, trigger_node_update, update_node_handler,
    },
    overview::{overview_handler, overview_stats_handler},
    runtimes::{
        create_image_handler, create_image_page_handler, create_runtime_handler,
        create_runtime_page_handler, delete_image_handler, delete_runtime_handler,
        edit_image_page_handler, edit_runtime_page_handler, import_egg_handler,
        reorder_runtimes_handler, runtimes_page_handler, update_image_handler,
        update_runtime_handler,
    },
    scripts::{install_script_handler, uninstall_script_handler},
    servers::{
        create_server_handler, create_server_page_handler, delete_server_handler,
        edit_server_page_handler, manage_server_page_handler, servers_page_handler,
        update_server_handler,
    },
};
use state::AppState;

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
                .with_ansi(false),
        )
        .init();

    tracing::info!("Starting Yunexal Panel...");

    // Initialize Database Connection
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost/yunexal".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to Postgres");

    // Ensure tables exist
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS nodes (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            token TEXT NOT NULL
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Attempt to add token column if it doesn't exist (migration hack for dev)
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS token TEXT")
        .execute(&pool)
        .await;
    let _ =
        sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS sftp_port INTEGER DEFAULT 2022")
            .execute(&pool)
            .await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS ram_limit INTEGER DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS disk_limit INTEGER DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS cpu_limit INTEGER DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS version TEXT DEFAULT ''")
        .execute(&pool)
        .await;

    // Allocations Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS allocations (
            id UUID PRIMARY KEY,
            node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            server_id UUID,
            UNIQUE(node_id, ip, port)
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Runtimes Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS runtimes (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            color TEXT DEFAULT '#007bff'
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Images Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS images (
            id UUID PRIMARY KEY,
            runtime_id UUID NOT NULL REFERENCES runtimes(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            docker_images TEXT NOT NULL,
            description TEXT,
            stop_command TEXT NOT NULL DEFAULT 'stop',
            startup_command TEXT NOT NULL DEFAULT '',
            log_config TEXT NOT NULL DEFAULT '{}',
            config_files TEXT NOT NULL DEFAULT '[]',
            start_config TEXT NOT NULL DEFAULT '{}',
            requires_port BOOLEAN NOT NULL DEFAULT TRUE
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Migrations for dev environment if table already exists (dirty hack)
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS docker_images TEXT DEFAULT ''")
            .execute(&pool)
            .await;

    // Fix conflict with old schema (singular docker_image vs plural docker_images)
    // 1. Copy old data if new column is empty
    // 2. Drop the old column to prevent NOT NULL constraints from failing inserts
    // We wrap this in a block or just run best-effort queries
    let _ = sqlx::query("UPDATE images SET docker_images = docker_image WHERE (docker_images IS NULL OR docker_images = '') AND docker_image IS NOT NULL")
        .execute(&pool)
        .await; // Might fail if docker_image doesn't exist, that's fine, we ignore result in this dev hack

    let _ = sqlx::query("ALTER TABLE images DROP COLUMN IF EXISTS docker_image")
        .execute(&pool)
        .await;

    // Copy old docker_image to docker_images if it exists (not strictly checking if col exists but trying updates)
    // Actually safe to just add columns.
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS stop_command TEXT DEFAULT 'stop'")
            .execute(&pool)
            .await;
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS startup_command TEXT DEFAULT ''")
            .execute(&pool)
            .await;
    let _ = sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS log_config TEXT DEFAULT '{}'")
        .execute(&pool)
        .await;
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS config_files TEXT DEFAULT '[]'")
            .execute(&pool)
            .await;
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS start_config TEXT DEFAULT '{}'")
            .execute(&pool)
            .await;
    let _ = sqlx::query(
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS requires_port BOOLEAN DEFAULT TRUE",
    )
    .execute(&pool)
    .await;

    // Advanced Egg Features Migrations
    let _ =
        sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS install_script TEXT DEFAULT ''")
            .execute(&pool)
            .await;
    let _ = sqlx::query(
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS install_container TEXT DEFAULT ''",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS install_entrypoint TEXT DEFAULT 'bash'",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query("ALTER TABLE images ADD COLUMN IF NOT EXISTS variables TEXT DEFAULT '[]'")
        .execute(&pool)
        .await;

    // Migration for color if it doesn't exist
    let _ =
        sqlx::query("ALTER TABLE runtimes ADD COLUMN IF NOT EXISTS color TEXT DEFAULT '#007bff'")
            .execute(&pool)
            .await;
    let _ =
        sqlx::query("ALTER TABLE runtimes ADD COLUMN IF NOT EXISTS sort_order INTEGER DEFAULT 0")
            .execute(&pool)
            .await;

    // Servers Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            owner_id TEXT NOT NULL,
            node_id UUID NOT NULL REFERENCES nodes(id),
            allocation_id UUID NOT NULL REFERENCES allocations(id),
            image_id UUID NOT NULL REFERENCES images(id),
            
            cpu_limit INTEGER DEFAULT 0,
            ram_limit INTEGER DEFAULT 0,
            disk_limit INTEGER DEFAULT 0,
            swap_limit INTEGER DEFAULT 0,
            backup_limit INTEGER DEFAULT 0,
            
            io_weight INTEGER DEFAULT 500,
            oom_killer BOOLEAN DEFAULT FALSE,
            docker_image TEXT NOT NULL,
            startup_command TEXT NOT NULL,
            
            cpu_pinning TEXT,
            status TEXT DEFAULT 'installing',
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Users Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            permissions TEXT DEFAULT '{}',
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Migration: allocations can be optional for servers
    let _ = sqlx::query("ALTER TABLE servers ALTER COLUMN allocation_id DROP NOT NULL")
        .execute(&pool)
        .await;

    // Sessions Table
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id UUID PRIMARY KEY,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TIMESTAMPTZ NOT NULL
        )
    "#,
    )
    .execute(&pool)
    .await;

    // Check if we need to seed Admin user
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap_or((0,));

    if user_count.0 == 0 {
        tracing::info!("No users found. Creating default 'admin' user...");
        let admin_id = uuid::Uuid::new_v4();
        let password = "admin";
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

        let _ = sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role, permissions, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(admin_id)
        .bind("admin")
        .bind("admin@localhost")
        .bind(hash)
        .bind("admin")
        .bind("{}")
        .bind(chrono::Utc::now())
        .execute(&pool)
        .await
        .expect("Failed to create admin user");

        tracing::info!(
            "Default admin user created. Username: 'admin', Password: 'admin'. PLEASE CHANGE THIS IMMEDIATELY!"
        );
    }

    // Initialize Redis
    let redis_url = std::env::var("REDIS_URL").ok();
    let redis_manager = if let Some(url) = redis_url {
        match RedisClient::open(url.clone()) {
            Ok(client) => match client.get_connection_manager().await {
                Ok(manager) => {
                    tracing::info!("Connected to Redis at {}", url);
                    Some(manager)
                }
                Err(e) => {
                    tracing::error!("Failed to connect to Redis: {}", e);
                    None
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
            std::env::var("PANEL_NAME").unwrap_or_else(|_| "Yunexal Panel".to_string()),
        )),
        panel_font: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_FONT").unwrap_or_else(|_| "Google Sans Flex".to_string()),
        )),
        panel_font_url: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_FONT_URL").unwrap_or_default(),
        )),
        nodes_cache: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        heartbeats_cache: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
    };

    // Build our application with a route
    let protected_routes = Router::new()
        .route("/", get(overview_handler))
        .route("/overview/stats", get(overview_stats_handler))
        .route(
            "/settings/update",
            post(http::handlers::overview::update_settings_handler),
        )
        .route("/nodes", get(nodes_page_handler).post(create_node_handler))
        .route("/servers", get(servers_page_handler).post(create_server_handler))
        .route("/servers/new", get(create_server_page_handler))
        .route("/servers/{id}/manage", get(manage_server_page_handler))
        .route("/servers/{id}/edit", get(edit_server_page_handler))
        .route("/servers/{id}/update", post(update_server_handler))
        .route("/servers/{id}/delete", post(delete_server_handler))
        .route(
            "/runtimes",
            get(runtimes_page_handler).post(create_runtime_handler),
        )
        .route("/runtimes/reorder", post(reorder_runtimes_handler))
        .route("/runtimes/new", get(create_runtime_page_handler))
        .route("/runtimes/{id}/edit", get(edit_runtime_page_handler))
        .route("/runtimes/{id}/update", post(update_runtime_handler))
        .route("/runtimes/{id}/images/new", get(create_image_page_handler))
        .route("/runtimes/{id}/images", post(create_image_handler))
        .route("/runtimes/{id}/images/import", post(import_egg_handler))
        .route(
            "/runtimes/{runtime_id}/images/{image_id}/edit",
            get(edit_image_page_handler),
        )
        .route(
            "/runtimes/{runtime_id}/images/{image_id}/update",
            post(update_image_handler),
        )
        .route(
            "/runtimes/{runtime_id}/images/{image_id}",
            delete(delete_image_handler),
        )
        .route("/runtimes/{id}", delete(delete_runtime_handler))
        .route("/logs", get(logs_handler))
        .route("/nodes/new", get(create_node_page_handler))
        .route("/nodes/{id}/setup", get(setup_node_page_handler))
        .route("/nodes/{id}/edit", get(edit_node_page_handler))
        .route(
            "/nodes/{id}/allocations",
            get(allocations_page_handler).post(create_allocations_handler),
        )
        .route(
            "/nodes/{id}/allocations/delete",
            post(delete_allocations_handler),
        )
        .route("/nodes/{id}/update", post(update_node_handler))
        .route("/nodes/{id}/trigger-update", post(trigger_node_update))
        .route("/nodes/{id}/rotate-token", post(rotate_token_handler))
        .route("/nodes/{id}", delete(delete_node_handler))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::auth_middleware));

    let public_routes = Router::new()
        .route("/nodes/{id}/heartbeat", post(heartbeat_handler))
        .route("/install/{id}", get(install_script_handler))
        .route("/uninstall/{id}", get(uninstall_script_handler))
        .nest("/auth", auth_routes())
        .nest_service("/assets", ServeDir::new("public/assets"));

    let app = Router::new()
        .merge(protected_routes)
        .merge(public_routes)
        .with_state(state);

    // Run it
    // Bind to 0.0.0.0 to allow external access
    let port = std::env::var("PANEL_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Panel listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

// Migration: Make allocation_id nullable
// This should be done carefully, but for this dev setup:
// We'll run it on startup.
