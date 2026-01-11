use crate::models::{HeartbeatPayload, Node};
use redis::aio::ConnectionManager;
use reqwest::Client as HttpClient;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: Option<ConnectionManager>,
    pub http_client: HttpClient,
    pub panel_name: Arc<RwLock<String>>,
    pub panel_font: Arc<RwLock<String>>,
    pub panel_font_url: Arc<RwLock<String>>,
    pub nodes_cache: Arc<RwLock<Option<Vec<Node>>>>,
    pub heartbeats_cache: Arc<RwLock<HashMap<Uuid, HeartbeatPayload>>>,
}

impl AppState {
    pub async fn get_nodes(&self) -> Vec<Node> {
        // 1. Check RAM Cache
        {
            let lock = self.nodes_cache.read().await;
            if let Some(nodes) = &*lock {
                return nodes.clone();
            }
        }

        // 2. Check Redis Cache
        if let Some(manager) = &self.redis {
            let mut con = manager.clone();
            let cached: Result<String, _> =
                redis::AsyncCommands::get(&mut con, "cache:nodes").await;
            if let Ok(json) = cached {
                if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(&json) {
                    // Populate RAM
                    let mut lock = self.nodes_cache.write().await;
                    *lock = Some(nodes.clone());
                    return nodes;
                }
            }
        }

        // 3. Fetch DB
        let nodes_result = sqlx::query_as::<_, Node>("SELECT id, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes")
            .fetch_all(&self.db)
            .await;

        let nodes = nodes_result.unwrap_or_default();

        // 4. Update Caches
        // Update Redis
        if let Some(manager) = &self.redis {
            let mut con = manager.clone();
            let json = serde_json::to_string(&nodes).unwrap_or_default();
            let _: Result<(), _> =
                redis::AsyncCommands::set_ex(&mut con, "cache:nodes", json, 300).await; // 5 min TTL
        }

        // Update RAM
        let mut lock = self.nodes_cache.write().await;
        *lock = Some(nodes.clone());

        nodes
    }

    pub async fn invalidate_nodes_cache(&self) {
        // Clear RAM
        let mut lock = self.nodes_cache.write().await;
        *lock = None;

        // Clear Redis
        if let Some(manager) = &self.redis {
            let mut con = manager.clone();
            let _: Result<(), _> = redis::AsyncCommands::del(&mut con, "cache:nodes").await;
        }
    }
}
