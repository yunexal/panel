use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use bollard::container::{ListContainersOptions, CreateContainerOptions, Config as DockerConfig, StartContainerOptions};
use std::collections::HashMap;
use crate::{state::NodeState, models::CreateContainerRequest};
use uuid::Uuid; // Make sure uuid is in Cargo.toml or use a different random generator

pub async fn list_containers(State(state): State<NodeState>) -> Json<Vec<String>> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    // Filter only containers managed by Yunexal
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);
    
    let options = Some(ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    });

    match state.docker.list_containers(options).await {
        Ok(containers) => {
            let names: Vec<String> = containers.into_iter().map(|c| {
                let name = c.names.unwrap_or_default().first().map(|s| s.to_string()).unwrap_or("unknown".to_string());
                let state = c.state.map(|s| format!("{:?}", s)).unwrap_or_else(|| "unknown".to_string());
                format!("{} [{}]", name, state)
            }).collect();
            Json(names)
        },
        Err(_) => Json(vec!["Error listing containers".to_string()]),
    }
}

pub async fn create_container(
    State(state): State<NodeState>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<String>, StatusCode> {
    let options = Some(CreateContainerOptions {
        name: payload.name.unwrap_or_else(|| format!("yunexal-{}", Uuid::new_v4())),
        platform: None,
    });

    let mut labels = HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());

    let config = DockerConfig {
        image: Some(payload.image),
        labels: Some(labels),
        ..Default::default()
    };

    match state.docker.create_container(options, config).await {
        Ok(res) => {
            // Start the container
            if let Err(e) = state.docker.start_container(&res.id, None::<StartContainerOptions<String>>).await {
                eprintln!("Failed to start container: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Ok(Json(res.id))
        },
        Err(e) => {
            eprintln!("Failed to create container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
