# Yunexal Panel 🚀

**Yunexal** is a high-performance, secure, and modular game server management panel written in **Rust**. It leverages modern web technologies to provide a lightning-fast experience with minimal resource overhead.

**Status:** ✅ Completed (**v0.1.3-dev**)

---

## 🔥 Key Features (v0.1.3-dev)

### 🛡️ Robust Node Management
- **Monitoring:** Real-time tracking of **CPU, RAM, and Disk Usage** with live heartbeat updates.
- **Smart Validation:** Prevents assignment of restricted system ports (0-1023) and warns about ephemeral ranges.
- **Self-Healing:** Nodes feature auto-discovery of resources and remote self-update capabilities.
- **Failover:** In-memory fallback system ensures status reporting continues even if Redis is temporarily unavailable.

### ⚡ Performant Architecture
- **Server-Side Rendering:** Uses **Askama** (compiled Jinja-like templates) ensures type safety and zero runtime parse overhead using.
- **Reactive UI:** **HTMX** powers dynamic interactions (polling, live updates, form handling) without complex JavaScript frameworks.
- **Caching:** Dual-layer caching (RAM + Redis) minimizes database queries for high-traffic endpoints.

### 🔒 Security & Allocations
- **Port Management:** Dedicated Allocation system for managing TCP/UDP ports.
- **Token Rotation:** One-click security token rotation for Nodes.
- **Strict Limits:** Configurable CPU, RAM, and Disk limits per node.

---

## 🛠️ Project Structure

- **`/panel`**: The main web interface (Axum web server).
- **`/node`**: The remote agent installed on dedicated servers (System metrics & Container management).

## 🚀 Getting Started

1. **Prerequisites**
   - Rust (latest stable)
   - PostgreSQL
   - (Optional) Redis

2. **Run Panel**
   ```bash
   cd panel
   cargo run
   ```
