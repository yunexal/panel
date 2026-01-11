use node::node_service_client::NodeServiceClient;
use node::ContainerRequest;
use tonic::transport::Channel;

pub mod node {
    tonic::include_proto!("node");
}

pub struct NodeClient {
    client: NodeServiceClient<Channel>,
}

impl NodeClient {
    pub async fn connect(addr: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let channel = Channel::from_shared(addr)?.connect().await?;
        Ok(Self {
            client: NodeServiceClient::new(channel),
        })
    }

    pub async fn start_container(&mut self, uuid: String) -> Result<String, tonic::Status> {
        let request = tonic::Request::new(ContainerRequest { uuid });
        let response = self.client.start_container(request).await?;
        Ok(response.into_inner().message)
    }

    pub async fn stop_container(&mut self, uuid: String) -> Result<String, tonic::Status> {
        let request = tonic::Request::new(ContainerRequest { uuid });
        let response = self.client.stop_container(request).await?;
        Ok(response.into_inner().message)
    }

    pub async fn get_stats(&mut self, uuid: String) -> Result<node::StatsResponse, tonic::Status> {
        let request = tonic::Request::new(ContainerRequest { uuid });
        let response = self.client.get_container_stats(request).await?;
        Ok(response.into_inner())
    }

    /*
    pub async fn stream_console(
        &mut self,
        uuid: String,
        mut in_stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    ) -> Result<impl Stream<Item = Result<ConsoleOutput, tonic::Status>>, tonic::Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);

        // Send UUID in first message
        let _ = tx
            .send(ConsoleInput {
                uuid: uuid.clone(),
                data: vec![],
            })
            .await;

        tokio::spawn(async move {
            use futures_util::StreamExt;
            while let Some(data) = in_stream.next().await {
                if tx
                    .send(ConsoleInput {
                        uuid: String::new(),
                        data,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let out_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let response = self.client.stream_console(out_stream).await?;
        Ok(response.into_inner())
    }
    */
}
