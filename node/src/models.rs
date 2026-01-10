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

use std::collections::HashMap;

#[derive(Deserialize)]
pub struct CreateContainerRequest {
    pub uuid: String,
    pub image: String,
    pub startup_command: String,
    pub environment: HashMap<String, String>,
    pub memory_limit: i64,
    pub swap_limit: i64,
    pub cpu_limit: i64,
    pub io_weight: u16,
    pub ports: HashMap<String, String>, // "8080/tcp" -> "8080"
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiskDetail {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub is_removable: bool,
    pub type_: String,
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
    #[serde(default)]
    pub disk_read: u64,
    #[serde(default)]
    pub disk_write: u64,
    #[serde(default)]
    pub net_rx: u64,
    #[serde(default)]
    pub net_tx: u64,
    #[serde(default)]
    pub disks: Vec<DiskDetail>,
}

#[derive(Deserialize)]
pub struct UpdateTokenRequest {
    pub token: String,
}
