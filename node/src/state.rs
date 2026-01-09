use bollard::Docker;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
#[allow(dead_code)]
pub struct NodeState {
    pub docker: Docker,
    pub token: Arc<RwLock<String>>,
    pub node_id: String,
    pub panel_url: String,
    pub port: u16,
    pub ram_limit: u64,
    pub disk_limit: u64,
}
