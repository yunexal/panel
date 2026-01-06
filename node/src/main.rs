#![allow(deprecated)]
use bollard::Docker;
use bollard::container::{ListContainersOptions, CreateContainerOptions, Config as DockerConfig, StartContainerOptions};
use std::collections::HashMap;
use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::{State, Request},
    middleware::{self, Next},
    response::Response,
    http::{StatusCode, HeaderMap},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::fs;
use sysinfo::System;

#[derive(Debug, Deserialize, Serialize)]
struct NodeConfig {
    token: String,
    node_id: String,
    panel_url: String,
    port: u16,
}

#[derive(Clone)]
struct NodeState {
    docker: Docker,
    token: std::sync::Arc<tokio::sync::RwLock<String>>,
    node_id: String,
    panel_url: String,
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    // Load env if .env file exists (optional fallback)
    dotenv::dotenv().ok(); 

    println!("Starting Yunexal Node Agent...");

    // Try to load config.yml
    let config_content = fs::read_to_string("config.yml").unwrap_or_default();
    let config: Option<NodeConfig> = serde_yaml::from_str(&config_content).ok();

    let (token, node_id, panel_url, port) = if let Some(cfg) = config {
        println!("Loaded configuration from config.yml");
        (cfg.token, cfg.node_id, cfg.panel_url, cfg.port)
    } else {
        println!("config.yml not found or invalid, falling back to environment variables");
        let token = std::env::var("APP_KEY").expect("APP_KEY environment variable must be set");
        let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| "unknown".to_string());
        let panel_url = std::env::var("PANEL_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let port = std::env::var("PORT").unwrap_or("3001".to_string()).parse().unwrap_or(3001);
        (token, node_id, panel_url, port)
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
    };

    // Build our application with routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", get(list_containers))
        .route("/containers", post(create_container))
        .route("/update-token", post(update_token_handler))
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

async fn auth_middleware(
    State(state): State<NodeState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let current_token = state.token.read().await;

    match auth_header {
        Some(token) if token == *current_token => {
            drop(current_token);
            Ok(next.run(request).await)
        }
        _ => {
            // User requested Error 500 for access denial
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    }
}

async fn health_check() -> &'static str {
    "OK"
}

async fn list_containers(State(state): State<NodeState>) -> Json<Vec<String>> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    // Filter only containers managed by Yunexal
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);
    
    let options = Some(ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    });

    match state.docker.list_containers(options).await {
        Ok(containers) => {
            let names: Vec<String> = containers.into_iter().map(|c| {
                let name = c.names.unwrap_or_default().first().map(|s| s.to_string()).unwrap_or("unknown".to_string());
                let state = c.state.map(|s| format!("{:?}", s)).unwrap_or_else(|| "unknown".to_string());
                format!("{} [{}]", name, state)
            }).collect();
            Json(names)
        },
        Err(_) => Json(vec!["Error listing containers".to_string()]),
    }
}

#[derive(Deserialize)]
struct CreateContainerRequest {
    image: String,
    name: Option<String>,
}

async fn create_container(
    State(state): State<NodeState>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<String>, StatusCode> {
    let options = Some(CreateContainerOptions {
        name: payload.name.unwrap_or_else(|| format!("yunexal-{}", uuid::Uuid::new_v4())),
        platform: None,
    });

    let mut labels = HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());

    let config = DockerConfig {
        image: Some(payload.image),
        labels: Some(labels),
        ..Default::default()
    };

    match state.docker.create_container(options, config).await {
        Ok(res) => {
            // Start the container
            if let Err(e) = state.docker.start_container(&res.id, None::<StartContainerOptions<String>>).await {
                eprintln!("Failed to start container: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Ok(Json(res.id))
        },
        Err(e) => {
            eprintln!("Failed to create container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
struct HeartbeatPayload {
    node_id: String,
    cpu_usage: f32,
    ram_usage: u64,
    ram_total: u64,
    uptime: u64,
    version: String,
    timestamp: i64,
}

async fn start_heartbeat_task(state: NodeState) {
    let client = reqwest::Client::new();
    let mut sys = System::new_all();
    let version = env!("CARGO_PKG_VERSION").to_string();
    
    loop {
        sys.refresh_all();
        
        let cpu_usage = sys.global_cpu_usage();
        let ram_usage = sys.used_memory();
        let ram_total = sys.total_memory();
        let uptime = System::uptime();
        let timestamp = chrono::Utc::now().timestamp_millis();

        let payload = HeartbeatPayload {
            node_id: state.node_id.clone(),
            cpu_usage,
            ram_usage,
            ram_total,
            uptime,
            version: version.clone(),
            timestamp,
        };

        // Assuming the panel has an endpoint /api/nodes/{id}/heartbeat
        let url = format!("{}/api/nodes/{}/heartbeat", state.panel_url, state.node_id);
        let current_token = state.token.read().await;

        match client.post(&url)
            .header("Authorization", format!("Bearer {}", *current_token))
            .json(&payload)
            .send()
            .await 
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    eprintln!("Heartbeat failed with status: {}", resp.status());
                }
            },
            Err(e) => eprintln!("Failed to send heartbeat: {}", e),
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

#[derive(Deserialize)]
struct UpdateTokenRequest {
    token: String,
}

async fn update_token_handler(
    State(state): State<NodeState>,
    Json(payload): Json<UpdateTokenRequest>,
) -> Result<StatusCode, StatusCode> {
    let old_token = state.token.read().await.clone();
    let new_token = payload.token;

    // 1. Update in-memory state temporarily
    {
        let mut token_lock = state.token.write().await;
        *token_lock = new_token.clone();
    }

    // 2. Try to ping Panel with new token
    let client = reqwest::Client::new();
    let url = format!("{}/api/nodes/{}/heartbeat", state.panel_url, state.node_id);
    
    // We send a dummy heartbeat just to verify auth
    let payload = HeartbeatPayload {
        node_id: state.node_id.clone(),
        cpu_usage: 0.0,
        ram_usage: 0,
        ram_total: 0,
        uptime: 0,
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", new_token))
        .json(&payload)
        .send()
        .await;

    match resp {
        Ok(res) if res.status().is_success() => {
            // 3. Success: Write to config.yml
            let config = NodeConfig {
                token: new_token,
                node_id: state.node_id.clone(),
                panel_url: state.panel_url.clone(),
                port: state.port,
            };
            
            if let Ok(content) = serde_yaml::to_string(&config) {
                if let Err(e) = fs::write("config.yml", content) {
                    eprintln!("Failed to write config.yml: {}", e);
                    // Revert memory? Or just log error? 
                    // If we can't save, we should probably revert to avoid restart issues.
                    let mut token_lock = state.token.write().await;
                    *token_lock = old_token;
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
            
            Ok(StatusCode::OK)
        },
        _ => {
            // 4. Failure: Revert to old token
            let mut token_lock = state.token.write().await;
            *token_lock = old_token;
            Err(StatusCode::UNAUTHORIZED) // Or 500
        }
    }
}

