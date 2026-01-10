use crate::http::handlers::HtmlTemplate;
use crate::state::AppState;
use askama::Template;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tracing::info;

#[derive(Deserialize)]
pub struct LogsQuery {
    file: Option<String>,
    raw: Option<String>, // "true"
}

struct LogFile {
    name: String,
    display_name: String,
    active: bool,
}

#[derive(Template)]
#[template(path = "logs.html")]
struct LogsTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    current_file: String,
    is_latest: bool,
    has_logs: bool,
    log_content: String,
    file_list: Vec<LogFile>,
}

pub async fn logs_handler(
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> Response {
    info!("[TRACE] -> logs_handler triggered");

    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    // Read logs
    let logs_dir = Path::new("logs");
    info!("[TRACE] Reading logs from directory: {:?}", logs_dir);

    let mut log_content = String::from("");
    let mut file_list = Vec::new();
    let mut current_file = String::new();
    let mut is_latest = false;
    let mut found_logs = false;

    if logs_dir.exists() {
        if let Ok(entries) = fs::read_dir(logs_dir) {
            let mut files: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    if let Some(name) = e.file_name().to_str() {
                        if name.starts_with("panel.log") {
                            return Some(name.to_string());
                        }
                    }
                    None
                })
                .collect();

            files.sort();
            files.reverse();

            let real_latest = files.first().cloned().unwrap_or_default();

            // Handle "latest.log" virtual file or default
            let requested_file = query
                .file
                .clone()
                .unwrap_or_else(|| "latest.log".to_string());

            if requested_file == "latest.log" {
                current_file = "latest.log".to_string();
                is_latest = true;
            } else {
                current_file = requested_file.clone();
            }

            // Determine which physical file to read
            let file_to_read = if is_latest {
                &real_latest
            } else {
                &current_file
            };

            // Raw mode
            if query.raw.as_deref() == Some("true") {
                let path = logs_dir.join(file_to_read);
                if let Ok(content) = fs::read_to_string(path) {
                    return axum::response::Response::builder()
                        .header("Content-Type", "text/plain")
                        .body(axum::body::Body::from(content))
                        .unwrap();
                }
            }

            // Read content
            if !file_to_read.is_empty() {
                let path = logs_dir.join(file_to_read);
                if path.exists() {
                    if let Ok(content) = fs::read_to_string(path) {
                        // Apply syntax highlighting logic here or in template?
                        // For Askama template, it's safer to pass pre-processed HTML or rely on client-side JS.
                        // But the previous implementation logic was server-side class processing.
                        // Let's keep it server-side for now to match feature parity.

                        let mut processed_content = String::new();
                        let mut lines: Vec<&str> = content.lines().collect();
                        lines.reverse();

                        for line in lines {
                            let lower = line.to_lowercase();
                            let level_class = if lower.contains("error") {
                                "log-error"
                            } else if lower.contains("warn") {
                                "log-warn"
                            } else if lower.contains("info") {
                                "log-info"
                            } else if lower.contains("debug") {
                                "log-debug"
                            } else if lower.contains("trace") {
                                "log-trace"
                            } else {
                                "log-info"
                            };

                            let escaped = line
                                .replace("&", "&amp;")
                                .replace("<", "&lt;")
                                .replace(">", "&gt;");

                            processed_content.push_str(&format!(
                                "<div class='log-entry {}'>{}</div>",
                                level_class, escaped
                            ));
                        }
                        log_content = processed_content;
                        found_logs = true;
                    }
                } else {
                    log_content =
                        "<div class='log-entry log-error'>Log file not found.</div>".to_string();
                }
            } else {
                log_content = "<div class='log-entry log-info'>No logs found.</div>".to_string();
            }

            // Build file list
            file_list.push(LogFile {
                name: "latest.log".to_string(),
                display_name: "latest.log (Live)".to_string(),
                active: is_latest, // "active" field in struct
            });

            for file in files {
                let display_name = if let Some(date_part) = file.strip_prefix("panel.log.") {
                    let parts: Vec<&str> = date_part.split('-').collect();
                    if parts.len() == 3 {
                        format!("{}.{}.{}", parts[2], parts[1], parts[0])
                    } else {
                        file.clone()
                    }
                } else {
                    file.clone()
                };

                file_list.push(LogFile {
                    name: file.clone(),
                    display_name,
                    active: file == current_file && !is_latest,
                });
            }
        }
    } else {
        log_content =
            "<div class='log-entry log-error'>Logs directory not found.</div>".to_string();
        current_file = "Error".to_string();
    }

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(LogsTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "logs".to_string(),
        current_file,
        is_latest,
        has_logs: found_logs,
        log_content,
        file_list,
    })
    .into_response()
}
