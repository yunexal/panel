use axum::{
    extract::State,
    Json,
    response::IntoResponse,
};
use crate::state::NodeState;
use serde_json::json;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

pub async fn self_update_handler(
    State(state): State<NodeState>,
) -> impl IntoResponse {
    let panel_url = state.panel_url.clone();
    
    // Spawn the update process in the background so we can return a response immediately
    tokio::spawn(async move {
        println!("Starting background update process...");
        tokio::time::sleep(Duration::from_secs(1)).await; // Give time for response to flush

        if let Err(e) = perform_update(&panel_url).await {
            eprintln!("Update failed: {}", e);
        } else {
            println!("Update successful. Restarting...");
            std::process::exit(0);
        }
    });

    Json(json!({
        "status": "success",
        "message": "Update initiated. Node will restart shortly."
    }))
}

async fn perform_update(panel_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Construct download URL. 
    // Ensure panel_url does not have trailing slash to avoid double slash, 
    // though most browsers/libs handle it.
    let base_url = panel_url.trim_end_matches('/');
    let url = format!("{}/public/yunexal-node", base_url);
    
    println!("Downloading update from: {}", url);

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!("Failed to download update: Status {}", response.status()).into());
    }

    let bytes = response.bytes().await?;
    
    // Determine current executable path
    let current_exe = env::current_exe()?;
    let tmp_exe = current_exe.with_extension("tmp");
    let backup_exe = current_exe.with_extension("bak");

    println!("Writing new binary to {:?}", tmp_exe);
    fs::write(&tmp_exe, bytes)?;

    // Make executable
    let mut perms = fs::metadata(&tmp_exe)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&tmp_exe, perms)?;

    // Rename current to backup
    if current_exe.exists() {
        println!("Backing up current binary to {:?}", backup_exe);
        fs::rename(&current_exe, &backup_exe)?;
    }

    // Rename new to current
    println!("Installing new binary...");
    if let Err(e) = fs::rename(&tmp_exe, &current_exe) {
        // Rollback
        eprintln!("Installation failed, rolling back: {}", e);
        if backup_exe.exists() {
             fs::rename(&backup_exe, &current_exe)?;
        }
        return Err(e.into());
    }
    
    // Cleanup backup? Maybe keep one backup.
    
    println!("Update installed successfully.");
    Ok(())
}
