use axum::{
    extract::{State, Path},
    http::StatusCode,
};
use crate::{state::AppState, models::Node};
use rand::Rng;

pub async fn rotate_token_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    // 1. Fetch node info
    let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if let Some(node) = node_opt {
        // 2. Generate new token
        let new_token: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // 3. Store pending token in Redis
        if let Some(manager) = &state.redis {
            let mut con = manager.clone();
            let key = format!("node:{}:pending_token", id);
            let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, &new_token, 60).await; // 60s TTL
        }

        // 4. Send to Node
        let url = format!("http://{}:{}/update-token", node.ip, node.port);
        let payload = serde_json::json!({ "token": new_token });

        let resp = state.http_client.post(&url)
            .header("Authorization", format!("Bearer {}", node.token))
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(res) if res.status().is_success() => {
                // Node accepted and verified the token.
                // We can now update the DB.
                
                let _ = sqlx::query("UPDATE nodes SET token = $1 WHERE id = $2::uuid")
                    .bind(&new_token)
                    .bind(&id)
                    .execute(&state.db)
                    .await;
                
                // Clear pending token
                if let Some(manager) = &state.redis {
                    let mut con = manager.clone();
                    let key = format!("node:{}:pending_token", id);
                    let _: Result<(), _> = redis::AsyncCommands::del(&mut con, key).await;
                }

                StatusCode::OK
            },
            _ => {
                // Node failed to update or verify.
                // We do NOT update the DB. The Node should have reverted.
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    } else {
        StatusCode::NOT_FOUND
    }
}
