// Utility functions for Yunexal Panel

/**
 * Format bytes to human-readable format
 */
function formatBytes(bytes, decimals = 2) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const dm = decimals < 0 ? 0 : decimals;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

/**
 * Apply byte formatting to elements with data-format="bytes"
 */
function applyByteFormatting() {
    document.querySelectorAll('[data-format="bytes"]').forEach(el => {
        if (el.dataset.formatted) return;
        const raw = el.innerText;
        const val = parseInt(raw);
        if (!isNaN(val)) {
            el.innerText = formatBytes(val);
            el.dataset.formatted = "true";
            el.title = raw + " bytes";
        }
    });
}

/**
 * Toggle main sidebar visibility
 */
function toggleMainSidebar() {
    const sidebar = document.getElementById('main-sidebar');
    if (sidebar) {
        sidebar.style.display = sidebar.style.display === 'none' ? 'flex' : 'none';
    }
}

/**
 * Update connection status banner
 */
function updateConnectionStatus(isError = false) {
    const banner = document.getElementById('connection-lost-banner');
    if (!banner) return;

    if (navigator.onLine && !isError) {
        banner.style.display = 'none';
    } else {
        banner.style.display = 'block';
    }
}

/**
 * Update active sidebar link based on current path
 */
function updateActiveSidebarLink() {
    const path = window.location.pathname;
    document.querySelectorAll('#main-sidebar nav a').forEach(a => {
        const href = a.getAttribute('href');
        a.classList.remove('active');
        if (href === '/' && path === '/') {
            a.classList.add('active');
        } else if (href !== '/' && path.startsWith(href)) {
            a.classList.add('active');
        }
    });
}

// Initialize on page load
document.addEventListener('DOMContentLoaded', function () {
    applyByteFormatting();
    updateActiveSidebarLink();
    updateConnectionStatus();
});

// Connection status listeners
window.addEventListener('online', () => updateConnectionStatus(false));
window.addEventListener('offline', () => updateConnectionStatus(false));
