use crate::http::handlers::HtmlTemplate;
use crate::models::HeartbeatPayload;
use crate::state::AppState;
use askama::Template;
use axum::{
    extract::{Form, State},
    response::{Html, IntoResponse},
};
use serde::Deserialize;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Template)]
#[template(path = "overview_stats.html")]
struct OverviewStatsTemplate {
    total_nodes: usize,
    online_nodes: usize,
    total_ram: u64,
    used_ram: u64,
    total_disk: u64,
    used_disk: u64,
    total_cpu: f32,
    disk_read_speed: u64,
    disk_write_speed: u64,
    net_rx_speed: u64,
    net_tx_speed: u64,
}

#[derive(Template)]
#[template(path = "overview.html")]
struct OverviewTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    total_nodes: usize,
    online_nodes: usize,
    total_ram: u64,
    used_ram: u64,
    total_disk: u64,
    used_disk: u64,
    total_cpu: f32,
    disk_read_speed: u64,
    disk_write_speed: u64,
    net_rx_speed: u64,
    net_tx_speed: u64,
    execution_time: f64,
    active_tab: String,
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    panel_name: String,
    panel_font: String,
    #[serde(default)]
    panel_font_url: String, // Added
}

struct CalculatedStats {
    total_nodes: usize,
    online_nodes: usize,
    total_ram: u64,
    used_ram: u64,
    total_disk: u64,
    used_disk: u64,
    total_cpu: f32,
    disk_read_speed: u64,
    disk_write_speed: u64,
    net_rx_speed: u64,
    net_tx_speed: u64,
}

async fn calculate_overview_stats(state: &AppState) -> CalculatedStats {
    let nodes = state.get_nodes().await;

    let total_nodes = nodes.len();
    let mut online_nodes = 0;

    let mut total_ram = 0u64;
    let mut used_ram = 0u64;
    let mut total_disk = 0u64;
    let mut used_disk = 0u64;
    let mut total_cpu = 0.0f32;
    let mut disk_read_speed = 0u64;
    let mut disk_write_speed = 0u64;
    let mut net_rx_speed = 0u64;
    let mut net_tx_speed = 0u64;

    for node in &nodes {
        let mut stats: Option<HeartbeatPayload> = None;

        // 1. Try Redis
        if let Some(manager) = &state.redis {
            let mut con = manager.clone();
            let key = format!("node:{}:stats", node.id);
            let cached: Result<String, _> = redis::AsyncCommands::get(&mut con, &key).await;
            if let Ok(json) = cached {
                if let Ok(payload) = serde_json::from_str::<HeartbeatPayload>(&json) {
                    stats = Some(payload);
                }
            }
        }

        // 2. Try Memory (if not in Redis)
        if stats.is_none() {
            let lock = state.heartbeats_cache.read().await;
            if let Some(payload) = lock.get(&node.id) {
                stats = Some(payload.clone());
            }
        }

        // 3. Process Stats if available
        if let Some(payload) = stats {
            // Check recency (e.g., within last 60 seconds)
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            // Allow some grace period
            if payload.timestamp > (now as i64 - 60) {
                online_nodes += 1;
                used_ram += payload.ram_usage;
                total_ram += payload.ram_total;
                used_disk += payload.disk_usage;
                total_disk += payload.disk_total;
                total_cpu += payload.cpu_usage;
                disk_read_speed += payload.disk_read;
                disk_write_speed += payload.disk_write;
                net_rx_speed += payload.net_rx;
                net_tx_speed += payload.net_tx;
            }
        }
    }

    CalculatedStats {
        total_nodes,
        online_nodes,
        total_ram,
        used_ram,
        total_disk,
        used_disk,
        total_cpu,
        disk_read_speed,
        disk_write_speed,
        net_rx_speed,
        net_tx_speed,
    }
}

pub async fn overview_stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = calculate_overview_stats(&state).await;

    HtmlTemplate(OverviewStatsTemplate {
        total_nodes: stats.total_nodes,
        online_nodes: stats.online_nodes,
        total_ram: stats.total_ram,
        used_ram: stats.used_ram,
        total_disk: stats.total_disk,
        used_disk: stats.used_disk,
        total_cpu: stats.total_cpu,
        disk_read_speed: stats.disk_read_speed,
        disk_write_speed: stats.disk_write_speed,
        net_rx_speed: stats.net_rx_speed,
        net_tx_speed: stats.net_tx_speed,
    })
}

pub async fn overview_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone(); // Added

    let stats = calculate_overview_stats(&state).await;

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(OverviewTemplate {
        panel_name,
        panel_font,
        panel_font_url, // Added
        panel_version,
        total_nodes: stats.total_nodes,
        online_nodes: stats.online_nodes,
        total_ram: stats.total_ram,
        used_ram: stats.used_ram,
        total_disk: stats.total_disk,
        used_disk: stats.used_disk,
        total_cpu: stats.total_cpu,
        disk_read_speed: stats.disk_read_speed,
        disk_write_speed: stats.disk_write_speed,
        net_rx_speed: stats.net_rx_speed,
        net_tx_speed: stats.net_tx_speed,
        execution_time,
        active_tab: "overview".to_string(),
    })
}

pub async fn update_settings_handler(
    State(state): State<AppState>,
    Form(payload): Form<UpdateSettingsRequest>,
) -> impl IntoResponse {
    let new_name = if payload.panel_name.trim().is_empty() {
        "Yunexal Panel".to_string()
    } else {
        payload.panel_name.trim().to_string()
    };

    let new_font = if payload.panel_font.trim().is_empty() {
        "Google Sans Flex".to_string()
    } else {
        payload.panel_font.trim().to_string()
    };

    let new_font_url = payload.panel_font_url.trim().to_string();

    // 1. Update In-Memory State
    {
        let mut lock = state.panel_name.write().await;
        *lock = new_name.clone();
    }
    {
        let mut lock = state.panel_font.write().await;
        *lock = new_font.clone();
    }
    {
        let mut lock = state.panel_font_url.write().await;
        *lock = new_font_url.clone();
    }

    // 2. Update .env File
    let env_path = std::path::Path::new(".env");
    if let Ok(env_content) = std::fs::read_to_string(env_path) {
        let mut new_lines = Vec::new();
        let mut name_updated = false;
        let mut font_updated = false;
        let mut font_url_updated = false;

        for line in env_content.lines() {
            if line.starts_with("PANEL_NAME=") {
                new_lines.push(format!("PANEL_NAME={}", new_name));
                name_updated = true;
            } else if line.starts_with("PANEL_FONT=") {
                new_lines.push(format!("PANEL_FONT={}", new_font));
                font_updated = true;
            } else if line.starts_with("PANEL_FONT_URL=") {
                new_lines.push(format!("PANEL_FONT_URL={}", new_font_url));
                font_url_updated = true;
            } else {
                new_lines.push(line.to_string());
            }
        }

        if !name_updated {
            new_lines.push(format!("PANEL_NAME={}", new_name));
        }
        if !font_updated {
            new_lines.push(format!("PANEL_FONT={}", new_font));
        }
        if !font_url_updated {
            new_lines.push(format!("PANEL_FONT_URL={}", new_font_url));
        }

        let _ = std::fs::write(env_path, new_lines.join("\n"));
    }

    // 3. Return HTMX OOB Swaps
    Html(format!(
        r#"
        <div id="settings-message" hx-swap-oob="true" style="color: #28a745; margin-top: 10px; font-weight: bold;">
            Settings saved successfully!
        </div>
        <h2 id="sidebar-panel-name" hx-swap-oob="true">{}</h2>
        <input id="panel_name-input" name="panel_name" value="{}" hx-swap-oob="true" style="max-width: 400px; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; width: 100%;">
    "#,
        new_name, new_name
    ))
}
