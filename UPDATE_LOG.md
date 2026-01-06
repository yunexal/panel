
### [Yunexal Node](https://github.com/yunexal/yunexal-panel/tree/main/node)

#### 🚀 Enhancements
- **Heartbeat Service:**
  - Implemented background task sending system metrics (CPU, RAM, Uptime) every 5 seconds.
  - Includes timestamp for latency tracking.
- **Configuration Management:**
  - Added support for dynamic token updates via API.
  - Automatically updates `config.yml` upon successful token rotation.
  - Thread-safe configuration reloading.
- **System Info:** Added `sysinfo` integration for accurate resource monitoring.
- **Versioning:** Now reports its version (`0.1.1-dev`) to the panel.
