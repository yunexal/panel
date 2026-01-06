use sqlx::FromRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[sqlx(default)]
    pub token: String,
}

#[derive(Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub node_id: String,
    pub cpu_usage: f32,
    pub ram_usage: u64,
    pub ram_total: u64,
    pub uptime: u64,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub timestamp: i64,
}

#[derive(Deserialize)]
pub struct UpdateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
}
