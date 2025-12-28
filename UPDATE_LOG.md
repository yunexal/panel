# Update Log - Yunexal

## Version: v0.1.0-dev
**Date:** December 27, 2025
**Author:** nestor_churin

### 🚀 Initial Release Features

#### Core System
- **Central Control Panel:** Rust-based web interface (Axum 0.8) for managing remote nodes.
- **Node Agent:** Lightweight Rust binary for executing commands on remote servers.
- **Database:** PostgreSQL integration for persistent node storage.

#### Node Lifecycle Management
- **Automated Installation:** One-line shell script (`curl | bash`) to set up Docker, download the agent, and configure systemd services.
- **Automated Uninstallation:** Script to clean up services and files, automatically removing the node from the panel database.
- **Node Management:** Web UI to add, edit (Name, IP, Port), and delete nodes.

#### Configuration & Security
- **YAML Configuration:** Node agents are configured via `config.yml`.
- **Token Authentication:** Secure communication between Panel and Nodes using UUID tokens.

#### Networking & Infrastructure
- **Remote Access:** Panel binds to `0.0.0.0` to support external connections.
- **Dynamic Script Generation:** Install scripts automatically adapt to the panel's IP address.
- **Binary Distribution:** Panel serves the Node agent binary directly to remote servers.

#### Docker Integration
- **Container Management:** Foundation laid for Docker container orchestration using `bollard`.
- **Managed Containers:** Nodes automatically tag containers with `yunexal.managed=true`.
## Version: v0.1.1-dev
**Date:** December 28, 2025
**Branch:** 0.1.1-dev

### [Yunexal Panel](https://github.com/yunexal/yunexal-panel)

#### 🌟 New Features
- **Real-time Monitoring:**
  - Added heartbeat system to receive CPU, RAM, and Uptime stats from nodes.
  - Integrated Redis for high-performance storage of transient node stats.
  - Added latency (ping) calculation and display.
- **Dashboard Improvements:**
  - **Status Indicators:** Visual Green/Red dots for Online/Offline status.
  - **Node Versioning:** Displays the running version of the connected node agent.
  - **Footer:** Added panel version and page execution time metrics.
- **Security:**
  - **Token Rotation:** Implemented a secure, one-click mechanism to rotate node authentication tokens without downtime.
  - Updated authentication to use standard `Authorization: Bearer` headers.
