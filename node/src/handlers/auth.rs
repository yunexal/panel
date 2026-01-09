use axum::{
    extract::{State, Request, Json},
    middleware::Next,
    response::Response,
    http::{StatusCode, HeaderMap},
};
use crate::{state::NodeState, models::{UpdateTokenRequest, HeartbeatPayload, NodeConfig}};
use std::fs;

pub async fn auth_middleware(
    State(state): State<NodeState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Allow public endpoints
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

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

pub async fn update_token_handler(
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
        disk_usage: 0,
        disk_total: 0,
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
            // We need to preserve limits if possible, but here we don't have them in state easily 
            // unless we add them to NodeState.
            // For now, let's default to 0 (Auto) if we are re-writing config, 
            // OR we should read the existing config first to preserve them.
            
            let mut current_config = if let Ok(content) = fs::read_to_string("config.yml") {
                serde_yaml::from_str::<NodeConfig>(&content).unwrap_or(NodeConfig {
                    token: "".to_string(),
                    node_id: state.node_id.clone(),
                    panel_url: state.panel_url.clone(),
                    port: state.port,
                    sftp_port: 2022,
                    ram_limit: 0,
                    disk_limit: 0,
                })
            } else {
                 NodeConfig {
                    token: "".to_string(),
                    node_id: state.node_id.clone(),
                    panel_url: state.panel_url.clone(),
                    port: state.port,
                    sftp_port: 2022,
                    ram_limit: 0, // Auto
                    disk_limit: 0, // Auto
                }
            };
            
            // Update fields
            current_config.token = new_token;
            current_config.node_id = state.node_id.clone();
            current_config.panel_url = state.panel_url.clone();
            current_config.port = state.port;
            
            if let Ok(content) = serde_yaml::to_string(&current_config) {
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
