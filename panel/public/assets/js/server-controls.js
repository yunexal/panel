// Server control functions (start, stop, restart)

/**
 * Send server control command
 * @param {string} serverId - Server UUID
 * @param {string} action - 'start', 'stop', or 'restart'
 */
async function sendServerAction(serverId, action) {
    const button = event.target;
    const originalText = button.textContent;

    button.disabled = true;
    button.textContent = 'Processing...';

    try {
        const response = await fetch(`/servers/${serverId}/${action}`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
            }
        });

        if (response.ok) {
            // Reload page to show updated status
            window.location.reload();
        } else {
            const error = await response.text();
            alert(`Failed to ${action} server: ${error}`);
            button.disabled = false;
            button.textContent = originalText;
        }
    } catch (error) {
        console.error(`Error ${action}ing server:`, error);
        alert(`Error: ${error.message}`);
        button.disabled = false;
        button.textContent = originalText;
    }
}

/**
 * Poll server stats and update display
 * @param {string} serverId - Server UUID
 * @param {number} interval - Poll interval in ms (default 2000)
 */
function startStatsPolling(serverId, interval = 2000) {
    const statsContainer = document.getElementById('server-stats');
    if (!statsContainer) return;

    async function updateStats() {
        try {
            const response = await fetch(`/servers/${serverId}/stats`);
            if (response.ok) {
                const stats = await response.json();
                updateStatsDisplay(stats);
            }
        } catch (error) {
            console.error('Error fetching stats:', error);
        }
    }

    // Initial update
    updateStats();

    // Poll periodically
    const intervalId = setInterval(updateStats, interval);

    // Return cleanup function
    return () => clearInterval(intervalId);
}

/**
 * Update stats display elements
 * @param {Object} stats - Stats object from API
 */
function updateStatsDisplay(stats) {
    const elements = {
        cpu: document.getElementById('stat-cpu'),
        ram: document.getElementById('stat-ram'),
        disk: document.getElementById('stat-disk'),
        network: document.getElementById('stat-network'),
    };

    if (elements.cpu && stats.cpu_usage !== undefined) {
        const cpu = stats.cpu_usage.toFixed(1);
        elements.cpu.textContent = `${cpu}%`;
        const cpuBar = document.getElementById('stat-cpu-bar');
        if (cpuBar) cpuBar.style.width = `${Math.min(100, stats.cpu_usage)}%`;
    }

    if (elements.ram && stats.memory_usage !== undefined && stats.memory_limit !== undefined) {
        const used = formatBytes(stats.memory_usage);
        const total = formatBytes(stats.memory_limit);
        elements.ram.textContent = `${used} / ${total}`;
        const ramPercent = (stats.memory_usage / stats.memory_limit) * 100;
        const ramBar = document.getElementById('stat-ram-bar');
        if (ramBar) ramBar.style.width = `${Math.min(100, ramPercent)}%`;
    }

    if (elements.disk && stats.disk_usage !== undefined) {
        elements.disk.textContent = formatBytes(stats.disk_usage);
    }

    if (elements.network && stats.network_rx !== undefined && stats.network_tx !== undefined) {
        const rx = formatBytes(stats.network_rx);
        const tx = formatBytes(stats.network_tx);
        elements.network.textContent = `↓ ${rx} / ↑ ${tx}`;
    }
}

// Export functions
/**
 * Format bytes to human readable string
 * @param {number} bytes 
 * @returns {string}
 */
function formatBytes(bytes) {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

window.YunexalServer = {
    sendAction: sendServerAction,
    startStatsPolling: startStatsPolling
};
