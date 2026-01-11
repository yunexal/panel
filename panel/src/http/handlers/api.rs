use crate::{
    models::{HeartbeatPayload, Node},
    state::AppState,
};
use axum::{
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
};
use tracing::{error, info};
use uuid::Uuid;

pub async fn heartbeat_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<HeartbeatPayload>,
) -> StatusCode {
    // [TRACE] Entry
    info!("[TRACE] -> heartbeat_handler triggered for ID: {}", id);

    // Verify Token
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    if let Some(token) = auth_header {
        info!(
            "[TRACE] Token received: {}...",
            &token.chars().take(5).collect::<String>()
        );
        let mut node_opt: Option<Node> = None;

        // 1. Try Cache
        if let Some(manager) = &state.redis {
            let mut con = manager.clone();
            let key = format!("node:{}:cache", id);
            let cached: Result<String, _> = redis::AsyncCommands::get(&mut con, key).await;
            if let Ok(json) = cached {
                info!("[TRACE] Node found in Redis Cache");
                if let Ok(n) = serde_json::from_str::<Node>(&json) {
                    node_opt = Some(n);
                }
            } else {
                info!("[TRACE] Node NOT in Redis Cache");
            }
        }

        // 2. Fallback to Memory Cache (Avoid DB Hit)
        if node_opt.is_none() {
            let nodes_lock = state.nodes_cache.read().await;
            if let Some(nodes) = &*nodes_lock {
                if let Some(n) = nodes.iter().find(|n| n.id == id) {
                    info!("[TRACE] Node found in Memory Cache");
                    node_opt = Some(n.clone());
                }
            }
        }

        // 3. Fallback to DB
        if node_opt.is_none() {
            info!("[TRACE] Fallback to DB Lookup for node: {}", id);
            node_opt = sqlx::query_as::<_, Node>("SELECT id, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

            // Re-populate Memory Cache if found
            if let Some(ref _n) = node_opt {
                // Trigger background cache refresh?
                // For now just rely on next dashboard load to populate it.
            }

            // Cache result if found (Redis)
            if let Some(ref n) = node_opt {
                info!("[TRACE] Node found in DB, caching...");
                if let Some(manager) = &state.redis {
                    let mut con = manager.clone();
                    let key = format!("node:{}:cache", id);
                    if let Ok(json) = serde_json::to_string(n) {
                        let _: Result<(), _> =
                            redis::AsyncCommands::set_ex(&mut con, key, json, 60).await;
                    }
                }
            } else {
                info!("[TRACE] Node NOT found in DB");
            }
        }

        let mut authorized = false;
        if let Some(node) = node_opt {
            if node.token == token {
                info!("[TRACE] Token MATCH - Authorized");
                authorized = true;
                // Update Version in DB if changed
                if node.version != payload.version {
                    info!(
                        "[TRACE] Updating version from {} to {}",
                        node.version, payload.version
                    );
                    let _ = sqlx::query("UPDATE nodes SET version = $1 WHERE id = $2")
                        .bind(&payload.version)
                        .bind(id)
                        .execute(&state.db)
                        .await;

                    // Invalidate cache to force refresh on next heartbeat
                    if let Some(manager) = &state.redis {
                        let mut con = manager.clone();
                        let key = format!("node:{}:cache", id);
                        let _: Result<(), _> = redis::AsyncCommands::del(&mut con, key).await;
                    }
                }
            } else {
                error!(
                    "[TRACE] Token mismatch for node {}. Expected: {}, Got: {}",
                    id, node.token, token
                );
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
                        info!("[TRACE] Pending Token MATCH - Authorized");
                        authorized = true;
                    }
                }
            }
        }

        if !authorized {
            info!("[TRACE] Returning 401 UNAUTHORIZED");
            return StatusCode::UNAUTHORIZED;
        }
    } else {
        error!("[TRACE] Missing authorization header for node {}", id);
        return StatusCode::UNAUTHORIZED;
    }

    if let Some(manager) = &state.redis {
        let key = format!("node:{}:stats", id);
        info!("[TRACE] Writing stats to Redis Key: {}", key);
        let json = serde_json::to_string(&payload).unwrap_or_default();

        let mut con = manager.clone();
        let res: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, json, 15).await;
        match res {
            Ok(_) => info!("[TRACE] Redis Write SUCCESS"),
            Err(e) => {
                error!("[TRACE] Redis Write FAILED: {}, falling back to memory", e);
                state.heartbeats_cache.write().await.insert(id, payload);
            }
        }
    } else {
        // info!("[TRACE] Redis not available to save heartbeat for {}, using memory cache", id);
        state.heartbeats_cache.write().await.insert(id, payload);
    }
    info!("[TRACE] Returning 200 OK");
    StatusCode::OK
}
