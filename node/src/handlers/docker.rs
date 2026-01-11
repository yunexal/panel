use crate::{models::CreateContainerRequest, state::NodeState};
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};
use bollard::container::{
    Config as DockerConfig, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::service::{HostConfig, PortBinding};
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Response structure for errors
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

/// Response structure for success operations
#[derive(Serialize)]
pub struct SuccessResponse {
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Container info for listing
#[derive(Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub status: String,
    pub image: String,
    pub server_id: Option<String>,
}

/// Check if a port is available on the host
fn is_port_free(port: u16) -> bool {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(_) => {
            debug!("Port {} is available", port);
            true
        }
        Err(e) => {
            warn!("Port {} is occupied: {}", port, e);
            false
        }
    }
}

/// List all managed containers
pub async fn list_containers(
    State(state): State<NodeState>,
) -> Result<Json<Vec<ContainerInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    // DO NOT CHANGE THIS LABEL ANYWAY!
    // Why? Because it's used to identify containers created by Node.
    // Avoid conflicts with other containers.
    filters.insert(
        "label".to_string(),
        vec!["yunexal.managed=true".to_string()],
    );

    let options = Some(ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    });

    match state.docker.list_containers(options).await {
        Ok(containers) => {
            let container_infos: Vec<ContainerInfo> = containers
                .into_iter()
                .map(|c| {
                    let name = c
                        .names
                        .as_ref()
                        .and_then(|names| names.first())
                        .map(|s| s.trim_start_matches('/').to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    let id = c.id.clone().unwrap_or_else(|| "unknown".to_string());
                    let state = c
                        .state
                        .as_ref()
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_else(|| "unknown".to_string());
                    let status = c.status.clone().unwrap_or_else(|| "unknown".to_string());
                    let image = c.image.clone().unwrap_or_else(|| "unknown".to_string());

                    let server_id = c
                        .labels
                        .as_ref()
                        .and_then(|labels| labels.get("yunexal.server_id"))
                        .map(|s| s.to_string());

                    ContainerInfo {
                        id,
                        name,
                        state,
                        status,
                        image,
                        server_id,
                    }
                })
                .collect();

            info!("Listed {} managed containers", container_infos.len());
            Ok(Json(container_infos))
        }
        Err(e) => {
            error!("Failed to list containers: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to list containers".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

/// Create and start a new container
pub async fn create_container(
    State(state): State<NodeState>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Creating container for server UUID: {}", payload.uuid);

    // Check if ports are available
    let mut occupied_ports = Vec::new();
    for (container_port, host_port) in &payload.ports {
        if let Ok(port) = host_port.parse::<u16>() {
            if !is_port_free(port) {
                occupied_ports.push((container_port.clone(), port));
            }
        } else {
            warn!(
                "Invalid port format for container port {}: {}",
                container_port, host_port
            );
        }
    }

    if !occupied_ports.is_empty() {
        let occupied_list: Vec<String> = occupied_ports
            .iter()
            .map(|(cp, hp)| format!("{} -> {}", cp, hp))
            .collect();

        error!(
            "Cannot create container {}: ports occupied: {}",
            payload.uuid,
            occupied_list.join(", ")
        );

        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "One or more ports are already in use".to_string(),
                details: Some(format!("Occupied ports: {}", occupied_list.join(", "))),
            }),
        ));
    }

    let container_name = format!("yunexal-{}", payload.uuid);
    info!("Container name: {}", container_name);

    let options = Some(CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    });

    let mut labels = HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());
    labels.insert("yunexal.server_id".to_string(), payload.uuid.clone());

    // Port Bindings
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for (container_port, host_port) in &payload.ports {
        let binding = vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(host_port.clone()),
        }];
        port_bindings.insert(container_port.clone(), Some(binding));
        debug!("Port binding: {} -> {}", container_port, host_port);
    }

    // Host Config (Limits)
    let host_config = HostConfig {
        memory: Some(payload.memory_limit * 1024 * 1024), // MB to Bytes
        memory_swap: Some(payload.swap_limit * 1024 * 1024),
        nano_cpus: Some(payload.cpu_limit * 10_000_000),
        blkio_weight: Some(payload.io_weight),
        port_bindings: Some(port_bindings),
        ..Default::default()
    };

    info!(
        "Resource limits: CPU: {}%, RAM: {}MB, SWAP: {}MB, IO: {}",
        payload.cpu_limit, payload.memory_limit, payload.swap_limit, payload.io_weight
    );

    // Env Vars
    let env: Vec<String> = payload
        .environment
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    debug!("Environment variables: {} vars set", env.len());

    let config = DockerConfig {
        image: Some(payload.image.clone()),
        labels: Some(labels),
        env: Some(env),
        // Wrap command in shell to ensure variable expansion and simple parsing works
        cmd: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            payload.startup_command.clone(),
        ]),
        host_config: Some(host_config),
        tty: Some(true),        // Enable TTY for console access
        open_stdin: Some(true), // Keep stdin open
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    info!("Using Docker image: {}", payload.image);

    // Create the container
    let container_id = match state.docker.create_container(options, config).await {
        Ok(res) => {
            info!("Container created successfully: {}", res.id);
            res.id
        }
        Err(e) => {
            error!("Failed to create container {}: {}", container_name, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to create container".to_string(),
                    details: Some(e.to_string()),
                }),
            ));
        }
    };

    // Start the container
    info!("Starting container: {}", container_id);
    if let Err(e) = state
        .docker
        .start_container(&container_id, None::<StartContainerOptions<String>>)
        .await
    {
        error!("Failed to start container {}: {}", container_id, e);

        // Rollback: try to remove the created container
        warn!("Attempting to rollback (remove) container {}", container_id);
        let remove_opts = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        if let Err(remove_err) = state
            .docker
            .remove_container(&container_id, remove_opts)
            .await
        {
            error!("Failed to rollback container removal: {}", remove_err);
        } else {
            info!("Rollback successful: container removed");
        }

        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to start container".to_string(),
                details: Some(e.to_string()),
            }),
        ));
    }

    info!("Container {} started successfully", container_id);

    Ok(Json(SuccessResponse {
        message: "Container created and started successfully".to_string(),
        data: Some(serde_json::json!({
            "container_id": container_id,
            "container_name": container_name,
        })),
    }))
}

/// Delete a container (stop and remove)
pub async fn delete_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let container_name = format!("yunexal-{}", uuid);
    info!("Deleting container: {}", container_name);

    // First, check if container exists
    let inspect_result = state
        .docker
        .inspect_container(
            &container_name,
            None::<bollard::container::InspectContainerOptions>,
        )
        .await;

    if let Err(e) = inspect_result {
        warn!(
            "Container {} not found or error inspecting: {}",
            container_name, e
        );
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Container not found".to_string(),
                details: Some(format!(
                    "Container {} does not exist or cannot be accessed",
                    container_name
                )),
            }),
        ));
    }

    let container_info = inspect_result.unwrap();
    let is_running = container_info
        .state
        .as_ref()
        .and_then(|s| s.running)
        .unwrap_or(false);

    // Stop container if running
    if is_running {
        info!("Stopping running container: {}", container_name);
        let stop_opts = Some(StopContainerOptions { t: 10 });

        match state
            .docker
            .stop_container(&container_name, stop_opts)
            .await
        {
            Ok(_) => info!("Container stopped successfully"),
            Err(e) => {
                warn!("Error stopping container (continuing with removal): {}", e);
            }
        }
    } else {
        info!("Container is not running, proceeding to removal");
    }

    // Remove container
    info!("Removing container: {}", container_name);
    let remove_opts = Some(RemoveContainerOptions {
        force: true,
        v: true, // Remove volumes
        ..Default::default()
    });

    match state
        .docker
        .remove_container(&container_name, remove_opts)
        .await
    {
        Ok(_) => {
            info!("Container {} deleted successfully", container_name);
            Ok(Json(SuccessResponse {
                message: "Container deleted successfully".to_string(),
                data: Some(serde_json::json!({
                    "container_name": container_name,
                    "was_running": is_running,
                })),
            }))
        }
        Err(e) => {
            error!("Failed to remove container {}: {}", container_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to remove container".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}
