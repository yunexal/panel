use crate::http::handlers::HtmlTemplate;
use crate::{
    models::{Allocation, CreateAllocationRequest, DeleteAllocationRequest, Node},
    state::AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "node_allocations.html")]
struct AllocationsTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    allocations: Vec<Allocation>,
    page: u32,
    has_more: bool,
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    page: Option<u32>,
}

pub async fn allocations_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let page = params.page.unwrap_or(1);
    let limit = 50;
    let offset = (page - 1) * limit;

    let node_opt = sqlx::query_as::<_, Node>("SELECT id::text, name, ip, port, token, sftp_port, ram_limit, disk_limit, cpu_limit, version FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if node_opt.is_none() {
        return Redirect::to("/nodes").into_response();
    }
    let node = node_opt.unwrap();

    let allocations = sqlx::query_as::<_, Allocation>("SELECT id::text, node_id::text, ip, port, server_id::text FROM allocations WHERE node_id = $1::uuid ORDER BY port ASC LIMIT $2 OFFSET $3")
        .bind(&id)
        .bind((limit + 1) as i32) // Fetch one more. Postgres needs i32/i64 not u32.
        .bind(offset as i32)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let has_more = allocations.len() > limit as usize;
    let display_allocations = if has_more {
        allocations[0..limit as usize].to_vec()
    } else {
        allocations
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(AllocationsTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node,
        allocations: display_allocations,
        page,
        has_more,
    })
    .into_response()
}

pub async fn create_allocations_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<CreateAllocationRequest>,
) -> Redirect {
    let ports = parse_ports(&payload.ports);

    // Deduplicate
    let unique_ports: HashSet<i32> = ports.into_iter().collect();

    for port in unique_ports {
        // Enforce port restrictions server-side
        if port >= 0 && port <= 1023 {
            continue; // Skip system ports
        }

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

    Redirect::to(&format!("/nodes/{}/allocations", id))
}

pub async fn delete_allocations_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<DeleteAllocationRequest>,
) -> Redirect {
    let ports_to_delete = parse_ports(&payload.ports);

    if payload.force {
        for port in ports_to_delete {
            let _ = sqlx::query("DELETE FROM allocations WHERE node_id = $1::uuid AND port = $2")
                .bind(&id)
                .bind(port)
                .execute(&state.db)
                .await;
        }
    } else {
        // Safe delete (only if server_id is NULL)
        for port in ports_to_delete {
            let _ = sqlx::query("DELETE FROM allocations WHERE node_id = $1::uuid AND port = $2 AND server_id IS NULL")
                .bind(&id)
                .bind(port)
                .execute(&state.db)
                .await;
        }
    }

    Redirect::to(&format!("/nodes/{}/allocations", id))
}

fn parse_ports(input: &str) -> Vec<i32> {
    let mut result = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains('-') {
            let range_parts: Vec<&str> = trimmed.split('-').collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (
                    range_parts[0].trim().parse::<i32>(),
                    range_parts[1].trim().parse::<i32>(),
                ) {
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
