use sqlx::postgres::PgPool;
use redis::aio::ConnectionManager;
use reqwest::Client as HttpClient;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: Option<ConnectionManager>,
    pub http_client: HttpClient,
    pub panel_name: Arc<RwLock<String>>,
}
