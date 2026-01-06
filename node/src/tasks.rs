use sysinfo::System;
use crate::{state::NodeState, models::HeartbeatPayload};

pub async fn start_heartbeat_task(state: NodeState) {
    let client = reqwest::Client::new();
    let mut sys = System::new_all();
    let version = env!("CARGO_PKG_VERSION").to_string();
    
    loop {
        sys.refresh_all();
        
        let cpu_usage = sys.global_cpu_usage();
        let ram_usage = sys.used_memory();
        let ram_total = sys.total_memory();
        let uptime = System::uptime();
        let timestamp = chrono::Utc::now().timestamp_millis();

        let payload = HeartbeatPayload {
            node_id: state.node_id.clone(),
            cpu_usage,
            ram_usage,
            ram_total,
            uptime,
            version: version.clone(),
            timestamp,
        };

        // Assuming the panel has an endpoint /api/nodes/{id}/heartbeat
        let url = format!("{}/api/nodes/{}/heartbeat", state.panel_url, state.node_id);
        let current_token = state.token.read().await;

        match client.post(&url)
            .header("Authorization", format!("Bearer {}", *current_token))
            .json(&payload)
            .send()
            .await 
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    eprintln!("Heartbeat failed with status: {}", resp.status());
                }
            },
            Err(e) => eprintln!("Failed to send heartbeat: {}", e),
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
