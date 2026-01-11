use crate::state::NodeState;
use bollard::container::{
    AttachContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions,
};
use futures_util::{Stream, StreamExt};
use node::node_service_server::NodeService;
use node::{ConsoleInput, ConsoleOutput, ContainerRequest, StatsResponse, SuccessResponse};
use std::pin::Pin;
use tokio::io::AsyncWriteExt;
use tonic::{Request, Response, Status};
use tracing::{error, info};

pub mod node {
    tonic::include_proto!("node");
}

pub struct MyNodeService {
    pub state: NodeState,
}

#[tonic::async_trait]
impl NodeService for MyNodeService {
    async fn start_container(
        &self,
        request: Request<ContainerRequest>,
    ) -> Result<Response<SuccessResponse>, Status> {
        let uuid = request.into_inner().uuid;
        let container_name = format!("yunexal-{}", uuid);
        info!("gRPC: Starting container: {}", container_name);

        match self
            .state
            .docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await
        {
            Ok(_) => Ok(Response::new(SuccessResponse {
                message: "Container started successfully".to_string(),
            })),
            Err(e) => {
                error!("gRPC: Failed to start container {}: {}", container_name, e);
                Err(Status::internal(format!(
                    "Failed to start container: {}",
                    e
                )))
            }
        }
    }

    async fn stop_container(
        &self,
        request: Request<ContainerRequest>,
    ) -> Result<Response<SuccessResponse>, Status> {
        let uuid = request.into_inner().uuid;
        let container_name = format!("yunexal-{}", uuid);
        info!("gRPC: Stopping container: {}", container_name);

        let stop_opts = Some(StopContainerOptions { t: 10 });

        match self
            .state
            .docker
            .stop_container(&container_name, stop_opts)
            .await
        {
            Ok(_) => Ok(Response::new(SuccessResponse {
                message: "Container stopped successfully".to_string(),
            })),
            Err(e) => {
                error!("gRPC: Failed to stop container {}: {}", container_name, e);
                Err(Status::internal(format!("Failed to stop container: {}", e)))
            }
        }
    }

    async fn get_container_stats(
        &self,
        request: Request<ContainerRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        let uuid = request.into_inner().uuid;
        let container_name = format!("yunexal-{}", uuid);

        let stream = self.state.docker.stats(
            &container_name,
            Some(StatsOptions {
                stream: false,
                ..Default::default()
            }),
        );
        let stats_vec: Vec<_> = stream.collect().await;

        if let Some(Ok(stats)) = stats_vec.first() {
            let cpu_usage =
                if let (Some(cpu), Some(precpu)) = (&stats.cpu_stats, &stats.precpu_stats) {
                    let cpu_total = cpu
                        .cpu_usage
                        .as_ref()
                        .and_then(|c| c.total_usage)
                        .unwrap_or(0) as f64;
                    let precpu_total = precpu
                        .cpu_usage
                        .as_ref()
                        .and_then(|c| c.total_usage)
                        .unwrap_or(0) as f64;
                    let cpu_delta = cpu_total - precpu_total;
                    let system_delta = (cpu.system_cpu_usage.unwrap_or(0) as f64)
                        - (precpu.system_cpu_usage.unwrap_or(0) as f64);
                    let online_cpus = cpu.online_cpus.unwrap_or(1) as f64;

                    if system_delta > 0.0 && cpu_delta > 0.0 {
                        (cpu_delta / system_delta) * online_cpus * 100.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

            let ram_usage = stats
                .memory_stats
                .as_ref()
                .and_then(|m| m.usage)
                .unwrap_or(0u64);
            let ram_total = stats
                .memory_stats
                .as_ref()
                .and_then(|m| m.limit)
                .unwrap_or(0u64);

            let net_stats = stats.networks.as_ref().and_then(|n| n.values().next());
            let net_rx = net_stats.and_then(|v| v.rx_bytes).unwrap_or(0u64);
            let net_tx = net_stats.and_then(|v| v.tx_bytes).unwrap_or(0u64);

            Ok(Response::new(StatsResponse {
                cpu_usage,
                ram_usage,
                ram_total,
                net_rx,
                net_tx,
            }))
        } else {
            Err(Status::not_found("Stats not available"))
        }
    }

    type StreamConsoleStream = Pin<Box<dyn Stream<Item = Result<ConsoleOutput, Status>> + Send>>;

    async fn stream_console(
        &self,
        request: Request<tonic::Streaming<ConsoleInput>>,
    ) -> Result<Response<Self::StreamConsoleStream>, Status> {
        let mut in_stream = request.into_inner();

        // Wait for first message to get UUID
        let first_msg = in_stream.next().await;
        let uuid = match first_msg {
            Some(Ok(msg)) => msg.uuid,
            _ => return Err(Status::invalid_argument("Missing UUID in first message")),
        };

        let container_name = format!("yunexal-{}", uuid);
        let options = Some(AttachContainerOptions::<String> {
            stdin: Some(true),
            stdout: Some(true),
            stderr: Some(true),
            stream: Some(true),
            logs: Some(true),
            ..Default::default()
        });

        match self
            .state
            .docker
            .attach_container(&container_name, options)
            .await
        {
            Ok(io) => {
                let mut container_output = io.output;
                let mut container_input = io.input;

                // Task to handle input
                tokio::spawn(async move {
                    while let Some(Ok(msg)) = in_stream.next().await {
                        if !msg.data.is_empty() {
                            let _ = container_input.write_all(&msg.data).await;
                        }
                    }
                });

                let output_stream = async_stream::try_stream! {
                    while let Some(Ok(msg)) = container_output.next().await {
                        yield ConsoleOutput { data: msg.to_string().into_bytes() };
                    }
                };

                Ok(Response::new(
                    Box::pin(output_stream) as Self::StreamConsoleStream
                ))
            }
            Err(e) => Err(Status::internal(format!("Docker attach error: {}", e))),
        }
    }
}
