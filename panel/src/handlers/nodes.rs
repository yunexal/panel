use axum::{
    extract::{State, Path, Form},
    response::{Redirect, IntoResponse},
    http::HeaderMap,
};
use std::collections::HashSet;
use crate::{state::AppState, models::{Node, CreateNodeRequest, UpdateNodeRequest}};
use uuid::Uuid;
use askama::Template;
use crate::handlers::HtmlTemplate;

#[derive(Template)]
#[template(path = "node_create.html")]
struct CreateNodeTemplate {
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
}

#[derive(Template)]
#[template(path = "node_edit.html")]
struct EditNodeTemplate {
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    found: bool,
    install_cmd: String,
    uninstall_cmd: String,
}

#[derive(Template)]
#[template(path = "node_setup.html")]
struct SetupNodeTemplate {
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    install_cmd: String,
    found: bool,
}

pub async fn create_node_page_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateNodeTemplate {
        panel_name,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
    })
}

pub async fn create_node_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateNodeRequest>,
) -> Redirect {
    // Validate Port
    if (payload.port >= 0 && payload.port <= 1023) || (payload.sftp_port >= 0 && payload.sftp_port <= 1023) {
        // In a real app we might return a fancy error page or error message in flash session
        // For now, just redirect back to creation page (or do nothing)
        eprintln!("Blocked attempt to create node on restricted port");
        return Redirect::to("/nodes/new");
    }

    // Validate Collision
    if payload.port == payload.sftp_port {
        eprintln!("Daemon Port and SFTP Port cannot be the same");
        return Redirect::to("/nodes/new");
    }

    let id = Uuid::new_v4().to_string();
    
    // Generate a random token for the node
    let token = Uuid::new_v4().to_string();

    if let Err(e) = sqlx::query("INSERT INTO nodes (id, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit) VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(&id)
        .bind(&payload.name)
        .bind(&payload.ip)
        .bind(&payload.port)
        .bind(&token)
        .bind(&payload.sftp_port)
        .bind(payload.ram_limit.unwrap_or(0))
        .bind(payload.disk_limit.unwrap_or(0))
        .bind(payload.cpu_limit.unwrap_or(0))
        .execute(&state.db)
        .await 
    {
        eprintln!("Failed to insert node: {}", e);
        return Redirect::to("/nodes");
    }

    // Process initial allocations if provided
    if let Some(ports_str) = &payload.allocation_ports {
        let ports = parse_ports(ports_str);
        
        // Validate Allocations
        for port in &ports {
            if *port >= 0 && *port <= 1023 {
                eprintln!("Blocked attempt to use restricted allocation port: {}", port);
                return Redirect::to("/nodes/new");
            }
        }

        // Deduplicate
        let unique_ports: HashSet<i32> = ports.into_iter().collect();
        for port in unique_ports {
             if port >= 0 && port <= 65535 {
                let _ = sqlx::query("INSERT INTO allocations (id, node_id, ip, port) VALUES ($1::uuid, $2::uuid, $3, $4) ON CONFLICT DO NOTHING")
                    .bind(Uuid::new_v4().to_string())
                    .bind(&id)
                    .bind(&payload.ip)
                    .bind(port)
                    .execute(&state.db)
                    .await;
             }
        }
    }

    // Invalidate Cache
    state.invalidate_nodes_cache().await;

    Redirect::to(&format!("/nodes/{}/setup", id))
}

pub async fn setup_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");
    let panel_name = state.panel_name.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let node_result = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await;

    let (node, found, install_cmd) = match node_result {
        Ok(Some(n)) => {
            let cmd = format!("curl -sSL http://{}/install/{} | sudo bash", host, n.id);
            (n, true, cmd)
        },
        _ => (
            Node { 
                id: "".to_string(), 
                name: "".to_string(), 
                ip: "".to_string(), 
                port: 0, 
                token: "".to_string(),
                sftp_port: 0,
                ram_limit: 0,
                disk_limit: 0,
                cpu_limit: 0,
                version: "".to_string()
            },
            false,
            "".to_string()
        ),
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(SetupNodeTemplate {
        panel_name,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node,
        found,
        install_cmd,
    })
}

pub async fn delete_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    let _ = sqlx::query("DELETE FROM nodes WHERE id = $1::uuid")
        .bind(id)
        .execute(&state.db)
        .await;
    
    // Invalidate Cache
    state.invalidate_nodes_cache().await;
    
    // Return empty string with 200 OK so that HTMX swaps the element with nothing (removing it)
    ""
}

pub async fn edit_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now(); 
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();

    let node_res = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await;
    
    // Log error if DB failure
    if let Err(ref e) = node_res {
        tracing::error!("Failed to fetch node for edit: {}", e);
    }
    let node = node_res.unwrap_or(None);

    // We ideally need the host to format install command, defaulting for now
    let host = "127.0.0.1:3000";

    let (node_val, found, install_cmd, uninstall_cmd) = if let Some(n) = node {
        let install = format!("curl -sSL http://{}/install/{} | sudo bash", host, n.id);
        let uninstall = format!("systemctl stop yunexal-node-{} && rm -rf /etc/yunexal/node-{}", n.id, n.id);
        (n, true, install, uninstall)
    } else {
        (
            Node { 
                id: "".to_string(), 
                name: "".to_string(), 
                ip: "".to_string(), 
                port: 0, 
                token: "".to_string(),
                sftp_port: 0,
                ram_limit: 0,
                disk_limit: 0,
                cpu_limit: 0,
                version: "".to_string()
            },
            false,
            "".to_string(),
            "".to_string()
        )
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(EditNodeTemplate {
        panel_name,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node: node_val,
        found,
        install_cmd,
        uninstall_cmd,
    })
}

pub async fn update_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<UpdateNodeRequest>,
) -> Redirect {
    // Validate Ports (duplicated logic from create, could be shared)
    if (payload.port >= 0 && payload.port <= 1023) || (payload.sftp_port >= 0 && payload.sftp_port <= 1023) {
        eprintln!("Blocked attempt to update node to restricted port");
        return Redirect::to(&format!("/nodes/{}/edit", id));
    }
    if payload.port == payload.sftp_port {
        eprintln!("Daemon Port and SFTP Port cannot be the same");
        return Redirect::to(&format!("/nodes/{}/edit", id));
    }

    let _ = sqlx::query("UPDATE nodes SET name = $1, ip = $2, port = $3, sftp_port = $4, ram_limit = $5, disk_limit = $6, cpu_limit = $7 WHERE id = $8::uuid")
        .bind(&payload.name)
        .bind(&payload.ip)
        .bind(&payload.port)
        .bind(&payload.sftp_port)
        .bind(payload.ram_limit.unwrap_or(0))
        .bind(payload.disk_limit.unwrap_or(0))
        .bind(payload.cpu_limit.unwrap_or(0))
        .bind(&id)
        .execute(&state.db)
        .await;
    
    // Invalidate Cache
    state.invalidate_nodes_cache().await;
    
    Redirect::to("/")
}

pub async fn trigger_node_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if let Some(node) = node_opt {
        let url = format!("http://{}:{}/self-update", node.ip, node.port);
        let client = reqwest::Client::new();
        
        let res = client.post(&url)
            .header("Authorization", &format!("Bearer {}", node.token))
            .send()
            .await;
            
        return match res {
            Ok(r) => {
                 if r.status().is_success() {
                     // Assuming we use a toast or just a simple alert via HTMX not supported yet?
                     // Return a simple HTML fragment or text
                     axum::response::Response::builder()
                        .status(200)
                        .body(axum::body::Body::from("Update initiated"))
                        .unwrap()
                 } else {
                     axum::response::Response::builder()
                        .status(500)
                        .body(axum::body::Body::from(format!("Node error: {}", r.status())))
                        .unwrap()
                 }
            },
            Err(e) => {
                axum::response::Response::builder()
                    .status(500)
                    .body(axum::body::Body::from(format!("Connection failed: {}", e)))
                    .unwrap()
            }
        };
    }
    
    axum::response::Response::builder()
        .status(404)
        .body(axum::body::Body::from("Node not found"))
        .unwrap()
}

fn parse_ports(input: &str) -> Vec<i32> {
    let mut result = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();
    
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() { continue; }
        
        if trimmed.contains('-') {
            let range_parts: Vec<&str> = trimmed.split('-').collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (range_parts[0].trim().parse::<i32>(), range_parts[1].trim().parse::<i32>()) {
                    if start <= end {
                        for p in start..=end {
                            result.push(p);
                        }
                    }
                }
            }
        } else {
            if let Ok(p) = trimmed.parse::<i32>() {
                result.push(p);
            }
        }
    }
    result
}
