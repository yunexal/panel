use sysinfo::{System, Disks};
use crate::{state::NodeState, models::HeartbeatPayload};

pub async fn start_heartbeat_task(state: NodeState) {
    let client = reqwest::Client::new();
    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();
    let version = env!("CARGO_PKG_VERSION").to_string();
    
    loop {
        sys.refresh_all();
        disks.refresh(true);
        
        let cpu_usage = sys.global_cpu_usage();
        let ram_usage = sys.used_memory();
        let ram_total = sys.total_memory();
        
        let mut disk_total = 0;
        let mut disk_usage = 0;
        for disk in &disks {
            disk_total += disk.total_space();
            disk_usage += disk.total_space() - disk.available_space();
        }

        let uptime = System::uptime();
        let timestamp = chrono::Utc::now().timestamp_millis();

        let payload = HeartbeatPayload {
            node_id: state.node_id.clone(),
            cpu_usage,
            ram_usage,
            ram_total,
            disk_usage,
            disk_total,
            uptime,
            version: version.clone(),
            timestamp,
        };

        // Assuming the panel has an endpoint /nodes/{id}/heartbeat
        let url = format!("{}/nodes/{}/heartbeat", state.panel_url, state.node_id);
        let current_token = state.token.read().await;

        println!("DEBUG: Sending heartbeat to: {}", url);

        match client.post(&url)
            .header("Authorization", format!("Bearer {}", *current_token))
            .json(&payload)
            .send()
            .await 
        {
            Ok(resp) => {
                println!("DEBUG: Heartbeat Response Status: {}", resp.status());
                if !resp.status().is_success() {
                    eprintln!("Heartbeat failed with status: {} | URL: {}", resp.status(), url);
                    let body = resp.text().await.unwrap_or_else(|_| "Failed to read body".to_string());
                    println!("DEBUG: Error Body: {}", body);
                } else {
                    println!("DEBUG: Heartbeat success: {}", resp.status());
                }
            },
            Err(e) => eprintln!("Failed to send heartbeat to {}: {}", url, e),
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
