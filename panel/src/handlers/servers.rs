use axum::{
    extract::State,
    response::IntoResponse, // Changed
};
use crate::state::AppState;
use askama::Template; // Added
use crate::handlers::HtmlTemplate;

#[derive(Template)]
#[template(path = "servers.html")]
struct ServersTemplate {
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    has_nodes: bool,
}

pub async fn servers_page_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    
    // Use cached nodes
    let nodes = state.get_nodes().await;
    let has_nodes = !nodes.is_empty();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(ServersTemplate {
        panel_name,
        panel_version,
        execution_time,
        active_tab: "servers".to_string(),
        has_nodes,
    })
}
