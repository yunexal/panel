use axum::{
    extract::{State, Path, Json},
    http::{HeaderMap, StatusCode},
};
use crate::{state::AppState, models::{Node, HeartbeatPayload}};

pub async fn heartbeat_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<HeartbeatPayload>,
) -> StatusCode {
    // Verify Token
    let auth_header = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    if let Some(token) = auth_header {
        // Check DB
        let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
            .bind(&id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

        let mut authorized = false;
        if let Some(node) = node_opt {
            if node.token == token {
                authorized = true;
            }
        }

        // Check Pending Token (if DB check failed)
        if !authorized {
            if let Some(manager) = &state.redis {
                let mut con = manager.clone();
                let key = format!("node:{}:pending_token", id);
                let pending: Result<String, _> = redis::AsyncCommands::get(&mut con, key).await;
                if let Ok(pending_token) = pending {
                    if pending_token == token {
                        authorized = true;
                    }
                }
            }
        }

        if !authorized {
            return StatusCode::UNAUTHORIZED;
        }
    } else {
        return StatusCode::UNAUTHORIZED;
    }

    if let Some(manager) = &state.redis {
        let key = format!("node:{}:stats", id);
        let json = serde_json::to_string(&payload).unwrap_or_default();
        
        let mut con = manager.clone();
        let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, json, 15).await;
    }
    StatusCode::OK
}
