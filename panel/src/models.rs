use sqlx::FromRow;
use serde::{Deserialize, Serialize, Deserializer};
use std::str::FromStr;
use std::fmt::Display;

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

fn default_sftp_port() -> i32 { 2022 }

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
        Some(s) if !s.is_empty() => s
            .parse::<T>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}
