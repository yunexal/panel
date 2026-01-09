use axum::{
    extract::{State, Form},
    response::{Html, IntoResponse},
};
use crate::state::AppState;
use std::time::Instant;
use serde::Deserialize;
use askama::Template;
use crate::handlers::HtmlTemplate;

#[derive(Template)]
#[template(path = "overview.html")]
struct OverviewTemplate {
    panel_name: String,
    panel_version: String,
    total_nodes: usize,
    online_nodes: usize,
    execution_time: f64,
    active_tab: String,
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    panel_name: String,
}

pub async fn overview_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let start_time = Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();

    let nodes = state.get_nodes().await;

    let total_nodes = nodes.len();
    let mut online_nodes = 0;
    
    if let Some(manager) = &state.redis {
        let mut con = manager.clone();
        for node in &nodes {
             let key = format!("node:{}:stats", node.id);
             let exists: Result<bool, _> = redis::AsyncCommands::exists(&mut con, &key).await;
             if let Ok(true) = exists {
                 online_nodes += 1;
             }
        }
    }

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(OverviewTemplate {
        panel_name,
        panel_version,
        total_nodes,
        online_nodes,
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

    // 1. Update In-Memory State
    {
        let mut lock = state.panel_name.write().await;
        *lock = new_name.clone();
    }
    
    // 2. Update .env File
    let env_path = std::path::Path::new(".env"); 
    if let Ok(env_content) = std::fs::read_to_string(env_path) {
        let mut new_lines = Vec::new();
        let mut updated = false;

        for line in env_content.lines() {
            if line.starts_with("PANEL_NAME=") {
                new_lines.push(format!("PANEL_NAME={}", new_name));
                updated = true;
            } else {
                new_lines.push(line.to_string());
            }
        }

        if !updated {
            new_lines.push(format!("PANEL_NAME={}", new_name));
        }

        let _ = std::fs::write(env_path, new_lines.join("\n"));
    }

    // 3. Return HTMX OOB Swaps
    Html(format!(r#"
        <div id="settings-message" hx-swap-oob="true" style="color: #28a745; margin-top: 10px; font-weight: bold;">
            Settings saved successfully!
        </div>
        <h2 id="sidebar-panel-name" hx-swap-oob="true">{}</h2>
        <input id="panel_name-input" name="panel_name" value="{}" hx-swap-oob="true" style="max-width: 400px; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; width: 100%;">
    "#, new_name, new_name))
}

