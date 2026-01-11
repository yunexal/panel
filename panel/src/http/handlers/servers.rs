use crate::grpc::NodeClient;
use crate::http::handlers::HtmlTemplate;
use crate::models::{
    Allocation, CreateServerRequest, DeleteServerRequest, Image, Node, Runtime, Server,
    UpdateServerRequest,
};
use crate::state::AppState;
use askama::Template;
use axum::{
    Json,
    extract::{
        Form, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "servers.html")]
struct ServersTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    has_nodes: bool,
    servers: Vec<Server>,
}

#[derive(Template)]
#[template(path = "server_create.html")]
struct CreateServerTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    nodes: Vec<Node>,
    runtimes: Vec<Runtime>,
    images_json: String,
    allocations_json: String,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "server_manage.html")]
struct ManageServerTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    server: Server,
    node: Node,
}

#[derive(Template)]
#[template(path = "server_edit.html")]
struct EditServerTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    server: Server,
}

#[derive(Deserialize)]
pub struct ServerCreateQuery {
    pub error: Option<String>,
}

pub async fn servers_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    // Check nodes
    let nodes = state.get_nodes().await;
    let has_nodes = !nodes.is_empty();

    let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(ServersTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "servers".to_string(),
        has_nodes,
        servers,
    })
}

pub async fn create_server_page_handler(
    State(state): State<AppState>,
    Query(query): Query<ServerCreateQuery>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    // Fetch Data
    let nodes = state.get_nodes().await;

    let runtimes = sqlx::query_as::<_, Runtime>("SELECT id::text, name, description, color, sort_order FROM runtimes ORDER BY sort_order ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let images = sqlx::query_as::<_, Image>("SELECT id::text, runtime_id::text, name, docker_images, description, stop_command, startup_command, log_config, config_files, start_config, requires_port, install_script::text, install_container::text, install_entrypoint::text, variables::text FROM images")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let allocations = sqlx::query_as::<_, Allocation>("SELECT id::text, node_id::text, ip, port, server_id::text FROM allocations WHERE server_id IS NULL")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    // Prepare JSON for frontend
    // Group images by runtime_id
    let mut images_map: HashMap<String, Vec<Image>> = HashMap::new();
    for img in images {
        images_map
            .entry(img.runtime_id.clone())
            .or_default()
            .push(img);
    }
    let images_json = serde_json::to_string(&images_map).unwrap_or("{}".to_string());

    // Group allocations by node_id
    let mut allocations_map: HashMap<String, Vec<Allocation>> = HashMap::new();
    for alloc in allocations {
        allocations_map
            .entry(alloc.node_id.clone())
            .or_default()
            .push(alloc);
    }
    let allocations_json = serde_json::to_string(&allocations_map).unwrap_or("{}".to_string());

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateServerTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "servers".to_string(),
        nodes,
        runtimes,
        images_json,
        allocations_json,
        error: query.error,
    })
}

pub async fn create_server_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateServerRequest>,
) -> Redirect {
    let server_id = Uuid::new_v4().to_string();

    // 0. Fetch Image to check requires_port
    let image = match sqlx::query_as::<_, Image>(
        "SELECT id::text, runtime_id::text, name, docker_images, description, stop_command, startup_command, log_config, config_files, start_config, requires_port, install_script::text, install_container::text, install_entrypoint::text, variables::text FROM images WHERE id = $1::uuid"
    )
    .bind(&payload.image_id)
    .fetch_optional(&state.db)
    .await {
        Ok(Some(img)) => img,
        Ok(None) => return Redirect::to("/servers/new?error=invalid_image"),
        Err(_) => return Redirect::to("/servers/new?error=db_error"),
    };

    let allocation_id: Option<String>;
    let node_id_resolved: String;

    // Check if user specifically selected an allocation (Manual Override)
    let user_selected_alloc = payload.default_allocation.clone().filter(|s| !s.is_empty());

    if let Some(alloc_id) = user_selected_alloc {
        // CASE A: User explicitly picked a port. We use it regardless of requires_port.
        let alloc_res = sqlx::query_as::<_, Allocation>("SELECT id::text, node_id::text, ip, port, server_id::text FROM allocations WHERE id = $1::uuid")
            .bind(&alloc_id)
            .fetch_optional(&state.db)
            .await;

        let (nid, alloc_valid) = match alloc_res {
            Ok(Some(a)) => (a.node_id, a.server_id.is_none()),
            _ => (String::new(), false),
        };

        if !alloc_valid {
            eprintln!("Invalid or occupied allocation selected");
            return Redirect::to("/servers/new?error=invalid_allocation");
        }

        allocation_id = Some(alloc_id);
        node_id_resolved = nid;
    } else if image.requires_port {
        // CASE B: Image REQUIRES a port, and user selected "Auto". We MUST find one.

        // If specific node requested:
        let query = if let Some(node_id) = payload.node_id.filter(|s| !s.is_empty()) {
            format!(
                "SELECT id::text FROM allocations WHERE node_id = '{}' AND server_id IS NULL LIMIT 1",
                node_id
            )
        } else {
            // Any node
            "SELECT id::text FROM allocations WHERE server_id IS NULL LIMIT 1".to_string()
        };

        let auto_alloc_id = match sqlx::query_scalar::<_, String>(&query)
            .fetch_optional(&state.db)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                eprintln!("No free allocations available");
                return Redirect::to("/servers/new?error=no_allocations");
            }
            Err(e) => {
                eprintln!("DB Error finding allocation: {}", e);
                return Redirect::to("/servers/new?error=db_error");
            }
        };

        // Get Node ID from this auto-assigned allocation
        let alloc_res = sqlx::query_as::<_, Allocation>("SELECT id::text, node_id::text, ip, port, server_id::text FROM allocations WHERE id = $1::uuid")
            .bind(&auto_alloc_id)
            .fetch_optional(&state.db)
            .await;

        let (nid, alloc_valid) = match alloc_res {
            Ok(Some(a)) => (a.node_id, a.server_id.is_none()),
            _ => (String::new(), false),
        };

        // This theoretically shouldn't happen if the previous query found it, but concurrency safe check
        if !alloc_valid {
            return Redirect::to("/servers/new?error=allocation_race_condition");
        }

        allocation_id = Some(auto_alloc_id);
        node_id_resolved = nid;
    } else {
        // CASE C: Image DOES NOT require a port, and user selected "Auto/None".
        // We do NOT assign a port.
        allocation_id = None;

        // But we DO need to resolve a Node ID.
        if let Some(nid) = payload.node_id.filter(|s| !s.is_empty()) {
            node_id_resolved = nid;
        } else {
            // Auto-select node (simplistic: pick first available node)
            let q = "SELECT id::text FROM nodes LIMIT 1";
            match sqlx::query_scalar::<_, String>(q)
                .fetch_optional(&state.db)
                .await
            {
                Ok(Some(nid)) => node_id_resolved = nid,
                Ok(_) => return Redirect::to("/servers/new?error=no_nodes_available"),
                Err(_) => return Redirect::to("/servers/new?error=db_error"),
            }
        }
    }

    // 2. Prepare Data
    let docker_image = if let Some(custom) = payload.custom_docker_image.filter(|s| !s.is_empty()) {
        custom
    } else {
        payload.docker_image.unwrap_or_default()
    };

    // Extract environment variables from flattened map
    // Form sends environment[VAR_NAME]=value, we want VAR_NAME=value
    let mut vars: HashMap<String, String> = HashMap::new();
    for (key, value) in &payload.environment {
        if key.starts_with("environment[") && key.ends_with(']') {
            let var_name = &key[12..key.len() - 1];
            vars.insert(var_name.to_string(), value.clone());
        }
    }
    let vars_json = serde_json::to_string(&vars).unwrap_or_else(|_| "{}".to_string());

    let start_status = if payload.start_on_install.is_some() {
        "installing"
    } else {
        "installing"
    }; // Always installing first? Or 'created'? Logic usually is installing.

    // 3. Transactions
    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to start transaction: {}", e);
            return Redirect::to("/servers/new?error=db_error");
        }
    };

    // Create Server
    let q = sqlx::query(
        r#"
        INSERT INTO servers (
            id, name, description, owner_id, node_id, allocation_id, image_id,
            cpu_limit, ram_limit, disk_limit, swap_limit, backup_limit,
            io_weight, oom_killer, docker_image, startup_command, cpu_pinning, variables, status
        ) VALUES (
            $1::uuid, $2, $3, $4, $5::uuid, $6, $7::uuid,
            $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17, $18, $19
        )
    "#,
    )
    .bind(&server_id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.owner_id.clone().unwrap_or_else(|| "1".to_string()))
    .bind(&node_id_resolved)
    .bind(allocation_id.as_ref().map(|s| Uuid::parse_str(s).unwrap())) // bind Option<Uuid>
    .bind(&payload.image_id)
    .bind(payload.cpu_limit.unwrap_or(0))
    .bind(payload.ram_limit.unwrap_or(0))
    .bind(payload.disk_limit.unwrap_or(0))
    .bind(payload.swap_limit.unwrap_or(0))
    .bind(payload.backup_limit.unwrap_or(0))
    .bind(payload.io_weight.unwrap_or(500))
    .bind(payload.oom_killer.is_some())
    .bind(&docker_image)
    .bind(payload.startup_command.as_deref().unwrap_or_default())
    .bind(&payload.cpu_pinning)
    .bind(&vars_json)
    .bind(start_status)
    .execute(&mut *tx)
    .await;

    if let Err(e) = q {
        eprintln!("Failed to create server: {}", e);
        let _ = tx.rollback().await;
        return Redirect::to("/servers/new?error=create_failed");
    }

    if let Some(alloc_id) = allocation_id {
        // Update Allocation
        let q2 = sqlx::query("UPDATE allocations SET server_id = $1::uuid WHERE id = $2::uuid")
            .bind(&server_id)
            .bind(&alloc_id)
            .execute(&mut *tx)
            .await;

        if let Err(e) = q2 {
            eprintln!("Failed to assign allocation: {}", e);
            let _ = tx.rollback().await;
            return Redirect::to("/servers/new?error=alloc_failed");
        }

        // Logic for additional ports
        if let Some(ports_str) = &payload.additional_ports {
            let ports = parse_ports(ports_str);
            if !ports.is_empty() {
                let q3 = sqlx::query("UPDATE allocations SET server_id = $1::uuid WHERE node_id = $2::uuid AND port = ANY($3) AND server_id IS NULL")
                    .bind(&server_id)
                    .bind(&node_id_resolved)
                    .bind(&ports)
                    .execute(&mut *tx)
                    .await;

                if let Err(e) = q3 {
                    eprintln!("Failed to assign additional ports: {}", e);
                    let _ = tx.rollback().await;
                    return Redirect::to("/servers/new?error=additional_alloc_failed");
                }
            }
        }
    }

    // Silence unused warning for runtime_id (used for frontend filtering)
    let _ = &payload.runtime_id;

    if let Err(e) = tx.commit().await {
        eprintln!("Failed to commit transaction: {}", e);
        return Redirect::to("/servers/new?error=commit_failed");
    }

    // --- NODE AGENT NOTIFICATION ---
    let node_id = node_id_resolved.clone();
    let state_clone = state.clone();
    let server_id_clone = server_id.clone();
    let docker_image_clone = docker_image.clone();
    let startup_command = payload.startup_command.clone().unwrap_or_default();
    let ram_limit = payload.ram_limit.unwrap_or(0);
    let swap_limit = payload.swap_limit.unwrap_or(0);
    let cpu_limit = payload.cpu_limit.unwrap_or(0);
    let io_weight = payload.io_weight.unwrap_or(500);
    let _token = state.clone(); // Not exactly token but we need it inside spawn

    tokio::spawn(async move {
        // 1. Get Node
        let node = match sqlx::query_as::<_, Node>("SELECT * FROM nodes WHERE id = $1::uuid")
            .bind(&node_id)
            .fetch_optional(&state_clone.db)
            .await
        {
            Ok(Some(n)) => n,
            _ => return,
        };

        // 2. Get Allocations for ports
        let allocations = sqlx::query_as::<_, Allocation>(
            "SELECT id::text, node_id::text, ip, port, server_id::text FROM allocations WHERE server_id = $1::text"
        )
        .bind(&server_id_clone)
        .fetch_all(&state_clone.db)
        .await
        .unwrap_or_default();

        let mut ports_map = HashMap::new();
        for alloc in allocations {
            ports_map.insert(format!("{}/tcp", alloc.port), alloc.port.to_string());
        }

        let container_req = NodeCreateContainerRequest {
            uuid: server_id_clone.clone(),
            image: docker_image_clone,
            startup_command,
            environment: vars,
            memory_limit: ram_limit as i64,
            swap_limit: swap_limit as i64,
            cpu_limit: cpu_limit as i64,
            io_weight: io_weight as u16,
            ports: ports_map,
        };

        let node_url = format!("http://{}:{}/containers", node.ip, node.port);
        let client = reqwest::Client::new();

        let _ = client
            .post(&node_url)
            .header("Authorization", format!("Bearer {}", node.token))
            .json(&container_req)
            .send()
            .await;
    });

    Redirect::to("/servers")
}

fn parse_ports(input: &str) -> Vec<i32> {
    let mut ports = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.parse::<i32>(), end.parse::<i32>()) {
                if s <= e {
                    for p in s..=e {
                        ports.push(p);
                    }
                }
            }
        } else if let Ok(p) = part.parse::<i32>() {
            ports.push(p);
        }
    }
    ports
}

pub async fn manage_server_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    tracing::info!("Accessing manage page for server: {}", id);

    let server = match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(s)) => {
            tracing::info!("Found server: {}", s.name);
            s
        }
        Ok(None) => {
            tracing::warn!("Server {} not found, redirecting", id);
            return Redirect::to("/servers").into_response();
        }
        Err(e) => {
            tracing::error!("Error fetching server {}: {}", id, e);
            return Redirect::to("/servers").into_response();
        }
    };

    // Fetch node info
    let node = match sqlx::query_as::<_, Node>("SELECT id::uuid, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1")
        .bind(server.node_id)
        .fetch_optional(&state.db)
        .await
{
    Ok(Some(n)) => {
        tracing::info!("Found node: {}", n.name);
        n
    }
    Ok(None) => {
        tracing::warn!(
            "Node {} not found for server {}, redirecting",
            server.node_id,
            id
        );
        return Redirect::to("/servers").into_response();
    }
    Err(e) => {
        tracing::error!("Error fetching node for server {}: {}", id, e);
        return Redirect::to("/servers").into_response();
    }
};

    let template = ManageServerTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version: env!("CARGO_PKG_VERSION").to_string(),
        execution_time: start_time.elapsed().as_secs_f64() * 1000.0,
        active_tab: "servers".to_string(),
        server,
        node,
    };

    HtmlTemplate(template).into_response()
}

pub async fn edit_server_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let server = match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return Redirect::to("/servers").into_response(),
        Err(e) => {
            eprintln!("Error fetching server: {}", e);
            return Redirect::to("/servers").into_response();
        }
    };

    let template = EditServerTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version: "0.1.0".to_string(),
        execution_time: start_time.elapsed().as_secs_f64(),
        active_tab: "servers".to_string(),
        server,
    };

    HtmlTemplate(template).into_response()
}

pub async fn update_server_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Form(payload): Form<UpdateServerRequest>,
) -> impl IntoResponse {
    let q = sqlx::query(
        r#"
        UPDATE servers SET
            name = $2,
            description = $3,
            owner_id = $4,
            cpu_limit = $5,
            ram_limit = $6,
            disk_limit = $7,
            swap_limit = $8,
            backup_limit = $9,
            io_weight = $10,
            oom_killer = $11,
            docker_image = $12,
            startup_command = $13
        WHERE id = $1
    "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.owner_id.unwrap_or("1".to_string()))
    .bind(payload.cpu_limit.unwrap_or(0))
    .bind(payload.ram_limit.unwrap_or(0))
    .bind(payload.disk_limit.unwrap_or(0))
    .bind(payload.swap_limit.unwrap_or(0))
    .bind(payload.backup_limit.unwrap_or(0))
    .bind(payload.io_weight.unwrap_or(500))
    .bind(payload.oom_killer.is_some())
    .bind(&payload.docker_image)
    .bind(&payload.startup_command)
    .execute(&state.db)
    .await;

    match q {
        Ok(_) => Redirect::to(&format!("/servers/{}/manage", id)).into_response(),
        Err(e) => {
            eprintln!("Failed to update server: {}", e);
            Redirect::to(&format!("/servers/{}/edit?error=update_failed", id)).into_response()
        }
    }
}

pub async fn delete_server_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Form(payload): Form<DeleteServerRequest>,
) -> impl IntoResponse {
    let force = payload.force.is_some();

    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(_) => return Redirect::to(&format!("/servers/{}/edit?error=db_error", id)),
    };

    // Free allocations
    let q1 = sqlx::query("UPDATE allocations SET server_id = NULL WHERE server_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    if let Err(e) = q1 {
        eprintln!("Failed to free allocations: {}", e);
        if !force {
            let _ = tx.rollback().await;
            return Redirect::to(&format!("/servers/{}/edit?error=alloc_cleanup_failed", id));
        }
    }

    // Delete server
    let q2 = sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    if let Err(e) = q2 {
        eprintln!("Failed to delete server: {}", e);
        let _ = tx.rollback().await;
        // In force mode, do we still rollback? "Force" usually means delete the record.
        // If DELETE fails, it's usually constraint or DB error.
        // If force, we might not be able to do much.
        // But for "safety vs forcibly", force usually applies to external checks (like daemon).
        // For DB, if DELETE fails, we assume it's critical.
        return Redirect::to(&format!("/servers/{}/edit?error=delete_failed", id));
    }

    if let Err(e) = tx.commit().await {
        eprintln!("Failed to commit delete: {}", e);
        return Redirect::to(&format!("/servers/{}/edit?error=commit_failed", id));
    }

    Redirect::to("/servers")
}

// ============= DOCKER INTEGRATION HANDLERS =============

use serde::Serialize;

#[derive(Serialize)]
pub struct NodeCreateContainerRequest {
    pub uuid: String,
    pub image: String,
    pub startup_command: String,
    pub environment: HashMap<String, String>,
    pub memory_limit: i64,
    pub swap_limit: i64,
    pub cpu_limit: i64,
    pub io_weight: u16,
    pub ports: HashMap<String, String>,
}

pub async fn server_stats_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    // 1. Get server from DB
    let server = match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
        Err(e) => {
            eprintln!("Error fetching server: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // 2. Get node info
    let node = match sqlx::query_as::<_, Node>("SELECT * FROM nodes WHERE id = $1")
        .bind(server.node_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(n)) => n,
        Ok(None) => return (StatusCode::NOT_FOUND, "Node not found").into_response(),
        Err(e) => {
            eprintln!("Error fetching node: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // 3. Connect to Node Agent via gRPC
    let mut grpc_client: NodeClient =
        match NodeClient::connect(format!("http://{}:{}", node.ip, node.port)).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error connecting to node gRPC: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Node connection error")
                    .into_response();
            }
        };

    match grpc_client.get_stats(id.to_string()).await {
        Ok(stats) => Json(serde_json::json!({
            "cpu": stats.cpu_usage,
            "ram": stats.ram_usage,
            "net_rx": stats.net_rx,
            "net_tx": stats.net_tx,
        }))
        .into_response(),
        Err(e) => {
            eprintln!("Error fetching stats via gRPC: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Node error").into_response()
        }
    }
}

pub async fn start_server_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    // 1. Get server from DB
    let server = match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return Redirect::to(&format!("/servers/{}/manage", id)).into_response(),
        Err(e) => {
            eprintln!("Error fetching server: {}", e);
            return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
        }
    };

    // 2. Get node info
    let node = match sqlx::query_as::<_, Node>("SELECT * FROM nodes WHERE id = $1")
        .bind(server.node_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(n)) => n,
        Ok(None) => return Redirect::to(&format!("/servers/{}/manage", id)).into_response(),
        Err(e) => {
            eprintln!("Error fetching node: {}", e);
            return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
        }
    };

    // 3. Call Node Agent via gRPC to start container
    let mut grpc_client: NodeClient =
        match NodeClient::connect(format!("http://{}:{}", node.ip, node.port)).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error connecting to node gRPC: {}", e);
                return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
            }
        };

    if let Err(e) = grpc_client.start_container(id.to_string()).await {
        eprintln!("Error starting container via gRPC: {}", e);
        return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
    }

    // Update server status to running
    let _ = sqlx::query("UPDATE servers SET status = 'running' WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    Redirect::to(&format!("/servers/{}/manage", id)).into_response()
}

pub async fn stop_server_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    // 1. Get server from DB
    let server = match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return Redirect::to(&format!("/servers/{}/manage", id)).into_response(),
        Err(e) => {
            eprintln!("Error fetching server: {}", e);
            return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
        }
    };

    // 2. Get node info
    let node = match sqlx::query_as::<_, Node>("SELECT * FROM nodes WHERE id = $1")
        .bind(server.node_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(n)) => n,
        Ok(None) => return Redirect::to(&format!("/servers/{}/manage", id)).into_response(),
        Err(e) => {
            eprintln!("Error fetching node: {}", e);
            return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
        }
    };

    // 3. Call Node Agent via gRPC to stop container
    let mut grpc_client: NodeClient =
        match NodeClient::connect(format!("http://{}:{}", node.ip, node.port)).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error connecting to node gRPC: {}", e);
                return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
            }
        };

    if let Err(e) = grpc_client.stop_container(id.to_string()).await {
        eprintln!("Error stopping container via gRPC: {}", e);
        return Redirect::to(&format!("/servers/{}/manage", id)).into_response();
    }

    // Update server status to stopped
    let _ = sqlx::query("UPDATE servers SET status = 'stopped' WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    Redirect::to(&format!("/servers/{}/manage", id)).into_response()
}

pub async fn restart_server_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    // Stop then start
    let _ = stop_server_handler(State(state.clone()), axum::extract::Path(id)).await;
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    start_server_handler(State(state), axum::extract::Path(id)).await
}
