use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;
use std::fmt::Display;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Runtime {
    pub id: String,
    pub name: String,
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub color: Option<String>,
    #[sqlx(default)]
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Image {
    pub id: String,
    pub runtime_id: String,
    pub name: String,
    pub docker_images: String, // multiline or json
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub stop_command: String,
    #[sqlx(default)]
    pub startup_command: String,
    #[sqlx(default)]
    pub log_config: String, // json
    #[sqlx(default)]
    pub config_files: String, // json
    #[sqlx(default)]
    pub start_config: String, // json
    #[sqlx(default)]
    pub requires_port: bool,
    #[sqlx(default)]
    pub install_script: String,
    #[sqlx(default)]
    pub install_container: String,
    #[sqlx(default)]
    pub install_entrypoint: String,
    #[sqlx(default)]
    pub variables: String, // json array of Variable struct
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Variable {
    pub name: String,
    pub description: String,
    pub env_variable: String,
    pub default_value: String,
    pub user_viewable: bool,
    pub user_editable: bool,
    pub rules: String,
    pub field_type: String, // text, boolean, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Allocation {
    pub id: String,
    pub node_id: String,
    pub ip: String,
    pub port: i32,
    pub server_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateAllocationRequest {
    pub ip: String,
    pub ports: String,
}

#[derive(Deserialize)]
pub struct DeleteAllocationRequest {
    pub ports: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,        // "admin", "user"
    pub permissions: Option<String>, // JSON or comma-separated
    pub created_at: DateTime<Utc>,
}

impl User {
    #[allow(dead_code)]
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[sqlx(default)]
    pub token: String,
    #[sqlx(default)]
    pub sftp_port: i32,
    #[sqlx(default)]
    pub ram_limit: i32,
    #[sqlx(default)]
    pub disk_limit: i32,
    #[sqlx(default)]
    pub cpu_limit: i32,
    #[sqlx(default)]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Server {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String, // Placeholder for now
    pub node_id: Uuid,
    pub allocation_id: Option<Uuid>,
    pub image_id: Uuid,

    // Limits
    pub cpu_limit: i32,
    pub ram_limit: i32,
    pub disk_limit: i32,
    pub swap_limit: i32,
    pub backup_limit: i32,

    // Config
    pub io_weight: i32,
    pub oom_killer: bool,
    pub docker_image: String,
    pub startup_command: String,

    // Advanced
    pub cpu_pinning: Option<String>,
    pub status: String, // installing, running, stopped, etc.
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateServerRequest {
    // Core
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub start_on_install: Option<String>, // Checkbox sends "on" or nothing

    // Allocations
    pub node_id: Option<String>,
    pub default_allocation: Option<String>, // Allocation ID
    pub additional_ports: Option<String>,

    // Feature Limits
    pub backup_limit: Option<i32>,

    // Resource Management
    pub cpu_limit: Option<i32>,
    pub cpu_pinning: Option<String>,
    pub ram_limit: Option<i32>,
    pub swap_limit: Option<i32>,
    pub disk_limit: Option<i32>,
    pub io_weight: Option<i32>,
    pub oom_killer: Option<String>, // Checkbox

    // Image
    pub runtime_id: String,
    pub image_id: String,

    // Docker
    pub docker_image: Option<String>,
    pub custom_docker_image: Option<String>,

    // Startup
    pub startup_command: Option<String>,
    // We'll handle variables as a dynamic map or just raw fields in the handler
    // But for strict typing, we might just grab them from form directly if possible,
    // or use a wrapper. For now, let's assume we handle them dynamically or add a field if needed.
}

#[derive(Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(default = "default_sftp_port")]
    pub sftp_port: i32,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub allocation_ports: Option<String>,
}

fn default_sftp_port() -> i32 {
    2022
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskDetail {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub is_removable: bool,
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub node_id: String,
    pub cpu_usage: f32,
    pub ram_usage: u64,
    pub ram_total: u64,
    #[serde(default)]
    pub disk_usage: u64,
    #[serde(default)]
    pub disk_total: u64,
    pub uptime: u64,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
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
pub struct UpdateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(default = "default_sftp_port")]
    pub sftp_port: i32,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
}

fn empty_string_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) if !s.is_empty() => s.parse::<T>().map(Some).map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}


#[derive(Deserialize)]
pub struct UpdateServerRequest {
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    
    // Limits
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub swap_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub backup_limit: Option<i32>,

    // Config
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub io_weight: Option<i32>,
    pub oom_killer: Option<String>, // "on"
    pub docker_image: String,
    pub startup_command: String,
}

#[derive(Deserialize)]
pub struct DeleteServerRequest {
    pub force: Option<String>,
}
