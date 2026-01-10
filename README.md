# Yunexal Panel

Yunexal Panel is a self-hosted server management platform focused on scalability, performance, and automation.  
The project is designed as a modern alternative to existing orchestration panels, with an emphasis on network-first architecture and minimal operational overhead.

---

## Project Status

⚠️ **Pre-Alpha**

Yunexal Panel is currently in an early **pre-alpha** stage.

The core architecture is under active development. APIs, internal workflows, and configuration formats may change without notice.

**The panel is not ready for production use.**

---

## Important Notice

🚫 **Do not install the panel yet**

Although a significant portion of the Panel UI and backend logic is implemented, **Yunexal Node is not yet capable of fully automated container provisioning**.

At this stage:
- Container deployment workflows are incomplete
- Installation and bootstrap logic is still evolving
- Servers may require manual intervention or fail to deploy

Installation instructions will be provided once the Node reaches a stable deployment state.

---

## Architecture Overview

Yunexal consists of two primary components:

### Yunexal Panel
- Central management interface
- Runtime (Nest) and Image (Egg) management
- Server lifecycle orchestration
- Monitoring and cluster overview
- Built with **axum + htmx** to minimize client-side complexity

### Yunexal Node
- Runs on managed host machines
- Responsible for container lifecycle and monitoring
- Handles Docker containers, networking, and resource metrics
- Communicates with the Panel via authenticated APIs and WebSockets

---

## Current Feature Set

### Panel
- Runtime management with full CRUD
- Image creation and native Pterodactyl Eggs import
- Dynamic allocation handling
- Cluster-wide resource monitoring
- HTMX-driven UI with minimal client-side logic
- Configurable UI font via `.env`

### Node
- Partial Docker container lifecycle handling
- WebSocket-based console access
- CPU, memory, disk, and network monitoring
- Smart disk grouping and detailed disk statistics
- Connection state detection and global status indicators

---

## Design Decisions and Differentiators

Yunexal Panel is built with a fundamentally different architectural approach compared to Pterodactyl and similar platforms.  
Even in its pre-alpha state, several core design decisions already distinguish Yunexal.

### Network-First Architecture
- Designed around **private internal networking**, not public node exposure.
- Architecture prepared for **WireGuard-based full-mesh** communication between Panel, Database, and Nodes.
- Internal services are not assumed to be reachable over the public internet.

### Backend-Driven UI
- Server-rendered UI powered by **axum + htmx**.
- No SPA frameworks or heavy frontend state management.
- Predictable behavior with lower client-side complexity.

### File Transfer Model
- Architecture prepared for **HTTP streaming using TAR + ZSTD** compression.
- Intended to replace traditional SFTP-based workflows.
- Optimized for high-throughput transfers over private networks.

### Authentication Model
- Planned support for **passwordless authentication**, including biometric-based access.
- Reduced reliance on static login/password credentials.

### Platform Architecture
- Designed to support **multiple asynchronous panels**.
- No hard dependency on a single control plane.
- Better suited for distributed and provider-grade deployments.

### Compatibility Without Lock-In
- **Native support for Pterodactyl Eggs** without conversion or vendor-specific formats.
- Existing ecosystems can be reused without migration tooling.

### Reliability-Oriented Design
- Architecture prepared for **node self-healing and automatic recovery**.
- Focus on minimizing manual intervention after crashes or failures.

---

## Planned Features (Roadmap)

The following features are planned and represent key long-term capabilities:

- WireGuard full-mesh deployment between Panel, Database, and Nodes
- High-speed file transfers via HTTP Stream + TAR + ZSTD
- Passwordless and biometric authentication
- Native Cloudflare API integration for subdomain management
- Multi-panel asynchronous operation
- One-command deployment
- Advanced provider-grade configuration
- Automated node recovery and self-healing

---

## Development Progress

- Overall project completion: **~40%**
- Core architecture: largely implemented
- Remaining major areas:
  - Server lifecycle management
  - Automated deployment
  - Backup and restore
  - Auditing and permissions
  - Additional operational tooling

---

## Contributions

Contributions are welcome.

If you are interested in helping with development, testing, documentation, or design:
- Expect rapid iteration and breaking changes
- Follow existing code style and architectural decisions
- Open issues or pull requests with clear context and rationale

More detailed contribution guidelines will be added as the project stabilizes.

---

## Installation

❌ **Installation is intentionally undocumented at this stage.**

Documentation and deployment guides will be published once automated container provisioning is fully implemented.

---

## Disclaimer

Yunexal Panel is under heavy development.

Breaking changes, incomplete features, and unstable behavior are expected.  
Use only for development, testing, or contribution purposes.
