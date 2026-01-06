use axum::{
    extract::State,
    response::Html,
    http::HeaderMap,
};
use crate::{state::AppState, models::{Node, HeartbeatPayload}};

pub async fn nodes_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Html<String> {
    let start_time = std::time::Instant::now();
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");
    let panel_name = state.panel_name.read().await.clone();

    let nodes_result = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes")
        .fetch_all(&state.db)
        .await;

    let nodes = match nodes_result {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to fetch nodes: {}", e);
            vec![]
        }
    };

    let mut nodes_html = String::new();
    for node in nodes {
        // Check status via Redis first
        let mut stats_html = String::new();
        let mut status = "<span style='color:red; font-weight:bold;'>● Offline</span>".to_string();

        if let Some(manager) = &state.redis {
             let mut con = manager.clone();
             let key = format!("node:{}:stats", node.id);
             let stats_json: Result<String, _> = redis::AsyncCommands::get(&mut con, key).await;
                 if let Ok(json_str) = stats_json {
                     if let Ok(payload) = serde_json::from_str::<HeartbeatPayload>(&json_str) {
                         let now = chrono::Utc::now().timestamp_millis();
                         let latency = if payload.timestamp > 0 {
                             now - payload.timestamp
                         } else {
                             0
                         };
                         
                         let status_color = "green";
                         let status_text = "Online";

                         let version_display = if !payload.version.is_empty() {
                             format!(" <span style='color:#666; font-size:0.8em;'>(v{})</span>", payload.version)
                         } else {
                             "".to_string()
                         };
                         
                         status = format!(
                             "<span style='color:{}; font-weight:bold;'>● {}</span> <span style='font-size:0.8em; color:#666;' title='Latency'>📶 {}ms</span>", 
                             status_color, status_text, latency
                         );

                         stats_html = format!(
                             "<p>CPU: {:.1}% | RAM: {}/{} MB | Uptime: {}s{}</p>",
                             payload.cpu_usage,
                             payload.ram_usage / 1024 / 1024,
                             payload.ram_total / 1024 / 1024,
                             payload.uptime,
                             version_display
                         );
                     }
                 }
             }

        // Fallback to HTTP check if Redis failed or no data
        if status.contains("Offline") {
             let url = format!("http://{}:{}/health", node.ip, node.port);
             if let Ok(res) = state.http_client.get(&url)
                .header("Authorization", format!("Bearer {}", node.token))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await 
             {
                 if res.status().is_success() {
                     status = "<span style='color:green; font-weight:bold;'>● Online</span> <span style='font-size:0.8em; color:#666;'>(No Stats)</span>".to_string();
                 }
             }
        }

        let install_cmd = format!("curl -sSL http://{}/install/{} | sudo bash", host, node.id);
        let uninstall_cmd = format!("curl -sSL http://{}/uninstall/{} | sudo bash", host, node.id);

        nodes_html.push_str(&format!(r#"
            <div class="node-card">
                <div style="display:flex; justify-content:space-between; align-items:center;">
                    <h3>{} ({}:{})</h3>
                    <div>
                        <button onclick="rotateToken('{}')" style="margin-right:10px; cursor:pointer; background:#17a2b8; color:white; border:none; padding:0.5rem 1rem; border-radius:4px;">Rotate Key</button>
                        <a href="/nodes/{}/edit" style="margin-right:10px; text-decoration:none; color:#007bff;">Edit</a>
                        <button onclick="deleteNode('{}')" class="delete-btn">Delete</button>
                    </div>
                </div>
                <p>Status: {}</p>
                {}
                <details>
                    <summary>Install Command</summary>
                    <pre>{}</pre>
                </details>
                <details>
                    <summary>Uninstall Command</summary>
                    <pre>{}</pre>
                </details>
            </div>
        "#, node.name, node.ip, node.port, node.id, node.id, node.id, status, stats_html, install_cmd, uninstall_cmd));
    }

    let elapsed = start_time.elapsed();
    let panel_version = env!("CARGO_PKG_VERSION");

    Html(format!(r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Nodes - {}</title>
        <style>
            body {{ font-family: sans-serif; margin: 0; padding: 0; display: flex; min-height: 100vh; }}
            .sidebar {{ width: 250px; background: #333; color: white; display: flex; flex-direction: column; padding: 1rem; flex-shrink: 0; }}
            .sidebar h2 {{ margin-top: 0; padding-bottom: 1rem; border-bottom: 1px solid #555; }}
            .sidebar a {{ color: #ccc; text-decoration: none; padding: 0.75rem; display: block; border-radius: 4px; margin-bottom: 0.5rem; }}
            .sidebar a:hover, .sidebar a.active {{ background: #444; color: white; }}
            .content {{ flex-grow: 1; padding: 2rem; background: #f9f9f9; overflow-y: auto; }}
            
            .header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; }}
            .node-card {{ border: 1px solid #ddd; padding: 1rem; margin-bottom: 1rem; border-radius: 4px; background: white; }}
            .delete-btn {{ background: #ff4444; color: white; border: none; padding: 0.5rem 1rem; cursor: pointer; border-radius: 4px; }}
            .add-btn {{ background: #28a745; color: white; text-decoration: none; padding: 0.75rem 1.5rem; border-radius: 4px; }}
            pre {{ background: #f4f4f4; padding: 1rem; overflow-x: auto; }}
            .footer {{ margin-top: 2rem; color: #888; font-size: 0.8rem; text-align: right; }}
        </style>
        <script>
            async function deleteNode(id) {{
                if(confirm('Delete this node?')) {{
                    await fetch('/nodes/' + id, {{ method: 'DELETE' }});
                    window.location.reload();
                }}
            }}
            async function rotateToken(id) {{
                if(confirm('Rotate token for this node? This will update the node configuration.')) {{
                    const res = await fetch('/nodes/' + id + '/rotate-token', {{ method: 'POST' }});
                    if (res.ok) {{
                        alert('Token rotated successfully!');
                        window.location.reload();
                    }} else {{
                        alert('Failed to rotate token.');
                    }}
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
            <h2>{}</h2>
            <nav>
                <a href="/">Overview</a>
                <a href="/nodes" class="active">Nodes</a>
                <a href="/logs">Panel Logs</a>
            </nav>
        </div>
        <main class="content">
            <div class="header">
                <div style="display:flex; align-items:center; gap:10px;">
                    <button onclick="toggleMainSidebar()" style="width:40px; height:40px; display:flex; align-items:center; justify-content:center; background:white; border:1px solid #ddd; border-radius:4px; color:#333; font-size:1.2rem; cursor:pointer;">☰</button>
                    <h1>{}</h1>
                </div>
                <a href="/nodes/new" class="add-btn">Add New Node</a>
            </div>
        
            <h2>Nodes</h2>
            {}

            <div class="footer">
                {} v{}<br>
                Execution time: {:.3}ms
            </div>
        </main>
    </body>
    </html>
    "#, panel_name, panel_name, panel_name, nodes_html, panel_name, panel_version, elapsed.as_secs_f64() * 1000.0))
}
