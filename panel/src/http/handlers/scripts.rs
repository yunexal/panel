use axum::{
    extract::{State, Path},
    http::HeaderMap,
};
use crate::{state::AppState, models::Node};

const LOGO: &str = r#"
............................+@@@#+:..............................+%@@@@*............................
...........................@@@@@@@@@@@@#....................+@@@@@@@@@@@@-..........................
...........................@@@@@@@@@@@@@@@+..............-@@@@@@@@@@@@@@@@..........................
...........................@@@@@@@@@@@@@@@@@@..........-@@@@@@@@@@@@@@@@@=..........................
...........................@@@@@@....#@@@@@@@@*.......@@@@@@@@@....@@@@@@...........................
...........................*@@@@@@......@@@@@@@%....%@@@@@@@=.....@@@@@@@...........................
............................@@@@@@@.......@@@@@:..:@@@@@@@-......@@@@@@@............................
.............................@@@@@@@.............@@@@@@@@.......@@@@@@@.............................
..............................@@@@@@@@:........@@@@@@@@-......@@@@@@@@..............................
...............................-@@@@@@@@@@@@@@@@@@@@@+......#@@@@@@@%...............................
.................................*@@@@@@@@@@@@@@@@@%......%@@@@@@@@:................................
...................................:@@@@@@@@@@@@@-......@@@@@@@@@:..................................
........................................+##*-..........+@@@@@@@.....................................
.....................................:#@@@@@@@+........:@@@@@@@@=...................................
..................................=@@@@@@@@@@@@@@@:......+@@@@@@@@-.................................
................................*@@@@@@@@@@@@@@@@@@@:......-@@@@@@@@................................
..............................:@@@@@@@@@%:.=@@@@@@@@@@.......:@@@@@@@*..............................
.............................=@@@@@@@:.........*@@@@@@@*.......+@@@@@@@.............................
............................:@@@@@@*........=....@@@@@@@@:.......@@@@@@#............................
...........................:@@@@@@=......@@@@@@...:@@@@@@@@......:@@@@@@+...........................
...........................@@@@@@=.....@@@@@@@@.....+@@@@@@@@.....:@@@@@@...........................
...........................@@@@@@.:#@@@@@@@@@@........@@@@@@@@@@#:.@@@@@@...........................
..........................:@@@@@@@@@@@@@@@@@:...........@@@@@@@@@@@@@@@@@%..........................
..........................:@@@@@@@@@@@@@@#................-@@@@@@@@@@@@@@@..........................
...........................@@@@@@@@@@@:.......................#@@@@@@@@@@...........................
"#;

pub async fn install_script_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> String {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    // Fetch node to get configured port and token
    let node_result = sqlx::query_as::<_, Node>("SELECT id::uuid, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await;
    
    let (port, token) = match node_result {
        Ok(Some(n)) => (n.port, n.token),
        _ => (3001, "unknown".to_string()),
    };

    format!(r#"#!/bin/bash
# Yunexal Node Installer

cat << "EOF"
{}
EOF

echo "Installing Yunexal Node..."

# 1. Install Docker if not present
if ! command -v docker &> /dev/null; then
    echo "Docker not found. Installing..."
    curl -fsSL https://get.docker.com -o get-docker.sh
    sh get-docker.sh
fi

# 2. Create directory
mkdir -p /opt/yunexal-node
cd /opt/yunexal-node

# 3. Create config.yml
cat <<EOF > config.yml
token: "{}"
node_id: "{}"
panel_url: "http://{}"
port: {}
EOF

# 4. Download and run the node agent
echo "Downloading Node Agent..."
curl -L -o yunexal-node http://{}/downloads/yunexal-node
chmod +x yunexal-node

# 5. Create systemd service
cat <<EOF > /etc/systemd/system/yunexal-node.service
[Unit]
Description=Yunexal Node Agent
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/yunexal-node
ExecStart=/opt/yunexal-node/yunexal-node
Restart=always

[Install]
WantedBy=multi-user.target
EOF

# 6. Start service
systemctl daemon-reload
systemctl enable yunexal-node
systemctl restart yunexal-node

echo "Node installed and started!"
"#, LOGO, token, id, host, port, host)
}

pub async fn uninstall_script_handler(
    Path(id): Path<String>,
    headers: HeaderMap,
) -> String {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");

    format!(r#"#!/bin/bash
cat << "EOF"
{}
EOF
echo "Uninstalling Yunexal Node..."

# Stop and disable service
if systemctl is-active --quiet yunexal-node; then
    systemctl stop yunexal-node
fi

if systemctl is-enabled --quiet yunexal-node; then
    systemctl disable yunexal-node
fi

# Remove service file
rm -f /etc/systemd/system/yunexal-node.service
systemctl daemon-reload

# Remove application directory
rm -rf /opt/yunexal-node

# Notify panel to delete node from database
echo "Notifying panel to remove node..."
curl -X DELETE http://{}/nodes/{}

echo "Node uninstalled successfully."
"#, LOGO, host, id)
}
