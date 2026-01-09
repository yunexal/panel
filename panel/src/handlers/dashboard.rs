use axum::{
    extract::State,
    response::IntoResponse,
    http::HeaderMap,
};
use tracing::info;
use crate::{state::AppState, models::HeartbeatPayload};
use askama::Template;
use crate::handlers::HtmlTemplate;

#[derive(Template)]
#[template(path = "nodes.html")]
struct NodesTemplate {
    panel_name: String,
    // panel_version/execution_time unused in footer override, but kept for trait compat if needed?
    // Askama will complain if fields are missing from struct but used in template.
    // But here they are NOT used in template anymore because we override footer.
    // So we can remove them or suppress warning.
    #[allow(dead_code)]
    panel_version: String,
    #[allow(dead_code)]
    execution_time: f64,
    active_tab: String,
    nodes: Vec<NodeViewModel>,
}

struct NodeViewModel {
    #[allow(dead_code)]
    id: String,
    id_short: String,
    name: String,
    ip: String,
    port: i32,
    status_color: String,
    status_text: String,
    is_online: bool,
    cpu_usage: String,
    ram_usage: u64,
    ram_total: u64,
    uptime_formatted: String,
    version: String,
    disk_usage: u64,
    disk_total: u64,
}

pub async fn nodes_page_handler(
    State(state): State<AppState>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();

    let nodes_data = state.get_nodes().await;

    let mut view_nodes = Vec::new();

    for node in nodes_data {
        let mut status_color = "red".to_string();
        let mut status_text = "Offline".to_string();
        let mut is_online = false;
        let mut cpu_usage = "0.0".to_string();
        let mut ram_usage = 0;
        let mut ram_total = 0;
        let mut disk_usage = 0;
        let mut disk_total = 0;
        let mut uptime_formatted = "0s".to_string();
        let mut version = node.version.clone();

        let mut payload_opt: Option<HeartbeatPayload> = None;

        // 1. Try Redis
        if let Some(manager) = &state.redis {
             let mut con = manager.clone();
             let key = format!("node:{}:stats", node.id);
             // info!("[TRACE] Dashboard Querying Key: {}", key); 
             let stats_json: Result<String, _> = redis::AsyncCommands::get(&mut con, &key).await;
             
             if let Ok(json_str) = stats_json {
                 info!("[TRACE] Redis HIT for {}: {}", node.id, json_str);
                 if let Ok(payload) = serde_json::from_str::<HeartbeatPayload>(&json_str) {
                     payload_opt = Some(payload);
                 }
             }
        }

        // 2. Fallback to Memory
        if payload_opt.is_none() {
             let sub_lock = state.heartbeats_cache.read().await;
             if let Some(payload) = sub_lock.get(&node.id) {
                  // Check if timestamp is fresh (e.g. < 20 seconds old)
                  let now = chrono::Utc::now().timestamp_millis();
                  if (now - payload.timestamp) < 20000 {
                      info!("[TRACE] Memory Cache HIT for {}", node.id);
                      payload_opt = Some(payload.clone());
                  }
             }
        }

        if let Some(payload) = payload_opt {
             status_color = "green".to_string();
             status_text = "Online".to_string();
             is_online = true;
             cpu_usage = format!("{:.1}", payload.cpu_usage);
             ram_usage = payload.ram_usage / 1024 / 1024;
             ram_total = payload.ram_total / 1024 / 1024;
             disk_usage = payload.disk_usage / 1024 / 1024;
             disk_total = payload.disk_total / 1024 / 1024;
             
             // Format Uptime
             if payload.uptime < 60 {
                 uptime_formatted = format!("{}s", payload.uptime);
             } else if payload.uptime < 3600 {
                 let mins = payload.uptime / 60;
                 let secs = payload.uptime % 60;
                 uptime_formatted = format!("{}m {}s", mins, secs);
             } else {
                 let hours = payload.uptime / 3600;
                 let mins = (payload.uptime % 3600) / 60;
                 uptime_formatted = format!("{}h {}m", hours, mins);
             }

             version = payload.version;
        }
        
        view_nodes.push(NodeViewModel {
            id: node.id.clone(),
            id_short: node.id[..8].to_string(),
            name: node.name,
            ip: node.ip,
            port: node.port,
            status_color,
            status_text,
            is_online,
            cpu_usage,
            ram_usage,
            ram_total,
            uptime_formatted,
            version,
            disk_usage,
            disk_total,
        });
    }

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(NodesTemplate {
        panel_name,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        nodes: view_nodes,
    })
}
