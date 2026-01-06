use bollard::Docker;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct NodeState {
    pub docker: Docker,
    pub token: Arc<RwLock<String>>,
    pub node_id: String,
    pub panel_url: String,
    pub port: u16,
}
