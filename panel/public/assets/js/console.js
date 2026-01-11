// Server console management using xterm.js

/**
 * Initialize console terminal for a server
 * @param {string} serverId - Server UUID
 * @param {string} nodeIp - Node IP address
 * @param {number} nodePort - Node port
 * @param {string} containerId - Container element ID
 */
function initializeConsole(serverId, nodeIp, nodePort, containerId) {
    const term = new Terminal({
        cursorBlink: true,
        theme: {
            background: '#1e1e1e',
            foreground: '#f8f8f2'
        }
    });

    const container = document.getElementById(containerId);
    if (!container) {
        console.error('Console container not found:', containerId);
        return;
    }

    // Clear placeholder text
    container.innerHTML = '';
    term.open(container);
    term.fit();

    // Dynamic resize support
    window.addEventListener('resize', function () {
        term.fit();
    });

    term.writeln('> Connection via gRPC-web is planned.');
    term.writeln('> WebSockets are now disabled.');

    return {
        terminal: term,
        socket: null,
        disconnect: function () {
        },
        sendMessage: function (data) {
        }
    };
}

// Export for use in templates
window.YunexalConsole = {
    init: initializeConsole
};
