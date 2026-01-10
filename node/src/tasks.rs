use sysinfo::{System, Disks};
use crate::{state::NodeState, models::HeartbeatPayload};

pub async fn start_heartbeat_task(state: NodeState) {
    let client = reqwest::Client::new();
    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();
    let mut networks = sysinfo::Networks::new();
    let version = env!("CARGO_PKG_VERSION").to_string();
    
    // Disk I/O Tracking
    let mut prev_read_bytes = 0u64;
    let mut prev_write_bytes = 0u64;
    
    // Network Tracking
    let mut prev_rx_bytes = 0u64;
    let mut prev_tx_bytes = 0u64;
    
    let mut first_run = true;
    
    loop {
        sys.refresh_all();
        disks.refresh(true);
        networks.refresh(true);
        
        let cpu_usage = sys.global_cpu_usage();
        let ram_usage = sys.used_memory();
        let ram_total = sys.total_memory();
        
        let mut disk_total = 0; // Keeping aggregate for backward compatibility
        let mut disk_usage = 0;
        let mut detailed_disks = Vec::new();
        let mut physical_disks: std::collections::HashMap<String, crate::models::DiskDetail> = std::collections::HashMap::new();
        
        for disk in &disks {
            let name_str = disk.name().to_string_lossy().to_string(); // e.g. /dev/sda1
            let mount_str = disk.mount_point().to_string_lossy().to_string();
            
            // 1. Filter: specific types only
            if !name_str.starts_with("/dev/sd") && 
               !name_str.starts_with("/dev/vd") && 
               !name_str.starts_with("/dev/nvme") && 
               mount_str != "/" {
                continue;
            }

            disk_total += disk.total_space();
            disk_usage += disk.total_space() - disk.available_space();
            
            // 2. Resolve Parent Device Name
            let parent_name = if name_str.starts_with("/dev/sd") || name_str.starts_with("/dev/vd") {
                // /dev/sda1 -> /dev/sda (Remove trailing digits)
                name_str.trim_end_matches(char::is_numeric).to_string()
            } else if name_str.starts_with("/dev/nvme") {
                // /dev/nvme0n1p1 -> /dev/nvme0n1 (Remove 'p' + digits at end)
                if let Some(idx) = name_str.rfind('p') {
                    // Check if 'p' is followed only by digits (partition suffix)
                    let suffix = &name_str[idx+1..];
                    if suffix.chars().all(char::is_numeric) && idx > 10 { // naive check length
                         name_str[..idx].to_string()
                    } else {
                        name_str.clone()
                    }
                } else {
                    name_str.clone()
                }
            } else {
                 name_str.clone()
            };

            let total = disk.total_space();
            let available = disk.available_space();
            
            let kind_str = match disk.kind() {
                sysinfo::DiskKind::HDD => "HDD",
                sysinfo::DiskKind::SSD => "SSD",
                _ => "Unknown", 
            };
            
            // 3. Aggregate
            physical_disks.entry(parent_name.clone())
                .and_modify(|d| {
                    d.total_space += total;
                    d.available_space += available;
                    // If any partition is root, mark this disk as System
                    if mount_str == "/" {
                        d.name = "System (Default)".to_string(); 
                    }
                })
                .or_insert(crate::models::DiskDetail {
                    name: if mount_str == "/" { "System (Default)".to_string() } else { parent_name }, // Temp name, replaced later by "Disk N"
                    mount_point: "".to_string(), // Not used in aggregated view
                    total_space: total,
                    available_space: available,
                    is_removable: disk.is_removable(),
                    type_: kind_str.to_string(),
                });
        }
        
        // 4. Convert to List and Apply "Disk N" naming for non-system disks
        let mut ext_counter = 0;
        let mut sorted_keys: Vec<String> = physical_disks.keys().cloned().collect();
        sorted_keys.sort(); // Consistent ordering (sda, sdb...)
        
        for key in sorted_keys {
            if let Some(mut d) = physical_disks.remove(&key) {
                 if d.name != "System (Default)" {
                     ext_counter += 1;
                     d.name = format!("Disk {} ({})", ext_counter, key.replace("/dev/", ""));
                 }
                 detailed_disks.push(d);
            }
        }

        // Calculate Disk I/O from /proc/diskstats
        let mut current_read_bytes = 0u64;
        let mut current_write_bytes = 0u64;
        
        if let Ok(content) = tokio::fs::read_to_string("/proc/diskstats").await {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 14 {
                    let name = parts[2];
                    // Filter: sdX, vdX, nvmeXnY (skip partitions like sdX1, nvmeXnYpZ)
                    let is_sd_vd = (name.starts_with("sd") || name.starts_with("vd")) 
                                   && name.chars().last().map_or(false, |c| c.is_alphabetic());
                    let is_nvme = name.starts_with("nvme") && !name.contains("p");
             
                    if is_sd_vd || is_nvme {
                        if let (Ok(r_sec), Ok(w_sec)) = (parts[5].parse::<u64>(), parts[9].parse::<u64>()) {
                           // Sector = 512 bytes usually
                           current_read_bytes += r_sec * 512;
                           current_write_bytes += w_sec * 512;
                        }
                    }
                }
            }
        }
        
        let disk_read_speed = if first_run { 0 } else { (current_read_bytes.saturating_sub(prev_read_bytes)) / 5 };
        let disk_write_speed = if first_run { 0 } else { (current_write_bytes.saturating_sub(prev_write_bytes)) / 5 };
        
        prev_read_bytes = current_read_bytes;
        prev_write_bytes = current_write_bytes;

        // Calculate Network I/O
        let mut current_rx_bytes = 0u64;
        let mut current_tx_bytes = 0u64;

        for (_interface_name, data) in &networks {
            current_rx_bytes += data.total_received();
            current_tx_bytes += data.total_transmitted();
        }

        let net_rx_speed = if first_run { 0 } else { (current_rx_bytes.saturating_sub(prev_rx_bytes)) / 5 };
        let net_tx_speed = if first_run { 0 } else { (current_tx_bytes.saturating_sub(prev_tx_bytes)) / 5 };

        prev_rx_bytes = current_rx_bytes;
        prev_tx_bytes = current_tx_bytes;

        first_run = false;

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
            disk_read: disk_read_speed,
            disk_write: disk_write_speed,
            net_rx: net_rx_speed,
            net_tx: net_tx_speed,
            disks: detailed_disks,
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
