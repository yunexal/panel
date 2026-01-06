use axum::{
    extract::{State, Query},
    response::Html,
};
use crate::state::AppState;
use std::fs;
use std::path::Path;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LogsQuery {
    file: Option<String>,
    raw: Option<String>, // "true"
}

pub async fn logs_handler(
    State(_state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> axum::response::Response {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION");
    
    // Read logs
    let logs_dir = Path::new("logs");
    let mut log_content = String::from("");
    let mut file_list_html = String::new();
    let mut current_file = String::new();
    let mut is_latest = false;

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
            
            // Sort to get the latest (reverse order usually better for UI)
            files.sort();
            files.reverse();

            // Identify the real latest file
            let real_latest = files.first().cloned().unwrap_or_default();

            // Handle "latest.log" virtual file or default
            let requested_file = query.file.clone().unwrap_or_else(|| "latest.log".to_string());
            
            if requested_file == "latest.log" {
                current_file = "latest.log".to_string();
                is_latest = true;
                // Read from real latest
                if !real_latest.is_empty() {
                    // Logic below reads from `target_path`
                }
            } else {
                current_file = requested_file.clone();
            }

            // Build file list HTML
            file_list_html.push_str("<h3>Log Files</h3>");
            
            // Add Virtual Latest
            let latest_active = if is_latest { "active" } else { "" };
            file_list_html.push_str(&format!(
                "<a href='/logs?file=latest.log' class='file-link {}'>latest.log (Live)</a>", 
                latest_active
            ));

            for file in &files {
                let active_class = if file == &current_file && !is_latest { "active" } else { "" };
                
                // Format display name: panel.log.YYYY-MM-DD -> DD.MM.YYYY
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

                file_list_html.push_str(&format!(
                    "<a href='/logs?file={}' class='file-link {}'>{}</a>", 
                    file, active_class, display_name
                ));
            }

            // Determine which physical file to read
            let file_to_read = if is_latest { &real_latest } else { &current_file };

            // Read content
            if !file_to_read.is_empty() {
                let file_path = logs_dir.join(file_to_read);
                // Security check
                if file_path.starts_with(logs_dir) && file_path.exists() {
                    if let Ok(content) = fs::read_to_string(file_path) {
                        for line in content.lines() {
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
                                "log-info" // Default
                            };

                            let escaped = line.replace("&", "&amp;")
                                            .replace("<", "&lt;")
                                            .replace(">", "&gt;");
                            
                            log_content.push_str(&format!("<div class='log-entry {}'>{}</div>", level_class, escaped));
                        }
                    } else {
                         log_content = "<div class='log-entry log-error'>Failed to read file</div>".to_string();
                    }
                } else {
                    log_content = "<div class='log-entry log-error'>Invalid file path</div>".to_string();
                }
            } else {
                log_content = "<div class='log-entry log-info'>No logs available</div>".to_string();
            }
        }
    }

    // Check if raw request (for AJAX polling)
    if let Some(raw) = query.raw {
        if raw == "true" {
            use axum::response::IntoResponse;
            return Html(log_content).into_response();
        }
    }

    let refresh_script = if is_latest {
        r#"
        <script>
            let isAutoScroll = true;
            
            // Allow user to disable auto-scroll
            document.getElementById('log-container').addEventListener('scroll', function() {
                const div = this;
                if (div.scrollHeight - div.scrollTop - div.clientHeight > 50) {
                   isAutoScroll = false;
                } else {
                   isAutoScroll = true;
                }
            });

            setInterval(async () => {
                try {
                    const res = await fetch('/logs?file=latest.log&raw=true');
                    if (res.ok) {
                        const html = await res.text();
                        const container = document.getElementById('log-container');
                        
                        // Simple content replacement (could be optimized)
                        // Preserve scroll if not near bottom
                        const wasAtBottom = isAutoScroll;
                        
                        // We need to re-apply filter logic if we replace innerHTML
                        // But wait, filter logic is just CSS classes + display:none.
                        // If we replace innerHTML, we lose the 'style' attribute set by JS.
                        // So we should re-run filter.
                        const currentFilter = document.querySelector('.filter-btn.active').getAttribute('data-level');
                        
                        container.innerHTML = html;
                        filterLogs(currentFilter);

                        if (wasAtBottom) {
                            container.scrollTop = container.scrollHeight;
                        }
                    }
                } catch (e) {
                    console.error('Failed to fetch logs', e);
                }
            }, 2000);
        </script>
        "#
    } else {
        ""
    };

    let logs_html = format!(r#"
        <div class="logs-layout" style="display: flex; gap: 1rem; flex-grow: 1; min-height: 0;">
            <div class="logs-main" style="flex-grow: 1; display: flex; flex-direction: column; min-height: 0;">
                <div class="filters" style="margin-bottom: 0.5rem; display: flex; gap: 0.5rem; flex-shrink: 0; align-items: center;">
                    <button class="filter-btn active" data-level="all" onclick="filterLogs('all')">All</button>
                    <button class="filter-btn" data-level="log-info" onclick="filterLogs('log-info')">INFO</button>
                    <button class="filter-btn" data-level="log-warn" onclick="filterLogs('log-warn')">WARN</button>
                    <button class="filter-btn" data-level="log-error" onclick="filterLogs('log-error')">ERROR</button>
                    <button class="filter-btn" data-level="log-debug" onclick="filterLogs('log-debug')">DEBUG</button>
                    <button class="filter-btn" onclick="toggleSidebar()" style="margin-left: auto;">Toggle Files</button>
                    {}
                </div>
                <div id="log-container" class="log-container" style="background: #1e1e1e; color: #d4d4d4; padding: 1rem; border-radius: 4px; flex-grow: 1; overflow-y: auto; font-family: monospace;">
                    {}
                </div>
            </div>
            <div id="logs-sidebar" class="logs-sidebar" style="width: 200px; background: #fff; padding: 1rem; border-radius: 4px; border: 1px solid #ddd; overflow-y: auto; flex-shrink: 0;">
                {}
            </div>
        </div>
        {}
    "#, 
    if is_latest { "<span style='margin-left:0.5rem; color:green; font-weight:bold;'>● Live</span>" } else { "" },
    log_content, file_list_html, refresh_script);
    
    let elapsed = start_time.elapsed();

    use axum::response::IntoResponse;
    Html(format!(r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Yunexal Panel - Logs</title>
        <style>
            body {{ font-family: sans-serif; margin: 0; padding: 0; display: flex; height: 100vh; overflow: hidden; }}
            .sidebar {{ width: 250px; background: #333; color: white; display: flex; flex-direction: column; padding: 1rem; flex-shrink: 0; }}
            .sidebar h2 {{ margin-top: 0; padding-bottom: 1rem; border-bottom: 1px solid #555; }}
            .sidebar a {{ color: #ccc; text-decoration: none; padding: 0.75rem; display: block; border-radius: 4px; margin-bottom: 0.5rem; }}
            .sidebar a:hover, .sidebar a.active {{ background: #444; color: white; }}
            .content {{ flex-grow: 1; padding: 2rem; background: #f9f9f9; display: flex; flex-direction: column; height: 100%; box-sizing: border-box; overflow: hidden; }}
            
            .header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; flex-shrink: 0; }}
            .footer {{ margin-top: 1rem; color: #888; font-size: 0.8rem; text-align: right; flex-shrink: 0; }}

            /* Logs Specific */
            .log-entry {{ padding: 2px 0; border-bottom: 1px solid #333; white-space: pre-wrap; word-break: break-all; }}
            .log-error {{ color: #ff6b6b; }}
            .log-warn {{ color: #feca57; }}
            .log-info {{ color: #54a0ff; }}
            .log-debug {{ color: #1dd1a1; }}
            .log-trace {{ color: #c8d6e5; }}
            
            .file-link {{ display: block; padding: 0.5rem; color: #333; text-decoration: none; margin-bottom: 0.2rem; border-radius: 3px; font-size: 0.9em; }}
            .file-link:hover {{ background: #eee; }}
            .file-link.active {{ background: #007bff; color: white; }}

            .filter-btn {{ padding: 0.25rem 0.75rem; border: 1px solid #ccc; background: white; cursor: pointer; border-radius: 3px; font-size: 0.9rem; }}
            .filter-btn:hover {{ background: #eee; }}
            .filter-btn.active {{ background: #007bff; color: white; border-color: #007bff; }}
        </style>
        <script>
            function filterLogs(level) {{
                // Update buttons
                document.querySelectorAll('.filter-btn').forEach(btn => btn.classList.remove('active'));
                document.querySelector(`.filter-btn[data-level="${{level}}"]`).classList.add('active');

                // Filter entries
                const entries = document.querySelectorAll('.log-entry');
                entries.forEach(entry => {{
                    if (level === 'all') {{
                        entry.style.display = 'block';
                    }} else {{
                        if (entry.classList.contains(level)) {{
                            entry.style.display = 'block';
                        }} else {{
                            entry.style.display = 'none';
                        }}
                    }}
                }});
            }}

            // Auto scroll to bottom of container on load
            document.addEventListener("DOMContentLoaded", function() {{
                const container = document.getElementById('log-container');
                if (container) {{
                    container.scrollTop = container.scrollHeight;
                }}
            }});

            function toggleSidebar() {{
                const sidebar = document.getElementById('logs-sidebar');
                if (sidebar.style.display === 'none') {{
                    sidebar.style.display = 'block';
                }} else {{
                    sidebar.style.display = 'none';
                }}
            }}

            function toggleMainSidebar() {{
                const sidebar = document.getElementById('main-sidebar');
                if (sidebar.style.display === 'none') {{
                    sidebar.style.display = 'flex';
                }} else {{
                    sidebar.style.display = 'none';
                }}
            }}
        </script>
    </head>
    <body>
        <div id="main-sidebar" class="sidebar">
            <h2>Yunexal</h2>
            <nav>
                <a href="/">Overview</a>
                <a href="/nodes">Nodes</a>
                <a href="/logs" class="active">Panel Logs</a>
            </nav>
        </div>
        <main class="content">
            <div class="header">
                 <div style="display:flex; align-items:center; gap:10px;">
                    <button onclick="toggleMainSidebar()" style="width:40px; height:40px; display:flex; align-items:center; justify-content:center; background:white; border:1px solid #ddd; border-radius:4px; color:#333; font-size:1.2rem; cursor:pointer;">☰</button>
                    <h1>System Logs <span style="font-size: 0.6em; color: #666; font-weight: normal;">({})</span></h1>
                 </div>
            </div>
            
            {}

            <div class="footer">
                Yunexal Panel v{}<br>
                Execution time: {:.3}ms
            </div>
        </main>
    </body>
    </html>
    "#, 
    if is_latest { "latest.log" } else { &current_file }, 
    logs_html, panel_version, elapsed.as_secs_f64() * 1000.0)).into_response()
}
