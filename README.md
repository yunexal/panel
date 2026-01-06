## Version: v0.1.2-dev
**Date:** January 06, 2026
**Branch:** 0.1.2-dev

### Yunexal Panel

#### 🌟 New Features
- **Overview Dashboard:** Central landing page with system-wide statistics (Total Nodes, Online Count) and quick settings.
- **Live Logs:** Real-time system log viewer with filtering, auto-scroll, and historical file access.
- **Dynamic Branding:** Configurable Panel Name via UI (persisted to `.env`).
- **Interactive UI:** Added sidebar navigation with toggle (hamburger menu) and improved responsive layout.

#### 🚀 Enhancements
- **Architecture:** Modular codebase structure (`handlers`, `models`, `state`).
- **Performance:** Optimized Redis usage with persistent connection pooling.
- **Reliability:** Graceful degradation when Redis is unavailable.
- **UX:** Installation scripts now include ASCII art branding.

### Yunexal Node

#### 🚀 Enhancements
- **Refactoring:** Modular code structure aligned with Panel architecture.

