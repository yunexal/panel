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
use serde::Deserialize;
use std::net::SocketAddr;
use std::fs;

#[derive(Debug, Deserialize)]
struct NodeConfig {
    token: String,
    node_id: String,
    panel_url: String,
    port: u16,
}

#[derive(Clone)]
struct NodeState {
    docker: Docker,
    token: String,
    node_id: String,
    panel_url: String,
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
        token,
        node_id,
        panel_url,
    };

    // Build our application with routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", get(list_containers))
        .route("/containers", post(create_container))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);

    // Run it on configured port
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Node Agent listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn auth_middleware(
    State(state): State<NodeState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match headers.get("X-Access-Token") {
        Some(token) if token == state.token.as_str() => {
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
