use axum::{
    extract::{State, Form},
    response::{Html, Redirect},
};
use crate::{state::AppState, models::Node};
use std::time::Instant;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    panel_name: String,
}

pub async fn overview_handler(
    State(state): State<AppState>,
) -> Html<String> {
    let start_time = Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION");
    let panel_name = state.panel_name.read().await.clone();

    let nodes = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes")
        .fetch_all(&state.db)
        .await
        .unwrap_or(vec![]);

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

    Html(format!(r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Overview - {}</title>
        <style>
            body {{ font-family: sans-serif; margin: 0; padding: 0; display: flex; height: 100vh; overflow: hidden; }}
            .sidebar {{ width: 250px; background: #333; color: white; display: flex; flex-direction: column; padding: 1rem; flex-shrink: 0; }}
            .sidebar h2 {{ margin-top: 0; padding-bottom: 1rem; border-bottom: 1px solid #555; }}
            .sidebar a {{ color: #ccc; text-decoration: none; padding: 0.75rem; display: block; border-radius: 4px; margin-bottom: 0.5rem; }}
            .sidebar a:hover, .sidebar a.active {{ background: #444; color: white; }}
            .content {{ flex-grow: 1; padding: 2rem; background: #f9f9f9; display: flex; flex-direction: column; height: 100%; box-sizing: border-box; overflow-y: auto; }}
            
            .header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }}
            
            .stats-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem; margin-bottom: 2rem; }}
            .stat-card {{ background: white; padding: 1.5rem; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); text-align: center; }}
            .stat-value {{ font-size: 2.5rem; font-weight: bold; color: #333; }}
            .stat-label {{ color: #666; margin-top: 0.5rem; text-transform: uppercase; font-size: 0.8rem; letter-spacing: 1px; }}
            
            .section-card {{ background: white; padding: 1.5rem; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); margin-bottom: 2rem; }}
            .form-group {{ margin-bottom: 1rem; }}
            label {{ display: block; margin-bottom: 0.5rem; font-weight: bold; }}
            input[type="text"] {{ width: 100%; max-width: 400px; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; }}
            button {{ background: #007bff; color: white; border: none; padding: 0.5rem 1rem; border-radius: 4px; cursor: pointer; }}
            
            .footer {{ margin-top: auto; color: #888; font-size: 0.8rem; text-align: right; }}
        </style>
        <script>
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
            <h2>{}</h2>
            <nav>
                <a href="/" class="active">Overview</a>
                <a href="/nodes">Nodes</a>
                <a href="/logs">Panel Logs</a>
            </nav>
        </div>
        <main class="content">
            <div class="header">
                 <div style="display:flex; align-items:center; gap:10px;">
                    <button onclick="toggleMainSidebar()" style="width:40px; height:40px; display:flex; align-items:center; justify-content:center; background:white; border:1px solid #ddd; border-radius:4px; color:#333; font-size:1.2rem; cursor:pointer;">☰</button>
                    <h1>Overview</h1>
                 </div>
            </div>

            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Total Nodes</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value" style="color: #28a745">{}</div>
                    <div class="stat-label">Online Nodes</div>
                </div>
                <div class="stat-card">
                   <div class="stat-value" style="color: #007bff">{}</div>
                   <div class="stat-label">Panel Version</div>
                </div>
            </div>

            <div class="section-card">
                <h3>Settings</h3>
                <form action="/settings/update" method="POST">
                    <div class="form-group">
                        <label for="panel_name">Panel Name</label>
                        <input type="text" id="panel_name" name="panel_name" value="{}" placeholder="Default: Yunexal Panel">
                    </div>
                    <button type="submit">Save Settings</button>
                </form>
            </div>

            <div class="footer">
                {} v{}<br>
                Execution time: {:.3}ms
            </div>
        </main>
    </body>
    </html>
    "#, panel_name, panel_name, total_nodes, online_nodes, panel_version, panel_name, panel_name, panel_version, elapsed.as_secs_f64() * 1000.0))
}

pub async fn update_settings_handler(
    State(state): State<AppState>,
    Form(payload): Form<UpdateSettingsRequest>,
) -> Redirect {
    let new_name = if payload.panel_name.trim().is_empty() {
        "Yunexal Panel".to_string()
    } else {
        payload.panel_name.trim().to_string()
    };

    // 1. Update In-Memory State
    let mut lock = state.panel_name.write().await;
    *lock = new_name.clone();
    
    // 2. Update .env File
    // We assume .env is in the current directory or parent directory usually.
    // Based on workspace info, it's in `panel/../.env` or similar? 
    // Wait, workspace info shows .env at root `/home/nestor/Documents/vscode/yunexal-panel/.env`
    // And panel is `/home/nestor/Documents/vscode/yunexal-panel/panel`
    
    let env_path = std::path::Path::new(".env"); 
    // Try to find .env file.
    // Since we ran `cargo run -p panel`, CWD is likely the workspace root.
    
    let env_content = std::fs::read_to_string(env_path).unwrap_or_default();
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

    let new_content = new_lines.join("\n");
    if let Err(e) = std::fs::write(env_path, new_content) {
        eprintln!("Failed to write to .env file: {}", e);
    }

    Redirect::to("/")
}
