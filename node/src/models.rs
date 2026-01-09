use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct NodeConfig {
    pub token: String,
    pub node_id: String,
    pub panel_url: String,
    pub port: u16,
    #[serde(default)]
    pub sftp_port: u16,
    #[serde(default)]
    pub ram_limit: u64, // In MB
    #[serde(default)]
    pub disk_limit: u64, // In MB
}

#[derive(Deserialize)]
pub struct CreateContainerRequest {
    pub image: String,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct HeartbeatPayload {
    pub node_id: String,
    pub cpu_usage: f32,
    pub ram_usage: u64,
    pub ram_total: u64,
    pub disk_usage: u64,
    pub disk_total: u64,
    pub uptime: u64,
    pub version: String,
    pub timestamp: i64,
}

#[derive(Deserialize)]
pub struct UpdateTokenRequest {
    pub token: String,
}
