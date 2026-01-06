use axum::{
    extract::{State, Path, Form},
    response::{Html, Redirect},
    http::HeaderMap,
};
use crate::{state::AppState, models::{Node, CreateNodeRequest, UpdateNodeRequest}};
use uuid::Uuid;

pub async fn create_node_page_handler() -> Html<String> {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION");
    let elapsed = start_time.elapsed();

    Html(format!(r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Create Node - Yunexal Panel</title>
        <style>
            body {{ font-family: sans-serif; padding: 2rem; max_width: 800px; margin: 0 auto; }}
            form {{ background: #f9f9f9; padding: 2rem; border-radius: 8px; border: 1px solid #ddd; }}
            .form-group {{ margin-bottom: 1rem; }}
            label {{ display: block; margin-bottom: 0.5rem; font-weight: bold; }}
            input {{ width: 100%; padding: 0.5rem; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px; }}
            button {{ background: #007bff; color: white; border: none; padding: 0.75rem 1.5rem; border-radius: 4px; cursor: pointer; font-size: 1rem; }}
            button:hover {{ background: #0056b3; }}
            .back-link {{ display: inline-block; margin-bottom: 1rem; color: #666; text-decoration: none; }}
            .footer {{ margin-top: 2rem; color: #888; font-size: 0.8rem; text-align: right; }}
        </style>
    </head>
    <body>
        <a href="/nodes" class="back-link">← Back to Nodes</a>
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

        <div class="footer">
            Yunexal Panel v{}<br>
            Execution time: {:.3}ms
        </div>
    </body>
    </html>
    "#, panel_version, elapsed.as_secs_f64() * 1000.0))
}

pub async fn create_node_handler(
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
        return Redirect::to("/nodes");
    }

    Redirect::to(&format!("/nodes/{}/setup", id))
}

pub async fn setup_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Html<String> {
    let start_time = std::time::Instant::now();
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
        let panel_version = env!("CARGO_PKG_VERSION");
        let elapsed = start_time.elapsed();
        
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
                .footer {{ margin-top: 2rem; color: #888; font-size: 0.8rem; text-align: right; }}
            </style>
        </head>
        <body>
            <h1>Node Created Successfully!</h1>
            <div class="container">
                <h2>Setup Instructions for "{}"</h2>
                <p>Run the following command on your remote server ({}):</p>
                <pre>{}</pre>
                <p>This command will install Docker (if needed), configure the node agent, and start it.</p>
                
                <a href="/nodes" class="btn btn-primary">Go to Nodes List</a>
            </div>

            <div class="footer">
                Yunexal Panel v{}<br>
                Execution time: {:.3}ms
            </div>
        </body>
        </html>
        "#, node.name, node.ip, install_cmd, panel_version, elapsed.as_secs_f64() * 1000.0))
    } else {
        Html("<h1>Node not found</h1>".to_string())
    }
}

pub async fn delete_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    let _ = sqlx::query("DELETE FROM nodes WHERE id = $1::uuid")
        .bind(id)
        .execute(&state.db)
        .await;
    
    axum::http::StatusCode::NO_CONTENT
}

pub async fn edit_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Html<String> {
    let start_time = std::time::Instant::now(); // Start timer
    let node = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();

    if let Some(node) = node {
        let panel_version = env!("CARGO_PKG_VERSION");
        let elapsed = start_time.elapsed();

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
                .footer {{ margin-top: 2rem; color: #888; font-size: 0.8rem; text-align: right; }}
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

            <div class="footer">
                Yunexal Panel v{}<br>
                Execution time: {:.3}ms
            </div>
        </body>
        </html>
        "#, node.id, node.name, node.ip, node.port, panel_version, elapsed.as_secs_f64() * 1000.0))
    } else {
        Html("<h1>Node not found</h1>".to_string())
    }
}

pub async fn update_node_handler(
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
