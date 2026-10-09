// Test-only observation in an additional restricted preload; the real preload is unchanged.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { randomUUID } = require('node:crypto');

function createSandboxObserver({ ipcMain, session, expectedUrl, onFailure }) {
    if (typeof onFailure !== 'function') throw new TypeError('Observer requires an early failure handler');
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'pentimento-renderer-observer-'));
    const filePath = path.join(directory, 'observe.cjs');
    const channel = `pentimento-runtime-sandbox-${randomUUID()}`;
    fs.writeFileSync(filePath, `require('electron').ipcRenderer.send(${JSON.stringify(channel)}, {sandboxed: process.sandboxed === true, contextIsolated: process.contextIsolated === true});\n`);
    let target, settled = false, cleaned = false, preloadId;
    let resolve, reject;
    const observation = new Promise((yes, no) => { resolve = yes; reject = no; });
    // Handle errors before WASM readiness while preserving the original rejected promise.
    observation.catch(onFailure);
    const receive = (event, value) => {
        if (settled || !target || event.sender !== target || !event.senderFrame ||
            event.senderFrame !== target.mainFrame || event.senderFrame.url !== expectedUrl) return;
        settled = true;
        ipcMain.removeListener(channel, receive);
        if (!value || typeof value.sandboxed !== 'boolean' || typeof value.contextIsolated !== 'boolean') {
            reject(new Error('Malformed production renderer sandbox observation'));
            return;
        }
        resolve({ sandboxed: value.sandboxed, contextIsolated: value.contextIsolated,
            webContentsId: target.id, frameProcessId: event.senderFrame.processId,
            frameRoutingId: event.senderFrame.routingId, url: event.senderFrame.url });
    };
    ipcMain.on(channel, receive);
    function cleanup() {
        if (cleaned) return;
        cleaned = true;
        ipcMain.removeListener(channel, receive);
        try {
            if (preloadId !== undefined) session.unregisterPreloadScript(preloadId);
        } finally {
            fs.rmSync(directory, { recursive: true, force: true });
        }
    }
    try {
        preloadId = session.registerPreloadScript({ type: 'frame', filePath });
    } catch (error) {
        cleanup();
        throw error;
    }
    return {
        observation, cleanup,
        bind(contents) {
            if (target) throw new Error('Production observer already bound to a WebContents');
            target = contents;
        },
    };
}
module.exports = { createSandboxObserver };
