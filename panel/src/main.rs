use axum::{
    routing::{get, post, delete},
    Router,
    response::{Html, Redirect},
    extract::{State, Path, Form, Json},
    http::{HeaderMap, StatusCode},
};
use std::net::SocketAddr;
use sqlx::postgres::{PgPoolOptions, PgPool};
use sqlx::FromRow;
use redis::Client as RedisClient;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use rand::Rng;

use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    redis: Option<RedisClient>,
    http_client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    // Initialize Database Connection
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:password@localhost/yunexal".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to Postgres");

    // Ensure tables exist
    // Note: If table exists with old schema, this might fail or ignore. 
    // For dev, we assume fresh start or manual migration.
    // We changed encrypted_token to token.
    let _ = sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS nodes (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            token TEXT NOT NULL
        )
    "#)
    .execute(&pool)
    .await;
    
    // Attempt to add token column if it doesn't exist (migration hack for dev)
    let _ = sqlx::query("ALTER TABLE nodes ADD COLUMN IF NOT EXISTS token TEXT").execute(&pool).await;


    // Initialize Redis
    let redis_url = std::env::var("REDIS_URL").ok();
    let redis_client = if let Some(url) = redis_url {
        RedisClient::open(url).ok()
    } else {
        None
    };

    let state = AppState {
        db: pool,
        redis: redis_client,
        http_client: reqwest::Client::new(),
    };

    // Build our application with a route
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/nodes/new", get(create_node_page_handler))
        .route("/nodes", post(create_node_handler))
        .route("/nodes/{id}/setup", get(setup_node_page_handler))
        .route("/nodes/{id}/edit", get(edit_node_page_handler))
        .route("/nodes/{id}/update", post(update_node_handler))
        .route("/nodes/{id}/rotate-token", post(rotate_token_handler))
        .route("/nodes/{id}", delete(delete_node_handler))
        .route("/nodes/{id}/heartbeat", post(heartbeat_handler))
        .route("/install/{id}", get(install_script_handler))
        .route("/uninstall/{id}", get(uninstall_script_handler))
        .nest_service("/downloads", ServeDir::new("public"))
        .with_state(state);

    // Run it
    // Bind to 0.0.0.0 to allow external access
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Panel listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- Handlers ---

async fn index_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Html<String> {
    let start_time = std::time::Instant::now();
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    // Fetch nodes from DB
    // We cast id to text because sqlx requires the uuid feature to map UUID columns to String automatically,
    // or we need to use Uuid type in the struct. Casting to text is the simplest fix for now.
    // We try to select 'token' if it exists, or fallback to 'encrypted_token' if migration failed, but for now let's assume 'token'
    // Actually, to be safe with existing DBs without migration, we might need to handle both, but let's assume the user will fix the DB.
    // We'll select 'token' as the field name.
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

        if let Some(client) = &state.redis {
             if let Ok(mut con) = client.get_multiplexed_async_connection().await {
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
                         
                         // Status indicator
                         // < 10s: Green, < 20s: Yellow, > 20s: Red (though Redis expires at 15s, so mostly Green/Yellow)
                         // Actually Redis expires at 15s, so if we see it, it's < 15s.
                         // Let's just use Green if present.
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
        <title>Yunexal Panel</title>
        <style>
            body {{ font-family: sans-serif; padding: 2rem; max_width: 800px; margin: 0 auto; padding-bottom: 4rem; position: relative; min-height: 100vh; box-sizing: border-box; }}
            .header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; }}
            .node-card {{ border: 1px solid #ddd; padding: 1rem; margin-bottom: 1rem; border-radius: 4px; }}
            .delete-btn {{ background: #ff4444; color: white; border: none; padding: 0.5rem 1rem; cursor: pointer; border-radius: 4px; }}
            .add-btn {{ background: #28a745; color: white; text-decoration: none; padding: 0.75rem 1.5rem; border-radius: 4px; }}
            pre {{ background: #f4f4f4; padding: 1rem; overflow-x: auto; }}
            .footer {{ position: absolute; bottom: 1rem; right: 2rem; color: #888; font-size: 0.8rem; text-align: right; }}
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
        </script>
    </head>
    <body>
        <div class="header">
            <h1>Yunexal Panel</h1>
            <a href="/nodes/new" class="add-btn">Add New Node</a>
        </div>
        
        <h2>Nodes</h2>
        {}

        <div class="footer">
            Yunexal Panel v{}<br>
            Execution time: {:.3}ms
        </div>
    </body>
    </html>
    "#, nodes_html, panel_version, elapsed.as_secs_f64() * 1000.0))
}

async fn create_node_page_handler() -> Html<&'static str> {
    Html(r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Create Node - Yunexal Panel</title>
        <style>
            body { font-family: sans-serif; padding: 2rem; max_width: 800px; margin: 0 auto; }
            form { background: #f9f9f9; padding: 2rem; border-radius: 8px; border: 1px solid #ddd; }
            .form-group { margin-bottom: 1rem; }
            label { display: block; margin-bottom: 0.5rem; font-weight: bold; }
            input { width: 100%; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; }
            button { background: #007bff; color: white; border: none; padding: 0.75rem 1.5rem; border-radius: 4px; cursor: pointer; font-size: 1rem; }
            button:hover { background: #0056b3; }
            .back-link { display: inline-block; margin-bottom: 1rem; color: #666; text-decoration: none; }
        </style>
    </head>
    <body>
        <a href="/" class="back-link">← Back to Dashboard</a>
        <h1>Add New Node</h1>
        
        <form action="/nodes" method="POST">
            <div class="form-group">
                <label for="name">Node Name</label>
                <input type="text" id="name" name="name" placeholder="e.g. Worker 1" required>
            </div>
            
            <div class="form-group">
                <label for="ip">IP Address</label>
                <input type="text" id="ip" name="ip" placeholder="e.g. 192.168.1.10" required>
            </div>
            
            <div class="form-group">
                <label for="port">Port</label>
                <input type="number" id="port" name="port" value="3001" required>
            </div>
            
            <button type="submit">Create Node</button>
        </form>
    </body>
    </html>
    "#)
}

// --- Data Structures ---

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct Node {
    id: String,
    name: String,
    ip: String,
    port: i32,
    // We use 'token' now. If DB has 'encrypted_token', we might need to handle it, 
    // but we are moving to 'token'.
    #[sqlx(default)]
    token: String,
}

#[derive(Deserialize)]
struct CreateNodeRequest {
    name: String,
    ip: String,
    port: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeartbeatPayload {
    node_id: String,
    cpu_usage: f32,
    ram_usage: u64,
    ram_total: u64,
    uptime: u64,
    #[serde(default)]
    version: String,
    #[serde(default)]
    timestamp: i64,
}

#[derive(Deserialize)]
struct UpdateNodeRequest {
    name: String,
    ip: String,
    port: i32,
}

// --- Additional Handlers ---

async fn create_node_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateNodeRequest>,
) -> Redirect {
    let id = Uuid::new_v4().to_string();
    
    // Generate a random token for the node
    let token = Uuid::new_v4().to_string();

    if let Err(e) = sqlx::query("INSERT INTO nodes (id, name, ip, port, token) VALUES ($1::uuid, $2, $3, $4, $5)")
        .bind(&id)
        .bind(&payload.name)
        .bind(&payload.ip)
        .bind(&payload.port)
        .bind(&token)
        .execute(&state.db)
        .await 
    {
        eprintln!("Failed to insert node: {}", e);
        return Redirect::to("/");
    }

    Redirect::to(&format!("/nodes/{}/setup", id))
}

async fn setup_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Html<String> {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    let node_result = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await;

    let node = match node_result {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to fetch node: {}", e);
            return Html(format!("<h1>Error fetching node: {}</h1>", e));
        }
    };

    if let Some(node) = node {
        let install_cmd = format!("curl -sSL http://{}/install/{} | sudo bash", host, node.id);
        
        Html(format!(r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Setup Node - Yunexal Panel</title>
            <style>
                body {{ font-family: sans-serif; padding: 2rem; max_width: 800px; margin: 0 auto; }}
                .container {{ background: #f9f9f9; padding: 2rem; border-radius: 8px; border: 1px solid #ddd; }}
                pre {{ background: #333; color: #fff; padding: 1rem; border-radius: 4px; overflow-x: auto; }}
                .btn {{ display: inline-block; padding: 0.75rem 1.5rem; border-radius: 4px; text-decoration: none; color: white; margin-top: 1rem; }}
                .btn-primary {{ background: #007bff; }}
                .btn-secondary {{ background: #6c757d; }}
            </style>
        </head>
        <body>
            <h1>Node Created Successfully!</h1>
            <div class="container">
                <h2>Setup Instructions for "{}"</h2>
                <p>Run the following command on your remote server ({}):</p>
                <pre>{}</pre>
                <p>This command will install Docker (if needed), configure the node agent, and start it.</p>
                
                <a href="/" class="btn btn-primary">Go to Dashboard</a>
            </div>
        </body>
        </html>
        "#, node.name, node.ip, install_cmd))
    } else {
        Html("<h1>Node not found</h1>".to_string())
    }
}

async fn delete_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    let _ = sqlx::query("DELETE FROM nodes WHERE id = $1::uuid")
        .bind(id)
        .execute(&state.db)
        .await;
    
    axum::http::StatusCode::NO_CONTENT
}

async fn edit_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Html<String> {
    let node = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();

    if let Some(node) = node {
        Html(format!(r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Edit Node - Yunexal Panel</title>
            <style>
                body {{ font-family: sans-serif; padding: 2rem; max_width: 800px; margin: 0 auto; }}
                form {{ background: #f9f9f9; padding: 2rem; border-radius: 8px; border: 1px solid #ddd; }}
                .form-group {{ margin-bottom: 1rem; }}
                label {{ display: block; margin-bottom: 0.5rem; font-weight: bold; }}
                input {{ width: 100%; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; }}
                button {{ background: #007bff; color: white; border: none; padding: 0.75rem 1.5rem; border-radius: 4px; cursor: pointer; font-size: 1rem; }}
                button:hover {{ background: #0056b3; }}
                .back-link {{ display: inline-block; margin-bottom: 1rem; color: #666; text-decoration: none; }}
            </style>
        </head>
        <body>
            <a href="/" class="back-link">← Back to Dashboard</a>
            <h1>Edit Node</h1>
            
            <form action="/nodes/{}/update" method="POST">
                <div class="form-group">
                    <label for="name">Node Name</label>
                    <input type="text" id="name" name="name" value="{}" required>
                </div>
                
                <div class="form-group">
                    <label for="ip">IP Address</label>
                    <input type="text" id="ip" name="ip" value="{}" required>
                </div>
                
                <div class="form-group">
                    <label for="port">Port</label>
                    <input type="number" id="port" name="port" value="{}" required>
                </div>
                
                <button type="submit">Save Changes</button>
            </form>
        </body>
        </html>
        "#, node.id, node.name, node.ip, node.port))
    } else {
        Html("<h1>Node not found</h1>".to_string())
    }
}

async fn update_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<UpdateNodeRequest>,
) -> Redirect {
    let _ = sqlx::query("UPDATE nodes SET name = $1, ip = $2, port = $3 WHERE id = $4::uuid")
        .bind(&payload.name)
        .bind(&payload.ip)
        .bind(&payload.port)
        .bind(&id)
        .execute(&state.db)
        .await;
    
    Redirect::to("/")
}

async fn uninstall_script_handler(
    Path(id): Path<String>,
    headers: HeaderMap,
) -> String {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    format!(r#"#!/bin/bash
echo "Uninstalling Yunexal Node..."

# Stop and disable service
if systemctl is-active --quiet yunexal-node; then
    systemctl stop yunexal-node
fi

if systemctl is-enabled --quiet yunexal-node; then
    systemctl disable yunexal-node
fi

# Remove service file
rm -f /etc/systemd/system/yunexal-node.service
systemctl daemon-reload

# Remove application directory
rm -rf /opt/yunexal-node

# Notify panel to delete node from database
echo "Notifying panel to remove node..."
curl -X DELETE http://{}/nodes/{}

echo "Node uninstalled successfully."
"#, host, id)
}

async fn heartbeat_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<HeartbeatPayload>,
) -> StatusCode {
    // Verify Token
    let auth_header = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    if let Some(token) = auth_header {
        // Check DB
        let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
            .bind(&id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

        let mut authorized = false;
        if let Some(node) = node_opt {
            if node.token == token {
                authorized = true;
            }
        }

        // Check Pending Token (if DB check failed)
        if !authorized {
            if let Some(client) = &state.redis {
                if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                    let key = format!("node:{}:pending_token", id);
                    let pending: Result<String, _> = redis::AsyncCommands::get(&mut con, key).await;
                    if let Ok(pending_token) = pending {
                        if pending_token == token {
                            authorized = true;
                            // Note: We could update DB here, but we do it in rotate_token_handler for simplicity
                            // Actually, if this is the verification ping, we should probably allow it.
                        }
                    }
                }
            }
        }

        if !authorized {
            return StatusCode::UNAUTHORIZED;
        }
    } else {
        return StatusCode::UNAUTHORIZED;
    }

    if let Some(client) = &state.redis {
        let key = format!("node:{}:stats", id);
        let json = serde_json::to_string(&payload).unwrap_or_default();
        
        match client.get_multiplexed_async_connection().await {
            Ok(mut con) => {
                let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, json, 15).await;
            },
            Err(e) => eprintln!("Failed to get redis connection: {}", e),
        }
    }
    StatusCode::OK
}

async fn rotate_token_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    // 1. Fetch node info
    let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if let Some(node) = node_opt {
        // 2. Generate new token
        let new_token: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // 3. Store pending token in Redis
        if let Some(client) = &state.redis {
            if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                let key = format!("node:{}:pending_token", id);
                let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, &new_token, 60).await; // 60s TTL
            }
        }

        // 4. Send to Node
        let url = format!("http://{}:{}/update-token", node.ip, node.port);
        let payload = serde_json::json!({ "token": new_token });

        let resp = state.http_client.post(&url)
            .header("Authorization", format!("Bearer {}", node.token))
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(res) if res.status().is_success() => {
                // Node accepted and verified the token.
                // We can now update the DB.
                
                let _ = sqlx::query("UPDATE nodes SET token = $1 WHERE id = $2::uuid")
                    .bind(&new_token)
                    .bind(&id)
                    .execute(&state.db)
                    .await;
                
                // Clear pending token
                if let Some(client) = &state.redis {
                    if let Ok(mut con) = client.get_multiplexed_async_connection().await {
                        let key = format!("node:{}:pending_token", id);
                        let _: Result<(), _> = redis::AsyncCommands::del(&mut con, key).await;
                    }
                }

                StatusCode::OK
            },
            _ => {
                // Node failed to update or verify.
                // We do NOT update the DB. The Node should have reverted.
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn install_script_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> String {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    // Fetch node to get configured port and token
    let node_result = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await;
    
    let (port, token) = match node_result {
        Ok(Some(n)) => (n.port, n.token),
        _ => (3001, "unknown".to_string()),
    };

    format!(r#"#!/bin/bash
# Yunexal Node Installer

echo "Installing Yunexal Node..."

# 1. Install Docker if not present
if ! command -v docker &> /dev/null; then
    echo "Docker not found. Installing..."
    curl -fsSL https://get.docker.com -o get-docker.sh
    sh get-docker.sh
fi

# 2. Create directory
mkdir -p /opt/yunexal-node
cd /opt/yunexal-node

# 3. Create config.yml
cat <<EOF > config.yml
token: "{}"
node_id: "{}"
panel_url: "http://{}"
port: {}
EOF

# 4. Download and run the node agent
echo "Downloading Node Agent..."
curl -L -o yunexal-node http://{}/downloads/yunexal-node
chmod +x yunexal-node

# 5. Create systemd service
cat <<EOF > /etc/systemd/system/yunexal-node.service
[Unit]
Description=Yunexal Node Agent
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/yunexal-node
ExecStart=/opt/yunexal-node/yunexal-node
Restart=always

[Install]
WantedBy=multi-user.target
EOF

# 6. Start service
systemctl daemon-reload
systemctl enable yunexal-node
systemctl restart yunexal-node

echo "Node installed and started!"
"#, token, id, host, port, host)
}
