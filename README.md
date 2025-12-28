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
