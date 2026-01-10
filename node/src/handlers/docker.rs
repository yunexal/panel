use crate::{models::CreateContainerRequest, state::NodeState};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use bollard::container::{
    AttachContainerOptions, Config as DockerConfig, CreateContainerOptions, ListContainersOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::service::{HostConfig, PortBinding};
use futures_util::{StreamExt, SinkExt};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt; // For writing to container input

fn is_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

pub async fn list_containers(State(state): State<NodeState>) -> Json<Vec<String>> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    //DO NOT CHANGE THIS LABEL ANYWAY!
    //Why? Because it's used to identify containers created by Node.
    //Avoid conflicts with other containers.
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
            let names: Vec<String> = containers
                .into_iter()
                .map(|c| {
                    let name = c
                        .names
                        .unwrap_or_default()
                        .first()
                        .map(|s| s.to_string())
                        .unwrap_or("unknown".to_string());
                    let state = c
                        .state
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_else(|| "unknown".to_string());
                    format!("{} [{}]", name, state)
                })
                .collect();
            Json(names)
        }
        Err(_) => Json(vec!["Error listing containers".to_string()]),
    }
}

pub async fn create_container(
    State(state): State<NodeState>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<String>, StatusCode> {
    // Check if ports are available
    for host_port in payload.ports.values() {
        if let Ok(port) = host_port.parse::<u16>() {
            if !is_port_free(port) {
                eprintln!("Port {} is occupied on this node.", port);
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    let container_name = format!("yunexal-{}", payload.uuid);

    let options = Some(CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    });

    let mut labels = HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());
    labels.insert("yunexal.server_id".to_string(), payload.uuid.clone());

    // Port Bindings
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for (container_port, host_port) in payload.ports {
        let binding = vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(host_port),
        }];
        port_bindings.insert(container_port, Some(binding));
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

    // Env Vars
    let env: Vec<String> = payload
        .environment
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    let config = DockerConfig {
        image: Some(payload.image),
        labels: Some(labels),
        env: Some(env),
        // Wrap command in shell to ensure variable expansion and simple parsing works
        cmd: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            payload.startup_command,
        ]),
        host_config: Some(host_config),
        tty: Some(true), // Enable TTY for console access
        open_stdin: Some(true), // Keep stdin open
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    match state.docker.create_container(options, config).await {
        Ok(res) => {
            // Start the container
            if let Err(e) = state
                .docker
                .start_container(&res.id, None::<StartContainerOptions<String>>)
                .await
            {
                eprintln!("Failed to start container: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Ok(Json(res.id))
        }
        Err(e) => {
            eprintln!("Failed to create container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let container_name = format!("yunexal-{}", uuid);

    // Stop container
    let stop_opts = Some(StopContainerOptions { t: 10 });
    // Ignore error if already stopped or not found
    let _ = state.docker.stop_container(&container_name, stop_opts).await;

    // Remove container
    let remove_opts = Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    });

    match state.docker.remove_container(&container_name, remove_opts).await {
        Ok(_) => Ok(Json("deleted".to_string())),
        Err(e) => {
            eprintln!("Failed to delete container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn console_handler(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, uuid))
}

async fn handle_socket(mut socket: WebSocket, state: NodeState, uuid: String) {
    let container_name = format!("yunexal-{}", uuid);

    let options = Some(AttachContainerOptions::<String> {
        stdin: Some(true),
        stdout: Some(true),
        stderr: Some(true),
        stream: Some(true),
        logs: Some(true),
        ..Default::default()
    });

    match state.docker.attach_container(&container_name, options).await {
        Ok(io) => {
            let (mut ws_sender, mut ws_receiver) = socket.split();
            let mut container_output = io.output;
            let mut container_input = io.input;

            // Task to forward container output to WebSocket
            let mut send_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = container_output.next().await {
                    let text = msg.to_string(); 
                    if ws_sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            });

            // Task to forward WebSocket input to container
            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = ws_receiver.next().await {
                    if let Message::Text(text) = msg {
                        if container_input.write_all(text.as_bytes()).await.is_err() {
                            break;
                        }
                    } else if let Message::Close(_) = msg {
                         break;
                    }
                }
            });
            
            tokio::select! {
                _ = (&mut send_task) => recv_task.abort(),
                _ = (&mut recv_task) => send_task.abort(),
            };
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error attaching to container: {}", e).into()))
                .await;
        }
    }
}
